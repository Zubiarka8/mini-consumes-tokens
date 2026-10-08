# A new branch in its own worktree, cut from origin/main (or another base), in a
# sibling directory of this checkout, so concurrent work never shares a working
# tree. Fetches origin first. Refuses to reuse an existing branch or path.
#
# Usage:
#   scripts\windows\branch-worktree.ps1 <branch>            # from origin/main
#   scripts\windows\branch-worktree.ps1 <branch> <base>     # from another ref
#
# Prints the directory to cd into. Exit status: 0 on success, 2 on usage or conflict.

$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

if ($argv.Count -lt 1) { Stop-Usage 'usage: scripts\windows\branch-worktree.ps1 <branch> [base]' }
$branch = $argv[0]
$base = if ($argv.Count -ge 2) { $argv[1] } else { 'origin/main' }

function Die([string]$Message) {
  [Console]::Error.WriteLine("error: $Message")
  exit 2
}

if ((Invoke-Captured 'git' @('check-ref-format', '--branch', $branch)).ExitCode -ne 0) { Die "invalid branch name: $branch" }
if ((Invoke-Captured 'git' @('rev-parse', '--verify', '--quiet', "refs/heads/$branch")).ExitCode -eq 0) { Die "branch $branch already exists" }

# <checkout>-<branch with / as ->, next to the checkout.
$path = Join-Path (Split-Path -Parent $Root) ((Split-Path -Leaf $Root) + '-' + ($branch -replace '/', '-'))
if (Test-Path -LiteralPath $path) { Die "$path already exists" }

if ((Invoke-Captured 'git' @('fetch', 'origin', '--quiet')).ExitCode -ne 0) { Die 'git fetch origin failed' }
if ((Invoke-Captured 'git' @('rev-parse', '--verify', '--quiet', $base)).ExitCode -ne 0) { Die "unknown base: $base" }
if ((Invoke-Captured 'git' @('worktree', 'add', '-b', $branch, $path, $base)).ExitCode -ne 0) { Die 'git worktree add failed' }

Write-Output "created branch $branch from $base"
Write-Output "cd `"$path`""
