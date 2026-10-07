#Requires -Version 7.2
<#
.SYNOPSIS
    Shared helpers of the warehouse operations scripts: configuration,
    structured logging, retries and small formatting utilities.

.DESCRIPTION
    Imported by Warehouse.Api.psm1 and Warehouse.Inventory.psm1, and by the
    Deploy-Warehouse.ps1 / Invoke-NightlyJobs.ps1 entry points. Nothing in
    here talks to the network; see Warehouse.Api.psm1 for that.
#>

Set-StrictMode -Version Latest

$script:ModuleRoot = $PSScriptRoot
$script:LogLevel = 'Information'
$script:LogFile = $null
$script:Config = $null
$Global:WarehouseCorrelationId = [guid]::NewGuid().ToString()

$LevelOrder = @{
    Debug       = 0
    Information = 1
    Warning     = 2
    Error       = 3
}

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------

function Set-WarehouseLogging {
    [CmdletBinding()]
    param(
        [ValidateSet('Debug', 'Information', 'Warning', 'Error')]
        [string] $Level = 'Information',

        [string] $Path
    )

    $script:LogLevel = $Level
    if ($Path) {
        $directory = Split-Path -Parent $Path
        if (-not (Test-Path $directory)) {
            New-Item -ItemType Directory -Path $directory -Force | Out-Null
        }
        $script:LogFile = $Path
    }
    $target = if ($Path) { $Path } else { 'console' }
    Write-WarehouseLog -Level Debug -Message "logging at $Level to $target"
}

function Write-WarehouseLog {
    [CmdletBinding()]
    param(
        [ValidateSet('Debug', 'Information', 'Warning', 'Error')]
        [string] $Level = 'Information',

        [Parameter(Mandatory)]
        [string] $Message,

        [hashtable] $Data = @{}
    )

    if ($LevelOrder[$Level] -lt $LevelOrder[$script:LogLevel]) {
        return
    }
    $entry = [ordered]@{
        timestamp     = (Get-Date).ToUniversalTime().ToString('o')
        level         = $Level
        message       = $Message
        correlationId = $Global:WarehouseCorrelationId
    }
    foreach ($key in $Data.Keys) {
        $entry[$key] = $Data[$key]
    }
    $line = $entry | ConvertTo-Json -Compress -Depth 4
    if ($script:LogFile) {
        Add-Content -Path $script:LogFile -Value $line -Encoding utf8
    }
    switch ($Level) {
        'Error' { Write-Error -Message $Message -ErrorAction Continue }
        'Warning' { Write-Warning -Message $Message }
        'Debug' { Write-Debug -Message $Message }
        default { Write-Information -MessageData $Message -InformationAction Continue }
    }
}

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

function Get-WarehouseConfig {
    [CmdletBinding()]
    param(
        [ValidateSet('dev', 'staging', 'prod')]
        [string] $Environment,

        [switch] $Force
    )

    if (-not $Environment) {
        $Environment = if ($env:WAREHOUSE_ENV) { $env:WAREHOUSE_ENV } else { 'dev' }
    }
    if ($script:Config -and -not $Force) {
        return $script:Config
    }
    $path = Join-Path $script:ModuleRoot "config/$Environment.psd1"
    if (-not (Test-Path $path)) {
        throw "no configuration for environment '$Environment' at $path"
    }
    $settings = Import-PowerShellDataFile -Path $path
    $settings.Environment = $Environment
    $settings.ApiToken = Get-WarehouseSecret -Name 'api-token' -Environment $Environment
    $script:Config = [pscustomobject]$settings
    Write-WarehouseLog -Message "loaded $Environment configuration" -Data @{ path = $path }
    return $script:Config
}

function Get-WarehouseSecret {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $Environment
    )

    $variable = "WAREHOUSE_$($Name.ToUpper().Replace('-', '_'))"
    $fromEnv = [Environment]::GetEnvironmentVariable($variable)
    if ($fromEnv) {
        return $fromEnv
    }
    if (Get-Command -Name Get-Secret -ErrorAction SilentlyContinue) {
        return Get-Secret -Name "warehouse-$Environment-$Name" -AsPlainText
    }
    throw "secret '$Name' not found: set `$env:$variable or register a SecretManagement vault"
}

# ---------------------------------------------------------------------------
# Retries
# ---------------------------------------------------------------------------

function Invoke-WithRetry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [scriptblock] $ScriptBlock,

        [ValidateRange(1, 10)]
        [int] $MaxAttempts = 4,

        [int] $InitialDelayMs = 200,

        [string] $Activity = 'operation'
    )

    $attempt = 0
    $delay = $InitialDelayMs
    while ($true) {
        $attempt++
        try {
            return & $ScriptBlock
        }
        catch {
            if ($attempt -ge $MaxAttempts -or -not (Test-TransientError -ErrorRecord $_)) {
                Write-WarehouseLog -Level Error -Message "$Activity failed after $attempt attempt(s): $($_.Exception.Message)"
                throw
            }
            Write-WarehouseLog -Level Warning -Message "$Activity failed (attempt $attempt), retrying in $delay ms"
            Start-Sleep -Milliseconds $delay
            $delay = [math]::Min($delay * 2, 5000)
        }
    }
}

function Test-TransientError {
    [CmdletBinding()]
    [OutputType([bool])]
    param(
        [Parameter(Mandatory)]
        [System.Management.Automation.ErrorRecord] $ErrorRecord
    )

    $exception = $ErrorRecord.Exception
    if ($exception -is [System.Net.Http.HttpRequestException]) {
        return $true
    }
    $status = $exception.Response.StatusCode.value__
    return $status -in 408, 429, 500, 502, 503, 504
}

# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

function Format-Money {
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory, ValueFromPipelineByPropertyName)]
        [decimal] $Amount,

        [Parameter(ValueFromPipelineByPropertyName)]
        [ValidateSet('EUR', 'USD', 'GBP', 'JPY')]
        [string] $Currency = 'EUR'
    )

    $culture = [System.Globalization.CultureInfo]::GetCultureInfo('es-ES')
    $decimals = if ($Currency -eq 'JPY') { 0 } else { 2 }
    $rounded = [math]::Round($Amount, $decimals, [System.MidpointRounding]::ToEven)
    return '{0} {1}' -f $rounded.ToString("N$decimals", $culture), $Currency
}

function ConvertTo-OrderNumber {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [int] $Sequence,
        [int] $Year = (Get-Date).Year
    )

    return 'PED-{0}-{1:D6}' -f $Year, $Sequence
}

function Test-OrderNumber {
    param([string] $Number)

    return $Number -match '^PED-\d{4}-\d{6}$'
}

function Format-Table2 {
    <#
    .SYNOPSIS
        Renders objects as a fixed-width text table for e-mail reports,
        where Format-Table's console layout does not survive.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory, ValueFromPipeline)] [object[]] $InputObject,
        [Parameter(Mandatory)] [string[]] $Property
    )

    $rows = @($InputObject)
    $widths = @{}
    foreach ($name in $Property) {
        $longest = ($rows | ForEach-Object { "$($_.$name)".Length } | Measure-Object -Maximum).Maximum
        $widths[$name] = [math]::Max($name.Length, [int]$longest)
    }
    $header = ($Property | ForEach-Object { $_.PadRight($widths[$_]) }) -join '  '
    $rule = ($Property | ForEach-Object { '-' * $widths[$_] }) -join '  '
    $body = foreach ($row in $rows) {
        ($Property | ForEach-Object { "$($row.$_)".PadRight($widths[$_]) }) -join '  '
    }
    return (@($header, $rule) + $body) -join [Environment]::NewLine
}

# ---------------------------------------------------------------------------
# Locks
# ---------------------------------------------------------------------------

function Lock-WarehouseJob {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Name,
        [int] $StaleAfterMinutes = 120
    )

    $lockFile = Join-Path ([IO.Path]::GetTempPath()) "warehouse-$Name.lock"
    if (Test-Path $lockFile) {
        $age = (Get-Date) - (Get-Item $lockFile).LastWriteTime
        if ($age.TotalMinutes -lt $StaleAfterMinutes) {
            Write-WarehouseLog -Level Warning -Message "job $Name is already running (lock age $([int]$age.TotalMinutes) min)"
            return $null
        }
        Write-WarehouseLog -Level Warning -Message "removing stale lock of $Name"
        Remove-Item $lockFile -Force
    }
    Set-Content -Path $lockFile -Value $PID
    return $lockFile
}

function Unlock-WarehouseJob {
    param([string] $LockFile)

    if ($LockFile -and (Test-Path $LockFile)) {
        Remove-Item $LockFile -Force
    }
}

Export-ModuleMember -Function @(
    'Set-WarehouseLogging'
    'Write-WarehouseLog'
    'Get-WarehouseConfig'
    'Get-WarehouseSecret'
    'Invoke-WithRetry'
    'Test-TransientError'
    'Format-Money'
    'ConvertTo-OrderNumber'
    'Test-OrderNumber'
    'Format-Table2'
    'Lock-WarehouseJob'
    'Unlock-WarehouseJob'
)
