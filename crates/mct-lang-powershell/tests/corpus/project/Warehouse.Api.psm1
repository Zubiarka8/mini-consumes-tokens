#Requires -Version 7.2
<#
.SYNOPSIS
    PowerShell client for the warehouse HTTP API (docs/api/reference.md).

.DESCRIPTION
    Every call goes through Invoke-WarehouseApi, which adds the bearer token,
    a request id and an idempotency key, retries transient failures with
    Invoke-WithRetry and turns RFC 9457 problem details into terminating
    errors with the problem attached.
#>

Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot 'Warehouse.Common.psm1')

$script:BaseUrl = $null
$script:Session = $null

# ---------------------------------------------------------------------------
# Transport
# ---------------------------------------------------------------------------

function Connect-WarehouseApi {
    [CmdletBinding()]
    param(
        [string] $Environment = 'dev'
    )

    $config = Get-WarehouseConfig -Environment $Environment
    $script:BaseUrl = $config.ApiBaseUrl.TrimEnd('/')
    $script:Session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
    $script:Session.Headers['Authorization'] = "Bearer $($config.ApiToken)"
    $script:Session.UserAgent = "warehouse-ops/$($PSVersionTable.PSVersion)"
    $health = Invoke-WarehouseApi -Method Get -Path '/health/ready' -NoRetry
    Write-WarehouseLog -Message "connected to $script:BaseUrl" -Data @{ health = $health.status }
}

function Invoke-WarehouseApi {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [ValidateSet('Get', 'Post', 'Patch', 'Delete')]
        [string] $Method = 'Get',

        [Parameter(Mandatory)]
        [string] $Path,

        [object] $Body,

        [hashtable] $Query = @{},

        [switch] $NoRetry
    )

    if (-not $script:Session) {
        throw 'not connected: run Connect-WarehouseApi first'
    }
    $uri = $script:BaseUrl + $Path
    if ($Query.Count -gt 0) {
        $pairs = foreach ($key in $Query.Keys) {
            foreach ($value in @($Query[$key])) {
                '{0}={1}' -f [uri]::EscapeDataString($key), [uri]::EscapeDataString("$value")
            }
        }
        $uri += '?' + ($pairs -join '&')
    }
    $requestId = [guid]::NewGuid().ToString()
    $params = @{
        Uri                = $uri
        Method             = $Method
        WebSession         = $script:Session
        Headers            = @{ 'X-Request-Id' = $requestId }
        ContentType        = 'application/json'
        StatusCodeVariable = 'status'
        ErrorAction        = 'Stop'
    }
    if ($Method -ne 'Get') {
        $params.Headers['Idempotency-Key'] = $requestId
    }
    if ($null -ne $Body) {
        $params.Body = $Body | ConvertTo-Json -Depth 8 -Compress
    }
    if ($Method -ne 'Get' -and -not $PSCmdlet.ShouldProcess($uri, $Method)) {
        return
    }
    $call = {
        try {
            Invoke-RestMethod @params
        }
        catch {
            ConvertFrom-ProblemError -ErrorRecord $_ -RequestId $requestId
        }
    }
    if ($NoRetry) {
        return & $call
    }
    return Invoke-WithRetry -ScriptBlock $call -Activity "$Method $Path"
}

function ConvertFrom-ProblemError {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [System.Management.Automation.ErrorRecord] $ErrorRecord,
        [Parameter(Mandatory)] [string] $RequestId
    )

    $problem = $null
    if ($ErrorRecord.ErrorDetails -and $ErrorRecord.ErrorDetails.Message) {
        $problem = $ErrorRecord.ErrorDetails.Message | ConvertFrom-Json -ErrorAction SilentlyContinue
    }
    if (-not $problem) {
        throw $ErrorRecord
    }
    $message = "$($problem.status) $($problem.title)"
    if ($problem.PSObject.Properties['detail']) {
        $message += ": $($problem.detail)"
    }
    $exception = [System.InvalidOperationException]::new("$message [request $RequestId]", $ErrorRecord.Exception)
    $exception.Data['Problem'] = $problem
    throw [System.Management.Automation.ErrorRecord]::new($exception, 'WarehouseApiProblem', 'InvalidOperation', $problem)
}

# ---------------------------------------------------------------------------
# Orders
# ---------------------------------------------------------------------------

function Get-WarehouseOrder {
    [CmdletBinding(DefaultParameterSetName = 'ByNumber')]
    param(
        [Parameter(Mandatory, ParameterSetName = 'ByNumber', ValueFromPipeline)]
        [ValidateScript({ Test-OrderNumber $_ })]
        [string] $Number,

        [Parameter(Mandatory, ParameterSetName = 'All')]
        [switch] $All,

        [Parameter(ParameterSetName = 'All')]
        [ValidateSet('NEW', 'RESERVED', 'PAID', 'PICKED', 'SHIPPED', 'INVOICED', 'CANCELLED')]
        [string] $State
    )

    process {
        if ($PSCmdlet.ParameterSetName -eq 'ByNumber') {
            Invoke-WarehouseApi -Path "/orders/$Number"
            return
        }
        $cursor = $null
        do {
            $query = @{ limit = 200 }
            if ($State) { $query.state = $State }
            if ($cursor) { $query.cursor = $cursor }
            $page = Invoke-WarehouseApi -Path '/orders' -Query $query
            $page.items
            $cursor = $page.next_cursor
        } while ($cursor)
    }
}

function New-WarehouseOrder {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)] [string] $CustomerId,
        [Parameter(Mandatory)] [hashtable] $Lines,
        [ValidateSet('standard', 'express', 'pickup')] [string] $Shipping = 'standard'
    )

    $body = @{
        customer_id = $CustomerId
        lines       = @($Lines.GetEnumerator() | Sort-Object Name | ForEach-Object {
                @{ sku = $_.Name; quantity = [int]$_.Value }
            })
        shipping    = @{ method = $Shipping }
    }
    $order = Invoke-WarehouseApi -Method Post -Path '/orders' -Body $body
    if ($order) {
        Write-WarehouseLog -Message "placed order $($order.number)" -Data @{ total = (Format-Money -Amount $order.total.amount -Currency $order.total.currency) }
    }
    return $order
}

function Stop-WarehouseOrder {
    [CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
    param(
        [Parameter(Mandatory, ValueFromPipelineByPropertyName)]
        [Alias('OrderNumber')]
        [string] $Number,

        [Parameter(Mandatory)] [string] $Reason
    )

    process {
        if ($PSCmdlet.ShouldProcess($Number, "cancel ($Reason)")) {
            Invoke-WarehouseApi -Method Post -Path "/orders/$Number/cancel" -Body @{ reason = $Reason } -Confirm:$false
            Write-WarehouseLog -Level Warning -Message "cancelled $Number" -Data @{ reason = $Reason }
        }
    }
}

function Approve-WarehouseOrder {
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory, ValueFromPipelineByPropertyName)] [string] $Number,
        [switch] $Reject,
        [string] $Comment = ''
    )

    process {
        $body = @{ approved = -not $Reject; comment = $Comment }
        Invoke-WarehouseApi -Method Post -Path "/orders/$Number/approve" -Body $body
    }
}

# ---------------------------------------------------------------------------
# Stock and invoices
# ---------------------------------------------------------------------------

function Get-WarehouseStock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory, ValueFromPipeline)]
        [ValidatePattern('^sku-\d{4}')]
        [string[]] $Sku
    )

    begin {
        $all = [System.Collections.Generic.List[string]]::new()
    }
    process {
        $all.AddRange($Sku)
    }
    end {
        foreach ($batch in (Split-Batch -Items $all -Size 50)) {
            Invoke-WarehouseApi -Path '/stock' -Query @{ sku = $batch }
        }
    }
}

function Split-Batch {
    param([object[]] $Items, [int] $Size)

    for ($i = 0; $i -lt $Items.Count; $i += $Size) {
        , $Items[$i..([math]::Min($i + $Size, $Items.Count) - 1)]
    }
}

function Get-WarehouseInvoice {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Number,
        [string] $OutFile
    )

    if ($OutFile) {
        Invoke-WithRetry -Activity "download $Number" -ScriptBlock {
            Invoke-WebRequest -Uri "$script:BaseUrl/invoices/$Number.pdf" -WebSession $script:Session -OutFile $OutFile
        }
        return Get-Item $OutFile
    }
    return Invoke-WarehouseApi -Path "/invoices/$Number"
}

function Get-WarehouseTracking {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory, ValueFromPipelineByPropertyName)]
        [Alias('OrderNumber')]
        [string] $Number
    )

    process {
        $tracking = Invoke-WarehouseApi -Path "/orders/$Number/tracking"
        foreach ($event in $tracking.events) {
            [pscustomobject]@{
                Order   = $Number
                Carrier = $tracking.carrier
                At      = [datetime]$event.at
                Status  = $event.type
            }
        }
    }
}

function Send-InvoiceToErp {
    [CmdletBinding(SupportsShouldProcess)]
    param([Parameter(Mandatory, ValueFromPipeline)] [string] $Number)

    process {
        if ($PSCmdlet.ShouldProcess($Number, 'resend to ERP')) {
            Invoke-WarehouseApi -Method Post -Path "/invoices/$Number/resend"
        }
    }
}

Export-ModuleMember -Function @(
    'Connect-WarehouseApi'
    'Invoke-WarehouseApi'
    'Get-WarehouseOrder'
    'New-WarehouseOrder'
    'Stop-WarehouseOrder'
    'Approve-WarehouseOrder'
    'Get-WarehouseStock'
    'Get-WarehouseInvoice'
    'Get-WarehouseTracking'
    'Send-InvoiceToErp'
)
