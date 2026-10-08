# Why a CI run failed, in one screen: the jobs that failed and the first error
# lines of each, instead of the full `gh run view --log-failed` output. The
# full log goes to target\script-logs\ci-failures.log.
#
# Usage:
#   scripts\windows\ci-failures.ps1              # latest failed run on the current branch
#   scripts\windows\ci-failures.ps1 <run-id>     # a specific run

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'needs the GitHub CLI (gh)' }

$MaxLines = 40
$log = 'target\script-logs\ci-failures.log'
New-Item -ItemType Directory -Force -Path 'target\script-logs' | Out-Null

$run = if ($args.Count -ge 1) { $args[0] } else {
  $branch = (git rev-parse --abbrev-ref HEAD)
  gh run list --branch $branch --status failure --limit 1 --json databaseId --jq '.[0].databaseId // empty'
}
if (-not $run) {
  Write-Output "no failed CI run on branch $(git rev-parse --abbrev-ref HEAD)"
  exit 0
}

# gh exits non-zero when the log has nothing to show; the summary below still applies.
$ErrorActionPreference = 'Continue'
gh run view $run --log-failed 2>&1 | Set-Content -LiteralPath $log
$ErrorActionPreference = 'Stop'

Write-Output "CI run $run failed in:"
gh run view $run --json jobs --jq '.jobs[] | select(.conclusion == "failure") | "  - " + .name'

# Error headlines with the line after each, as ci-failures.sh does. gh prefixes
# every log line with job, step and timestamp, and thread ids differ per run.
$lines = Get-Content -LiteralPath $log | ForEach-Object {
  ($_ -split "`t")[-1] -replace '^[0-9T:.-]+Z ', '' -replace '\(\d+\)', ''
}
$pattern = 'error(\[[A-Z0-9]+\])?:|panicked at|assertion .*failed|^test .* FAILED'
$hits = @()
for ($i = 0; $i -lt $lines.Count; $i++) {
  if ($lines[$i] -match $pattern) {
    $hits += $lines[$i].Trim()
    if ($i + 1 -lt $lines.Count) { $hits += $lines[$i + 1].Trim() }
  }
}
$hits = $hits | Select-Object -Unique
if ($hits.Count -eq 0) {
  Write-Output "no error headline matched; read $log"
} else {
  Write-Output 'first errors:'
  $hits | Select-Object -First $MaxLines | ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }
}
