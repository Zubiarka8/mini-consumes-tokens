#Requires -Version 7.2
<#
.SYNOPSIS
    Stock operations for the warehouse back office: cycle counts, transfers,
    reorder reports and the annual stock count freeze.

.DESCRIPTION
    Builds on Warehouse.Api.psm1 for every call to the service and on
    Warehouse.Common.psm1 for logging and formatting. Counts are read from the
    scanners' CSV exports.
#>

Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot 'Warehouse.Common.psm1')
Import-Module (Join-Path $PSScriptRoot 'Warehouse.Api.psm1')

enum StorageZone {
    Ambient
    Chilled
    Frozen
    Bulky
}

class Location {
    [string] $Zone
    [int] $Aisle
    [int] $Level

    Location([string] $code) {
        if ($code -notmatch '^(?<zone>[A-Z]+)-(?<aisle>\d+)-(?<level>\d+)$') {
            throw "bad location: $code"
        }
        $this.Zone = $Matches.zone
        $this.Aisle = [int]$Matches.aisle
        $this.Level = [int]$Matches.level
    }

    [StorageZone] StorageZone() {
        switch -Regex ($this.Zone) {
            '^F' { return [StorageZone]::Chilled }
            '^G' { return [StorageZone]::Frozen }
            '^Z' { return [StorageZone]::Bulky }
        }
        return [StorageZone]::Ambient
    }

    [string] ToString() {
        return '{0}-{1:D2}-{2}' -f $this.Zone, $this.Aisle, $this.Level
    }
}

class CycleCount {
    [string] $Sku
    [Location] $Location
    [int] $Expected
    [int] $Counted
    [string] $Reason

    [int] Difference() {
        return $this.Counted - $this.Expected
    }

    [bool] NeedsReview([int] $tolerance) {
        $diff = $this.Difference()
        return [math]::Abs($diff) -gt $tolerance -or ($diff -ne 0 -and [string]::IsNullOrWhiteSpace($this.Reason))
    }

    [string] Describe() {
        $diff = $this.Difference()
        if ($diff -eq 0) { return "$($this.Sku) at $($this.Location): ok" }
        if ($diff -gt 0) { return "$($this.Sku) at $($this.Location): $diff over" }
        return "$($this.Sku) at $($this.Location): $(-$diff) short ($($this.Reason))"
    }
}

# ---------------------------------------------------------------------------
# Cycle counts
# ---------------------------------------------------------------------------

function Import-CycleCount {
    <#
    .SYNOPSIS
        Reads a scanner export (sku;location;expected;counted;reason) into
        CycleCount objects, skipping and reporting unreadable rows.
    #>
    [CmdletBinding()]
    [OutputType([CycleCount])]
    param(
        [Parameter(Mandatory)]
        [ValidateScript({ Test-Path $_ -PathType Leaf })]
        [string] $Path
    )

    $rows = Import-Csv -Path $Path -Delimiter ';' -Header 'sku', 'location', 'expected', 'counted', 'reason'
    $line = 0
    foreach ($row in $rows) {
        $line++
        try {
            $count = [CycleCount]::new()
            $count.Sku = $row.sku
            $count.Location = [Location]::new($row.location)
            $count.Expected = [int]$row.expected
            $count.Counted = [int]$row.counted
            $count.Reason = $row.reason
            $count
        }
        catch {
            Write-WarehouseLog -Level Warning -Message "skipping line $line of ${Path}: $($_.Exception.Message)"
        }
    }
}

function Split-CycleCount {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory, ValueFromPipeline)] [CycleCount[]] $Count,
        [int] $Tolerance = 2
    )

    begin {
        $review = [System.Collections.Generic.List[CycleCount]]::new()
        $fine = [System.Collections.Generic.List[CycleCount]]::new()
    }
    process {
        foreach ($c in $Count) {
            if ($c.NeedsReview($Tolerance)) { $review.Add($c) } else { $fine.Add($c) }
        }
    }
    end {
        Write-WarehouseLog -Message "counts: $($review.Count) to review, $($fine.Count) fine"
        [pscustomobject]@{ Review = $review; Fine = $fine }
    }
}

function Submit-CycleCount {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory, ValueFromPipeline)] [CycleCount] $Count
    )

    process {
        if ($Count.NeedsReview(0) -and -not $Count.Reason) {
            Write-WarehouseLog -Level Warning -Message "not submitting without a reason: $($Count.Describe())"
            return
        }
        if ($PSCmdlet.ShouldProcess($Count.Describe(), 'submit count')) {
            Invoke-WarehouseApi -Method Post -Path "/stock/$($Count.Sku)/counts" -Body @{
                location = "$($Count.Location)"
                counted  = $Count.Counted
                reason   = $Count.Reason
            }
        }
    }
}

# ---------------------------------------------------------------------------
# Transfers
# ---------------------------------------------------------------------------

function Move-WarehouseStock {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)] [ValidatePattern('^sku-\d{4}')] [string] $Sku,
        [Parameter(Mandatory)] [string] $From,
        [Parameter(Mandatory)] [string] $To,
        [Parameter(Mandatory)] [ValidateRange(1, 99999)] [int] $Quantity
    )

    $source = [Location]::new($From)
    $target = [Location]::new($To)
    if ("$source" -eq "$target") {
        throw 'transfer to the same location'
    }
    if ($source.StorageZone() -ne $target.StorageZone()) {
        Write-WarehouseLog -Level Warning -Message "transfer of $Sku crosses zones: $($source.StorageZone()) -> $($target.StorageZone())"
    }
    $stock = Get-WarehouseStock -Sku $Sku
    if ($stock.available -lt $Quantity) {
        throw "only $($stock.available) of $Sku available"
    }
    if ($PSCmdlet.ShouldProcess("$Quantity x $Sku", "move $source -> $target")) {
        Invoke-WarehouseApi -Method Post -Path "/stock/$Sku/transfers" -Body @{
            from     = "$source"
            to       = "$target"
            quantity = $Quantity
        }
    }
}

function Get-PickingRoute {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string[]] $Location)

    $parsed = $Location | ForEach-Object { [Location]::new($_) }
    $parsed | Sort-Object -Property @(
        @{ Expression = { $_.Zone } }
        @{ Expression = { $_.Aisle } }
        @{ Expression = { if ($_.Aisle % 2 -eq 0) { -$_.Level } else { $_.Level } } }
    ) | ForEach-Object { "$_" }
}

# ---------------------------------------------------------------------------
# Reorder report
# ---------------------------------------------------------------------------

function Get-ReorderReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [hashtable] $ReorderPoints,
        [switch] $AsText
    )

    $skus = @($ReorderPoints.Keys)
    $stock = Get-WarehouseStock -Sku $skus
    $below = foreach ($row in $stock) {
        $point = $ReorderPoints[$row.sku]
        if ($row.available -lt $point) {
            [pscustomobject]@{
                Sku       = $row.sku
                Available = $row.available
                Point     = $point
                Missing   = $point - $row.available
            }
        }
    }
    $sorted = $below | Sort-Object -Property Missing -Descending
    if ($AsText) {
        return Format-Table2 -InputObject $sorted -Property 'Sku', 'Available', 'Point', 'Missing'
    }
    return $sorted
}

function Get-SafetyStock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [int[]] $DailyDemand,
        [Parameter(Mandatory)] [int] $LeadTimeDays,
        [double] $Z = 1.88
    )

    $mean = ($DailyDemand | Measure-Object -Average).Average
    $squares = $DailyDemand | ForEach-Object { [math]::Pow($_ - $mean, 2) }
    $stdDev = [math]::Sqrt(($squares | Measure-Object -Sum).Sum / $DailyDemand.Count)
    $safety = $Z * $stdDev * [math]::Sqrt($LeadTimeDays)
    return [pscustomobject]@{
        Mean         = [math]::Round($mean, 2)
        StdDev       = [math]::Round($stdDev, 2)
        SafetyStock  = [math]::Ceiling($safety)
        ReorderPoint = [math]::Ceiling($mean * $LeadTimeDays + $safety)
    }
}

# ---------------------------------------------------------------------------
# Annual stock count freeze
# ---------------------------------------------------------------------------

function Set-ReservationFreeze {
    [CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
    param(
        [Parameter(Mandatory)] [bool] $Frozen,
        [string] $Reason = 'annual stock count'
    )

    $action = if ($Frozen) { 'freeze' } else { 'unfreeze' }
    if ($PSCmdlet.ShouldProcess('reservations', $action)) {
        Invoke-WarehouseApi -Method Patch -Path '/admin/flags/reservations.frozen' -Body @{ enabled = $Frozen; reason = $Reason }
        Write-WarehouseLog -Level Warning -Message "reservations ${action}d: $Reason"
    }
}

function Invoke-AnnualCount {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)] [string[]] $ExportPath
    )

    Set-ReservationFreeze -Frozen $true
    try {
        $counts = $ExportPath | ForEach-Object { Import-CycleCount -Path $_ }
        $split = $counts | Split-CycleCount -Tolerance 0
        $split.Fine | Submit-CycleCount
        if ($split.Review.Count -gt 0) {
            $report = $split.Review | ForEach-Object { $_.Describe() }
            Write-WarehouseLog -Level Warning -Message "counts needing review:`n$($report -join "`n")"
        }
    }
    finally {
        Set-ReservationFreeze -Frozen $false -Reason 'annual stock count finished'
    }
}

Export-ModuleMember -Function @(
    'Import-CycleCount'
    'Split-CycleCount'
    'Submit-CycleCount'
    'Move-WarehouseStock'
    'Get-ReorderReport'
    'Get-SafetyStock'
    'Set-ReservationFreeze'
    'Invoke-AnnualCount'
)
