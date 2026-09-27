# Reinstalls mct-mcp-server and mct-cli from this checkout into
# %USERPROFILE%\.cargo\bin, makes sure .mct-index\index.sqlite3 matches this
# checkout's schema, and smoke-tests the installed server. Run it after
# pulling, switching branches or merging a PR that changed the server - then
# `/mcp` in Claude Code to reconnect.
#
# Usage:
#   scripts\windows\reinstall.ps1               # install both, fix the index only if needed
#   scripts\windows\reinstall.ps1 --reindex     # also rebuild the index from scratch
#   scripts\windows\reinstall.ps1 --semantic    # server built with --features semantic
#   scripts\windows\reinstall.ps1 --server-only # skip mct-cli
#
# The index is fully derived from source, so deleting it is always safe; it
# is only deleted when --reindex is given or its schema is newer than this
# checkout's (the `migration number that is too high` startup failure).
# Windows locks a running .exe and an open database: if Claude Code is
# connected to the server, disconnect it first (`/mcp`, or quit Claude Code).

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$reindex = $false; $semantic = $false; $cli = $true
foreach ($arg in $argv) {
  switch -CaseSensitive ($arg) {
    '--reindex' { $reindex = $true }
    '--semantic' { $semantic = $true }
    '--server-only' { $cli = $false }
    { $_ -eq '-h' -or $_ -eq '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $arg (see --help)" }
  }
}

$lockHint = '      the file is in use - is Claude Code connected to the server? disconnect it (/mcp, or quit Claude Code) and re-run'

function Install-Crate([string]$Crate, [string[]]$Extra) {
  $log = "$LogDir/reinstall-$Crate.log"
  New-Log $log
  $status = Invoke-Logged $log 'cargo' (@('install', '--locked', '--force', '--path', "crates/$Crate") + @($Extra))
  if ($status -eq 0) {
    $rev = Get-Git @('describe', '--always', '--dirty')
    Write-Host "installed $Crate $rev -> $CargoBin"
    return
  }
  Write-Host "install FAILED for $Crate - full log: $log"
  $lines = @(Get-Lines $log)
  $errors = Select-WithContext $lines '^error' 8
  if (@($errors).Count -gt 0) { Write-Capped $errors 30 $log }
  else { $lines | Select-Object -Last 20 | ForEach-Object { Write-Host $_ } }
  if ($lines | Where-Object { $_ -match 'os error 32|os error 5\b|Access is denied|being used by another process' }) {
    Write-Host $lockHint
  }
  exit 1
}

if ($semantic) { Install-Crate 'mct-mcp-server' @('--features', 'semantic') } else { Install-Crate 'mct-mcp-server' @() }
if ($cli) { Install-Crate 'mct-cli' @() }

$index = Get-RepoPath '.mct-index/index.sqlite3'
$cliBin = Join-Path $CargoBin 'mct-cli.exe'
if (-not (Test-Path -LiteralPath $cliBin -PathType Leaf)) { $cliBin = '' }

if (-not $reindex -and (Test-Path -LiteralPath $index -PathType Leaf) -and $cliBin) {
  $statusLog = "$LogDir/reinstall-status.log"
  New-Log $statusLog
  if ((Invoke-Logged $statusLog $cliBin @('--root', $Root, 'status')) -eq 0) {
    Write-Host 'index ok - schema matches this checkout'
  } elseif (Get-Lines $statusLog | Where-Object { $_ -match 'migration number that is too high' }) {
    Write-Host 'index was migrated by a newer schema - rebuilding it'
    $reindex = $true
  } else {
    Write-Host "WARN  mct-cli status failed - see $statusLog"
  }
}

if ($reindex -or -not (Test-Path -LiteralPath $index -PathType Leaf)) {
  foreach ($f in @($index, "$index-wal", "$index-shm")) {
    if (-not (Test-Path -LiteralPath $f)) { continue }
    try {
      Remove-Item -LiteralPath $f -Force
    } catch {
      Write-Host "index rebuild FAILED - could not delete $f"
      Write-Host $lockHint
      exit 1
    }
  }
  $initLog = "$LogDir/reinstall-init.log"
  New-Log $initLog
  if ($cliBin) {
    $status = Invoke-Logged $initLog $cliBin @('--root', $Root, 'init')
  } else {
    $status = Invoke-Logged $initLog 'cargo' @('run', '-q', '-p', 'mct-cli', '--', '--root', $Root, 'init')
  }
  if ($status -ne 0) { Write-Host "index rebuild FAILED - see $initLog"; exit 1 }
  $complete = Get-Lines $initLog | Where-Object { $_ -match 'complete' } | Select-Object -First 1
  Write-Host "index rebuilt - $complete"
}

& (Join-Path $ScriptsDir 'mcp-smoke.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host 'done - run /mcp in Claude Code to reconnect'
