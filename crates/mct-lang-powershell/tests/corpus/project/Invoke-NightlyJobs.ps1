#Requires -Version 7.2
<#
.SYNOPSIS
    The warehouse's nightly operations: stale reservations, the reorder
    report, invoice re-sends to the ERP, log rotation and a summary mail.

.DESCRIPTION
    Scheduled at 01:30 Europe/Madrid. Every job runs independently: one
    failing job is reported in the summary and does not stop the others.
    The jobs reuse Warehouse.Api.psm1 and Warehouse.Inventory.psm1.
#>
[CmdletBinding()]
param(
    [ValidateSet('dev', 'staging', 'prod')]
    [string] $Environment = 'prod',

    [string[]] $Only,

    [string] $ReportTo = 'warehouse-team@example.com',

    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Warehouse.Common.psm1')
Import-Module (Join-Path $PSScriptRoot 'Warehouse.Api.psm1')
Import-Module (Join-Path $PSScriptRoot 'Warehouse.Inventory.psm1')
. "$PSScriptRoot/lib/Mail.ps1"

$Results = [System.Collections.Generic.List[pscustomobject]]::new()
$ReorderPoints = @{
    'sku-1001' = 40
    'sku-1002' = 10
    'sku-2001' = 200
    'sku-2101' = 60
    'sku-3001' = 2
    'sku-3101' = 3
}

# ---------------------------------------------------------------------------
# Job runner
# ---------------------------------------------------------------------------

function Invoke-Job {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [scriptblock] $Action
    )

    if ($Only -and $Name -notin $Only) {
        Write-WarehouseLog -Level Debug -Message "skipping job $Name"
        return
    }
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $status = 'ok'
    $detail = ''
    try {
        $detail = & $Action
    }
    catch {
        $status = 'failed'
        $detail = $_.Exception.Message
        Write-WarehouseLog -Level Error -Message "job $Name failed: $detail"
    }
    finally {
        $watch.Stop()
    }
    $Results.Add([pscustomobject]@{
            Job     = $Name
            Status  = $status
            Seconds = [math]::Round($watch.Elapsed.TotalSeconds, 1)
            Detail  = "$detail"
        })
}

function Assert-KnownJob {
    <#
    .SYNOPSIS
        Fails early when -Only names a job that does not exist, instead of
        silently running nothing.
    #>
    param([string[]] $Names)

    $known = @('health', 'stale-reservations', 'invoices', 'reorder-report', 'carriers', 'metrics', 'overdue-shipments', 'logs', 'reports')
    $unknown = @($Names | Where-Object { $_ -notin $known })
    if ($unknown.Count -gt 0) {
        throw "unknown job(s): $($unknown -join ', '); known jobs: $($known -join ', ')"
    }
}

# ---------------------------------------------------------------------------
# Jobs
# ---------------------------------------------------------------------------

function Clear-StaleReservation {
    [CmdletBinding(SupportsShouldProcess)]
    param([int] $OlderThanMinutes = 45)

    $cutoff = (Get-Date).ToUniversalTime().AddMinutes(-$OlderThanMinutes)
    $stale = Get-WarehouseOrder -All -State RESERVED | Where-Object {
        [datetime]$_.reserved_at -lt $cutoff
    }
    $count = 0
    foreach ($order in $stale) {
        if ($PSCmdlet.ShouldProcess($order.number, 'cancel stale reservation')) {
            Stop-WarehouseOrder -Number $order.number -Reason 'reservation expired (nightly sweep)' -Confirm:$false
            $count++
        }
    }
    return "$count stale order(s) cancelled"
}

function Send-PendingInvoice {
    [CmdletBinding(SupportsShouldProcess)]
    param()

    $pending = Invoke-WarehouseApi -Path '/invoices' -Query @{ erp_status = 'failed'; limit = 500 }
    $numbers = @($pending.items | ForEach-Object { $_.number })
    if ($numbers.Count -eq 0) {
        return 'no invoices to re-send'
    }
    $numbers | Send-InvoiceToErp -WhatIf:$DryRun
    return "$($numbers.Count) invoice(s) queued for the ERP batch"
}

function Write-ReorderReport {
    $report = Get-ReorderReport -ReorderPoints $ReorderPoints -AsText
    $path = Join-Path $PSScriptRoot "reports/reorder-$(Get-Date -Format yyyy-MM-dd).txt"
    New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
    Set-Content -Path $path -Value $report -Encoding utf8
    $lines = @($report -split "`n").Count - 2
    return "$lines SKU(s) below their reorder point ($path)"
}

function Compress-OldLog {
    param([int] $KeepDays = 14)

    $logs = Join-Path $PSScriptRoot 'logs'
    if (-not (Test-Path $logs)) {
        return 'no log directory'
    }
    $old = Get-ChildItem -Path $logs -Filter '*.jsonl' | Where-Object {
        $_.LastWriteTime -lt (Get-Date).AddDays(-$KeepDays)
    }
    foreach ($file in $old) {
        $archive = "$($file.FullName).zip"
        Compress-Archive -Path $file.FullName -DestinationPath $archive -Force
        Remove-Item $file.FullName
    }
    return "$(@($old).Count) log file(s) archived"
}

function Test-ApiHealth {
    $ready = Invoke-WarehouseApi -Path '/health/ready' -NoRetry
    $info = Invoke-WarehouseApi -Path '/health/info' -NoRetry
    if ($ready.status -ne 'ok') {
        throw "service not ready: $($ready.status)"
    }
    return "version $($info.version), up since $($info.started_at)"
}

function Test-CarrierEndpoint {
    <#
    .SYNOPSIS
        Checks that every carrier's booking API answers, so a dead carrier is
        known before the first shipment of the morning.
    #>
    $carriers = Invoke-WarehouseApi -Path '/admin/carriers'
    $down = [System.Collections.Generic.List[string]]::new()
    foreach ($carrier in $carriers) {
        try {
            $response = Invoke-WebRequest -Uri $carrier.health_url -Method Head -TimeoutSec 10 -SkipHttpErrorCheck
            if ($response.StatusCode -ge 500) {
                $down.Add("$($carrier.name) ($($response.StatusCode))")
            }
        }
        catch {
            $down.Add("$($carrier.name) (unreachable)")
        }
    }
    if ($down.Count -gt 0) {
        throw "carriers down: $($down -join ', ')"
    }
    return "$(@($carriers).Count) carrier(s) reachable"
}

function Export-DailyMetric {
    [CmdletBinding()]
    param([datetime] $Day = (Get-Date).Date.AddDays(-1))

    $query = @{
        from = $Day.ToString('yyyy-MM-dd')
        to   = $Day.AddDays(1).ToString('yyyy-MM-dd')
    }
    $metrics = Invoke-WarehouseApi -Path '/admin/metrics/daily' -Query $query
    $row = [pscustomobject]@{
        Day            = $Day.ToString('yyyy-MM-dd')
        OrdersPlaced   = $metrics.orders_placed
        OrdersShipped  = $metrics.orders_shipped
        Cancelled      = $metrics.orders_cancelled
        Revenue        = Format-Money -Amount $metrics.revenue.amount -Currency $metrics.revenue.currency
        ShippedIn24h   = '{0:P1}' -f $metrics.shipped_within_24h
    }
    $csv = Join-Path $PSScriptRoot 'reports/daily-metrics.csv'
    $row | Export-Csv -Path $csv -Append -NoTypeInformation -Encoding utf8
    if ($metrics.shipped_within_24h -lt 0.98) {
        Write-WarehouseLog -Level Warning -Message "shipped within 24 h below SLO: $($row.ShippedIn24h)"
    }
    return "metrics for $($row.Day) appended to $csv"
}

function Get-OverdueShipment {
    [CmdletBinding()]
    param([int] $Hours = 24)

    $cutoff = (Get-Date).ToUniversalTime().AddHours(-$Hours)
    $paid = Get-WarehouseOrder -All -State PAID
    $overdue = @($paid | Where-Object { [datetime]$_.paid_at -lt $cutoff })
    if ($overdue.Count -eq 0) {
        return 'no overdue shipments'
    }
    $lines = $overdue | ForEach-Object { "$($_.number) paid $($_.paid_at)" }
    Write-WarehouseLog -Level Warning -Message "overdue shipments:`n$($lines -join "`n")"
    return "$($overdue.Count) order(s) paid more than $Hours h ago and not shipped"
}

function Remove-OldReport {
    param([int] $KeepDays = 90)

    $reports = Join-Path $PSScriptRoot 'reports'
    if (-not (Test-Path $reports)) {
        return 'no report directory'
    }
    $old = Get-ChildItem -Path $reports -Filter 'reorder-*.txt' | Where-Object {
        $_.LastWriteTime -lt (Get-Date).AddDays(-$KeepDays)
    }
    $old | Remove-Item -WhatIf:$DryRun
    return "$(@($old).Count) old report(s) removed"
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

function Send-Summary {
    param([string] $To)

    $failed = @($Results | Where-Object Status -EQ 'failed')
    $subject = if ($failed.Count -gt 0) {
        "[warehouse $Environment] nightly jobs: $($failed.Count) FAILED"
    }
    else {
        "[warehouse $Environment] nightly jobs ok"
    }
    $body = @"
Nightly jobs for $Environment, $(Get-Date -Format 'yyyy-MM-dd HH:mm')

$(Format-Table2 -InputObject $Results -Property 'Job', 'Status', 'Seconds', 'Detail')

Correlation id: $Global:WarehouseCorrelationId
"@
    if ($DryRun) {
        Write-Output $subject
        Write-Output $body
        return
    }
    Send-WarehouseMail -To $To -Subject $subject -Body $body
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if ($Only) {
    Assert-KnownJob -Names $Only
}
Set-WarehouseLogging -Level Information -Path (Join-Path $PSScriptRoot "logs/nightly-$Environment.jsonl")
Connect-WarehouseApi -Environment $Environment

$lock = Lock-WarehouseJob -Name "nightly-$Environment"
if (-not $lock) {
    Write-WarehouseLog -Level Warning -Message 'another nightly run is in progress; exiting'
    exit 0
}

try {
    Invoke-Job -Name 'health' -Action { Test-ApiHealth }
    Invoke-Job -Name 'stale-reservations' -Action { Clear-StaleReservation -WhatIf:$DryRun }
    Invoke-Job -Name 'invoices' -Action { Send-PendingInvoice }
    Invoke-Job -Name 'reorder-report' -Action { Write-ReorderReport }
    Invoke-Job -Name 'carriers' -Action { Test-CarrierEndpoint }
    Invoke-Job -Name 'metrics' -Action { Export-DailyMetric }
    Invoke-Job -Name 'overdue-shipments' -Action { Get-OverdueShipment -Hours 24 }
    Invoke-Job -Name 'logs' -Action { Compress-OldLog -KeepDays 14 }
    Invoke-Job -Name 'reports' -Action { Remove-OldReport -KeepDays 90 }
    Send-Summary -To $ReportTo
}
finally {
    Unlock-WarehouseJob -LockFile $lock
}

$failures = @($Results | Where-Object Status -EQ 'failed').Count
exit $failures
