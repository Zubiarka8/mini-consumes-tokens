# Checks issue #74's progress table against GitHub: every row whose status is
# "In PR" or "Done" must name a PR that is merged (Done) or still open (In PR).
# The status is set by hand, so it goes stale as soon as a PR merges and nobody
# edits the row; this makes that visible before picking the next language.
#
# Usage:
#   scripts\windows\corpus-progress-check.ps1          # one line per row, exit 1 if any is stale
#
# Needs the GitHub CLI (`gh`) authenticated for this repository. Offline checks
# of the table's own consistency run as a test in CI (repo_ledgers.rs).

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'needs the GitHub CLI (gh)' }
$progress = 'internal\corpus-progress.md'
if (-not (Test-Path -LiteralPath $progress)) { throw "$progress not found" }

$repo = (gh repo view --json nameWithOwner --jq .nameWithOwner)
$stale = 0
foreach ($line in Get-Content -LiteralPath $progress) {
  if (-not $line.StartsWith('|') -or -not $line.Contains('`mct-lang-')) { continue }
  $cells = $line.Split('|') | ForEach-Object { $_.Trim() }
  $language = $cells[1]
  $status = $cells[3].Trim('*').Trim()
  $pr = $cells[4].TrimStart('#').Trim()
  if ($status -ne 'In PR' -and $status -ne 'Done') { continue }
  if (-not $pr) {
    Write-Output "✗ ${language}: status $status but no PR number"
    $stale++
    continue
  }
  $state = (gh pr view $pr --repo $repo --json state --jq .state)
  if ($status -eq 'In PR' -and $state -eq 'OPEN') { Write-Output "✓ ${language}: PR #$pr is open"; continue }
  if ($status -eq 'Done' -and $state -eq 'MERGED') { Write-Output "✓ ${language}: PR #$pr is merged"; continue }
  if ($status -eq 'In PR' -and $state -eq 'MERGED') {
    Write-Output "✗ ${language}: PR #$pr is merged; set the row to Done"
  } else {
    Write-Output "✗ ${language}: status $status but PR #$pr is $state"
  }
  $stale++
}

if ($stale -gt 0) {
  Write-Output "$stale row(s) out of date in $progress"
  exit 1
}
Write-Output 'all started rows match their PR state'
