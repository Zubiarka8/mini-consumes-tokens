# Brings a PR branch up to date: merges its base (the PR's base branch, or main)
# into it, in the worktree where the branch is checked out (a new sibling
# worktree when it is not checked out anywhere). The two append-only ledgers
# that nearly every PR touches, the phase tracker and errors/HISTORY.md, are
# resolved by keeping both sides; any other conflict stops the script with the
# merge left in progress for a manual fix. Never forces anything.
#
# Usage:
#   scripts\windows\pr-sync.ps1 <branch> [--base <branch>] [--push] [--no-fetch]
#
#   --base   merge origin/<branch> instead of the PR's base (default: the base
#            of the PR for <branch> when gh knows it, else main)
#   --push   plain `git push origin HEAD` after a clean merge
#
# Exit status: 0 up to date or merged, 1 conflicts left for a manual fix,
# 2 usage or tool error.

$argv = @($args)
$ScriptPath = $MyInvocation.MyCommand.Path
. (Join-Path $PSScriptRoot 'lib.ps1')

# Files whose conflicts are "both sides added entries": kept as ours then theirs.
$UnionFiles = @('docs/plans/2026-11-10-phase-2-3-tracker.md', 'errors/HISTORY.md')

$branch = ''
$base = ''
$push = $false
$fetch = $true
for ($i = 0; $i -lt $argv.Count; $i++) {
  switch ($argv[$i]) {
    '--base' { if ($i + 1 -ge $argv.Count) { Stop-Usage '--base needs a branch' }; $i++; $base = $argv[$i] }
    '--push' { $push = $true }
    '--no-fetch' { $fetch = $false }
    default {
      if ($argv[$i].StartsWith('-')) { Stop-Usage "unknown option $($argv[$i])" }
      if ($branch) { Stop-Usage 'one branch only' }
      $branch = $argv[$i]
    }
  }
}
if (-not $branch) { Stop-Usage 'usage: scripts\windows\pr-sync.ps1 <branch> [--base <branch>] [--push] [--no-fetch]' }

# Runs git in $Dir; returns @{ ExitCode; Out; Err }.
function Invoke-GitIn([string]$Dir, [string[]]$GitArgs) {
  return Invoke-Captured 'git' (@('-C', $Dir) + $GitArgs)
}

if ($fetch) {
  if ((Invoke-GitIn $Root @('fetch', '-q', 'origin')).ExitCode -ne 0) { Stop-Usage 'git fetch origin failed' }
}
if (-not $base -and (Get-Command gh -ErrorAction SilentlyContinue)) {
  $found = Invoke-Captured 'gh' @('pr', 'view', $branch, '--json', 'baseRefName', '--jq', '.baseRefName')
  if ($found.ExitCode -eq 0) { $base = $found.Out.Trim() }
}
if (-not $base) { $base = 'main' }
if ((Invoke-GitIn $Root @('rev-parse', '--verify', '-q', "origin/$base")).ExitCode -ne 0) { Stop-Usage "origin/$base does not exist" }

# The worktree that has the branch checked out, if any.
$worktree = ''
$current = ''
foreach ($line in ((Invoke-GitIn $Root @('worktree', 'list', '--porcelain')).Out -split "`n")) {
  $line = $line.TrimEnd("`r")
  if ($line.StartsWith('worktree ')) { $current = $line.Substring(9) }
  elseif ($line -eq "branch refs/heads/$branch") { $worktree = $current; break }
}
if (-not $worktree) {
  $worktree = Join-Path (Split-Path -Parent $Root) ((Split-Path -Leaf $Root) + '-sync-' + ($branch -replace '/', '-'))
  if (Test-Path -LiteralPath $worktree) { Stop-Usage "$worktree exists but is not a worktree of $branch" }
  if ((Invoke-GitIn $Root @('rev-parse', '--verify', '-q', "refs/heads/$branch")).ExitCode -eq 0) {
    $added = Invoke-GitIn $Root @('worktree', 'add', '-q', $worktree, $branch)
  } else {
    if ((Invoke-GitIn $Root @('rev-parse', '--verify', '-q', "origin/$branch")).ExitCode -ne 0) { Stop-Usage "no branch $branch, local or on origin" }
    $added = Invoke-GitIn $Root @('worktree', 'add', '-q', '-B', $branch, $worktree, "origin/$branch")
  }
  if ($added.ExitCode -ne 0) { Stop-Usage "git worktree add failed: $($added.Err.Trim())" }
  Write-Output "worktree: $worktree (new)"
} else {
  Write-Output "worktree: $worktree"
}

$gitDir = (Invoke-GitIn $worktree @('rev-parse', '--absolute-git-dir')).Out.Trim()
$mergeHead = Join-Path $gitDir 'MERGE_HEAD'
if (-not (Test-Path -LiteralPath $mergeHead)) {
  if ((Invoke-GitIn $worktree @('status', '--porcelain')).Out.Trim()) { Stop-Usage "$worktree has uncommitted changes" }
  if ((Invoke-GitIn $worktree @('merge-base', '--is-ancestor', "origin/$base", 'HEAD')).ExitCode -eq 0) {
    Write-Output "$branch already contains origin/$base"
  } else {
    [void](Invoke-GitIn $worktree @('merge', '--no-edit', "origin/$base"))
  }
}

# Conflicts: resolve the ledgers, report the rest.
$rest = @()
$conflicted = @((Invoke-GitIn $worktree @('diff', '--name-only', '--diff-filter=U')).Out -split "`n" |
  ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($file in $conflicted) {
  if ($UnionFiles -contains $file) {
    $path = Join-Path $worktree $file
    $kept = New-Object System.Collections.Generic.List[string]
    $inConflict = $false
    $skip = $false
    foreach ($text in [IO.File]::ReadAllLines($path, [Text.Encoding]::UTF8)) {
      if ($text.StartsWith('<<<<<<< ')) { $inConflict = $true; continue }
      if ($inConflict -and $text.StartsWith('||||||| ')) { $skip = $true; continue }
      if ($inConflict -and $text -eq '=======') { $skip = $false; continue }
      if ($inConflict -and $text.StartsWith('>>>>>>> ')) { $inConflict = $false; continue }
      if (-not $skip) { $kept.Add($text) }
    }
    [IO.File]::WriteAllLines($path, $kept.ToArray(), $Utf8NoBom)
    [void](Invoke-GitIn $worktree @('add', $file))
    Write-Output "kept both sides: $file"
  } else {
    $rest += $file
  }
}
if ($rest.Count -gt 0) {
  Write-Output "conflicts to fix by hand in ${worktree}:"
  foreach ($file in $rest) { Write-Output "  $file" }
  exit 1
}
if (Test-Path -LiteralPath $mergeHead) {
  [void](Invoke-GitIn $worktree @('commit', '-q', '--no-edit'))
  $short = (Invoke-GitIn $worktree @('rev-parse', '--short', 'HEAD')).Out.Trim()
  Write-Output "merged origin/$base into ${branch}: $short"
}

if ($push) {
  $pushed = Invoke-GitIn $worktree @('push', 'origin', 'HEAD')
  Write-Output (($pushed.Out + $pushed.Err).Trim().Split("`n")[-1])
}
