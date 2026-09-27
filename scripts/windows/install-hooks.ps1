# Installs this repo's git hooks from scripts\windows\hooks\ into the hooks
# directory git actually uses (core.hooksPath if set, else .git\hooks).
# Hooks are copied, not symlinked, so they keep working after checking out a
# branch that predates them; re-run this script to update them.
#
# Usage:
#   scripts\windows\install-hooks.ps1              # install / update
#   scripts\windows\install-hooks.ps1 --uninstall  # remove the hooks it installed
#
# A hook of the same name that this script didn't install is left alone and
# reported - merge it by hand. The hooks themselves are sh scripts: Git for
# Windows runs every hook through its bundled bash, whatever shell started
# git. They are written with LF line endings, which that bash requires.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$uninstall = $false
if ($argv.Count -gt 1) { Stop-Usage "unknown argument: $($argv[1]) (see --help)" }
if ($argv.Count -eq 1) {
  switch -CaseSensitive ($argv[0]) {
    '--uninstall' { $uninstall = $true }
    { $_ -eq '-h' -or $_ -eq '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $($argv[0]) (see --help)" }
  }
}

$hooksDir = Get-Git @('rev-parse', '--git-path', 'hooks')
if (-not $hooksDir) { Stop-Usage 'not inside a git checkout' }
if (-not [IO.Path]::IsPathRooted($hooksDir)) { $hooksDir = Get-RepoPath $hooksDir }
[void](New-Item -ItemType Directory -Force -Path $hooksDir)
# Every hook under scripts\windows\hooks\ carries this marker on its second
# line; a hook installed by scripts/unix/install-hooks.sh (e.g. from WSL on
# the same checkout) counts as ours too, and is replaced.
$marker = 'installed by scripts/(windows/install-hooks\.ps1|unix/install-hooks\.sh)'
$status = 0

foreach ($source in Get-ChildItem -LiteralPath (Join-Path $ScriptsDir 'hooks') -File) {
  $target = Join-Path $hooksDir $source.Name
  $existing = $null
  if (Test-Path -LiteralPath $target -PathType Leaf) { $existing = [IO.File]::ReadAllText($target) }
  $ours = ($null -ne $existing) -and ($existing -match $marker)
  if ($uninstall) {
    if ($ours) { Remove-Item -LiteralPath $target -Force; Write-Host "removed   $target" }
    continue
  }
  if ((Test-Path -LiteralPath $target) -and -not $ours) {
    Write-Host "skipped   $target - an existing hook this script didn't install; merge $($source.FullName) into it by hand"
    $status = 1
    continue
  }
  # LF only, whatever core.autocrlf did to the checked-out copy.
  $content = [IO.File]::ReadAllText($source.FullName) -replace "`r`n", "`n"
  if ($ours -and $existing -ceq $content) {
    Write-Host "current   $target"
  } else {
    [IO.File]::WriteAllText($target, $content, $Utf8NoBom)
    Write-Host "installed $target"
  }
}
exit $status
