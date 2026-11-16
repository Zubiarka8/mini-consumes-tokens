# Every open PR in one screen: merge state, base branch when it is not main,
# and the CI checks passed, failed, still running. By default only the PRs that
# need attention (conflicting, blocked, failing or running) are listed. Uses
# one small `gh` query per PR instead of one huge GraphQL query, which times
# out (HTTP 504) on repositories with many PRs; each call is retried.
#
# Usage:
#   scripts\windows\pr-overview.ps1 [--all] [--limit N]
#
# Exit status: 0 nothing failing or conflicting, 1 some PR fails or conflicts,
# 2 usage or tool error. Needs gh.

$argv = @($args)
$ScriptPath = $MyInvocation.MyCommand.Path
. (Join-Path $PSScriptRoot 'lib.ps1')

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { Stop-Usage 'needs the GitHub CLI (gh)' }
$all = $false
$limit = 100
for ($i = 0; $i -lt $argv.Count; $i++) {
  switch ($argv[$i]) {
    '--all' { $all = $true }
    '--limit' { if ($i + 1 -ge $argv.Count) { Stop-Usage '--limit needs a number' }; $i++; $limit = [int]$argv[$i] }
    default { Stop-Usage 'usage: scripts\windows\pr-overview.ps1 [--all] [--limit N]' }
  }
}

# Runs gh up to three times; returns its output lines, or $null when it kept failing.
function Invoke-GhRetry([string[]]$GhArgs) {
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    $result = Invoke-Captured 'gh' $GhArgs
    if ($result.ExitCode -eq 0) { return @($result.Out -split "`n" | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ }) }
    Start-Sleep -Seconds $attempt
  }
  return $null
}

$log = "$LogDir/pr-overview.log"
New-Log $log
$list = Invoke-GhRetry @('pr', 'list', '--state', 'open', '--limit', "$limit", '--json', 'number,headRefName,baseRefName,mergeStateStatus',
  '--jq', '.[] | "\(.number)\t\(.mergeStateStatus)\t\(.baseRefName)\t\(.headRefName)"')
if ($null -eq $list) { Stop-Usage 'could not list the open PRs' }

$bad = $false
$shown = 0
$total = 0
foreach ($row in $list) {
  $parts = $row -split "`t"
  if ($parts.Count -lt 4) { continue }
  $number = $parts[0]; $state = $parts[1]; $base = $parts[2]; $head = $parts[3]
  $total++
  $counts = Invoke-GhRetry @('pr', 'checks', $number, '--json', 'bucket',
    '--jq', '[.[].bucket] | group_by(.) | map("\(.[0])=\(length)") | join(" ")')
  if ($null -eq $counts) { $countText = 'checks unavailable' } else { $countText = ($counts -join ' ') }
  $stack = ''
  if ($base -ne 'main') { $stack = " base=$base" }
  $line = "#$number $state$stack $head [$countText]"
  Add-Utf8Line (Get-RepoPath $log) $line
  if ($state -eq 'DIRTY' -or $countText -match 'fail=') { $bad = $true }
  if ($all -or $state -ne 'CLEAN') {
    Write-Output $line
    $shown++
  }
}
Write-Output "$shown of $total open PR(s) listed; full list in $log"
if ($bad) { exit 1 }
