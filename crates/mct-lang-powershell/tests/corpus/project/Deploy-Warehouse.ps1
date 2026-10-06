#Requires -Version 7.2
<#
.SYNOPSIS
    Deploys a release of the warehouse service and watches its canary.

.DESCRIPTION
    Runs the release flow of docs/operations/runbook.md: pre-flight checks,
    database migrations, image rollout with a 5% canary for ten minutes, and
    an automatic rollback when the canary's error rate or p95 latency exceed
    the thresholds derived from the SLOs.

.EXAMPLE
    ./Deploy-Warehouse.ps1 -Environment staging -Version 2026.10.05.1

.EXAMPLE
    ./Deploy-Warehouse.ps1 -Environment prod -Rollback -Reason 'SEV-2 checkout errors'
#>
[CmdletBinding(SupportsShouldProcess, DefaultParameterSetName = 'Deploy')]
param(
    [Parameter(Mandatory)]
    [ValidateSet('staging', 'prod')]
    [string] $Environment,

    [Parameter(Mandatory, ParameterSetName = 'Deploy')]
    [ValidatePattern('^\d{4}\.\d{2}\.\d{2}\.\d+$')]
    [string] $Version,

    [Parameter(Mandatory, ParameterSetName = 'Rollback')]
    [switch] $Rollback,

    [Parameter(ParameterSetName = 'Rollback')]
    [string] $Reason = 'manual rollback',

    [int] $CanaryMinutes = 10,

    [double] $MaxErrorRatio = 1.5,

    [double] $MaxLatencyRatio = 1.2
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Warehouse.Common.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'Warehouse.Api.psm1') -Force
Import-Module Microsoft.PowerShell.SecretManagement -ErrorAction SilentlyContinue
. (Join-Path $PSScriptRoot 'lib/Kubernetes.ps1')

$Namespace = 'warehouse'
$Deployments = @('warehouse-web', 'warehouse-worker', 'warehouse-relay')
$StartedAt = Get-Date

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Invoke-Kubectl {
    [CmdletBinding()]
    param([Parameter(ValueFromRemainingArguments)] [string[]] $Arguments)

    Write-WarehouseLog -Level Debug -Message "kubectl $($Arguments -join ' ')"
    $output = & kubectl --namespace $Namespace @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "kubectl $($Arguments[0]) failed: $output"
    }
    return $output
}

function Get-CurrentVersion {
    $image = Invoke-Kubectl get deployment warehouse-web -o 'jsonpath={.spec.template.spec.containers[0].image}'
    return ($image -split ':')[-1]
}

function Test-Preflight {
    [CmdletBinding()]
    param([string] $TargetVersion)

    $problems = [System.Collections.Generic.List[string]]::new()
    if ((Get-CurrentVersion) -eq $TargetVersion) {
        $problems.Add("version $TargetVersion is already deployed")
    }
    $freeze = Invoke-WarehouseApi -Path '/admin/flags/deploy.frozen'
    if ($freeze.enabled) {
        $problems.Add("deploys are frozen: $($freeze.reason)")
    }
    $ready = Invoke-WarehouseApi -Path '/health/ready' -NoRetry
    if ($ready.status -ne 'ok') {
        $problems.Add("current release is not ready: $($ready.status)")
    }
    foreach ($problem in $problems) {
        Write-WarehouseLog -Level Error -Message "preflight: $problem"
    }
    return $problems.Count -eq 0
}

function Invoke-Migrations {
    [CmdletBinding(SupportsShouldProcess)]
    param([string] $TargetVersion)

    if ($PSCmdlet.ShouldProcess($Environment, "migrate database to $TargetVersion")) {
        Invoke-Kubectl run "migrate-$($TargetVersion.Replace('.', '-'))" --rm -i '--restart=Never' `
            "--image=registry.example.com/warehouse:$TargetVersion" '--' migrate
        Write-WarehouseLog -Message "migrations applied for $TargetVersion"
    }
}

function Set-ImageVersion {
    [CmdletBinding(SupportsShouldProcess)]
    param([string] $TargetVersion, [string[]] $Names = $Deployments)

    foreach ($name in $Names) {
        if ($PSCmdlet.ShouldProcess($name, "set image $TargetVersion")) {
            Invoke-Kubectl set image "deployment/$name" "app=registry.example.com/warehouse:$TargetVersion"
        }
    }
}

function Get-CanaryMetrics {
    param([string] $Track)

    $query = @{
        track  = $Track
        window = '5m'
    }
    $metrics = Invoke-WarehouseApi -Path '/admin/metrics/http' -Query $query
    return [pscustomobject]@{
        ErrorRate = [double]$metrics.error_rate
        P95Ms     = [double]$metrics.p95_ms
    }
}

function Watch-Canary {
    [CmdletBinding()]
    param([int] $Minutes)

    $deadline = (Get-Date).AddMinutes($Minutes)
    $total = $Minutes * 60
    while ((Get-Date) -lt $deadline) {
        $left = [int]($deadline - (Get-Date)).TotalSeconds
        $percent = [int](100 * ($total - $left) / $total)
        Write-Progress -Activity "Canary of $Version" -Status "$left s left" -PercentComplete $percent
        Start-Sleep -Seconds 30
        $stable = Get-CanaryMetrics -Track stable
        $canary = Get-CanaryMetrics -Track canary
        $errorRatio = if ($stable.ErrorRate -gt 0) { $canary.ErrorRate / $stable.ErrorRate } else { 1 }
        $latencyRatio = if ($stable.P95Ms -gt 0) { $canary.P95Ms / $stable.P95Ms } else { 1 }
        Write-WarehouseLog -Message 'canary check' -Data @{ errorRatio = $errorRatio; latencyRatio = $latencyRatio }
        if ($errorRatio -gt $MaxErrorRatio -or $latencyRatio -gt $MaxLatencyRatio) {
            Write-WarehouseLog -Level Error -Message "canary regression: errors x$([math]::Round($errorRatio, 2)), p95 x$([math]::Round($latencyRatio, 2))"
            return $false
        }
    }
    Write-Progress -Activity "Canary of $Version" -Completed
    return $true
}

function Invoke-Rollback {
    [CmdletBinding(SupportsShouldProcess)]
    param([string] $Why)

    foreach ($name in $Deployments) {
        if ($PSCmdlet.ShouldProcess($name, 'rollout undo')) {
            Invoke-Kubectl rollout undo "deployment/$name"
        }
    }
    Invoke-Kubectl rollout status deployment/warehouse-web '--timeout=300s'
    Send-DeployNotification -Text "Rolled back $Environment: $Why"
}

function Send-DeployNotification {
    param([string] $Text)

    $config = Get-WarehouseConfig -Environment $Environment
    if (-not $config.SlackWebhook) {
        Write-WarehouseLog -Level Warning -Message "no Slack webhook configured; $Text"
        return
    }
    $payload = @{ text = $Text } | ConvertTo-Json -Compress
    Invoke-RestMethod -Uri $config.SlackWebhook -Method Post -Body $payload -ContentType 'application/json' | Out-Null
}

function Test-DatabaseBackup {
    <#
    .SYNOPSIS
        Refuses to migrate production without a backup younger than a day.
    #>
    [CmdletBinding()]
    param([int] $MaxAgeHours = 24)

    if ($Environment -ne 'prod') {
        return $true
    }
    $backups = Invoke-WarehouseApi -Path '/admin/backups' -Query @{ limit = 1 }
    $latest = @($backups.items)[0]
    if (-not $latest) {
        Write-WarehouseLog -Level Error -Message 'no database backup found'
        return $false
    }
    $age = (Get-Date).ToUniversalTime() - [datetime]$latest.finished_at
    if ($age.TotalHours -gt $MaxAgeHours) {
        Write-WarehouseLog -Level Error -Message "latest backup is $([int]$age.TotalHours) h old"
        return $false
    }
    Write-WarehouseLog -Message "latest backup $($latest.id) is $([int]$age.TotalHours) h old"
    return $true
}

function Get-ReleaseNote {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $From,
        [Parameter(Mandatory)] [string] $To
    )

    $range = "v$From..v$To"
    $log = & git log '--pretty=format:%h %s' $range 2>$null
    if ($LASTEXITCODE -ne 0) {
        return "(no git history for $range)"
    }
    $groups = $log | Group-Object { ($_ -split ' ', 3)[1] -replace '[:(].*$', '' }
    $sections = foreach ($group in ($groups | Sort-Object Name)) {
        "## $($group.Name)"
        $group.Group | ForEach-Object { "- $_" }
        ''
    }
    return $sections -join [Environment]::NewLine
}

function Write-DeployRecord {
    param(
        [string] $From,
        [string] $To,
        [string] $Outcome
    )

    $record = [ordered]@{
        environment = $Environment
        from        = $From
        to          = $To
        outcome     = $Outcome
        by          = [Environment]::UserName
        at          = (Get-Date).ToUniversalTime().ToString('o')
    }
    $path = Join-Path $PSScriptRoot "logs/deploys-$Environment.jsonl"
    Add-Content -Path $path -Value ($record | ConvertTo-Json -Compress) -Encoding utf8
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Set-WarehouseLogging -Level Information -Path (Join-Path $PSScriptRoot "logs/deploy-$Environment.jsonl")
Connect-WarehouseApi -Environment $Environment

$lock = Lock-WarehouseJob -Name "deploy-$Environment"
if (-not $lock) {
    exit 2
}

try {
    if ($Rollback) {
        Invoke-Rollback -Why $Reason
        exit 0
    }

    if (-not (Test-Preflight -TargetVersion $Version)) {
        throw 'preflight failed'
    }
    if (-not (Test-DatabaseBackup)) {
        throw 'no recent database backup'
    }
    $previous = Get-CurrentVersion
    Write-WarehouseLog -Message "release notes:`n$(Get-ReleaseNote -From $previous -To $Version)"
    Invoke-Migrations -TargetVersion $Version
    Invoke-Kubectl scale deployment warehouse-web-canary '--replicas=1'
    Set-ImageVersion -TargetVersion $Version -Names @('warehouse-web-canary')

    if (Watch-Canary -Minutes $CanaryMinutes) {
        Set-ImageVersion -TargetVersion $Version
        Invoke-Kubectl rollout status deployment/warehouse-web '--timeout=600s'
        Send-DeployNotification -Text "Deployed $Version to $Environment (was $previous)"
        Write-DeployRecord -From $previous -To $Version -Outcome 'deployed'
    }
    else {
        Set-ImageVersion -TargetVersion $previous -Names @('warehouse-web-canary')
        Invoke-Rollback -Why "canary regression of $Version"
        Write-DeployRecord -From $previous -To $Version -Outcome 'rolled back'
        exit 1
    }
}
catch {
    Write-WarehouseLog -Level Error -Message "deploy failed: $($_.Exception.Message)"
    Send-DeployNotification -Text "Deploy of $Version to $Environment FAILED: $($_.Exception.Message)"
    throw
}
finally {
    Invoke-Kubectl scale deployment warehouse-web-canary '--replicas=0'
    Unlock-WarehouseJob -LockFile $lock
    $elapsed = (Get-Date) - $StartedAt
    Write-WarehouseLog -Message "deploy script finished in $([int]$elapsed.TotalSeconds) s"
}
