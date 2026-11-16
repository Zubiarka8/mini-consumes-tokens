# Runs check.ps1 in several worktrees, one after the other, so a batch of PR
# branches is verified without two cargo runs fighting for the CPU. Each
# worktree gets its own Cargo target directory and the CI toolchain, and a
# time limit: a run over the limit is killed (with its child processes) and
# reported, which is how a hanging test shows up instead of blocking the batch.
#
# Usage:
#   scripts\windows\check-many.ps1 [--toolchain <t>] [--timeout <minutes>] <worktree>... [-- <check.ps1 options>]
#
# Defaults: toolchain 1.96.0 (the CI pin), 40 minutes per worktree.
# Output: one line per worktree, and a table in target\script-logs\check-many.md.
# Exit status: 0 every worktree passed, 1 some failed or timed out, 2 usage error.

$argv = @($args)
$ScriptPath = $MyInvocation.MyCommand.Path
. (Join-Path $PSScriptRoot 'lib.ps1')

$toolchain = '1.96.0'
$minutes = 40
$trees = @()
$forward = @()
for ($i = 0; $i -lt $argv.Count; $i++) {
  if ($argv[$i] -eq '--') { $forward = @($argv[($i + 1)..($argv.Count - 1)] | Where-Object { $null -ne $_ }); break }
  switch ($argv[$i]) {
    '--toolchain' { if ($i + 1 -ge $argv.Count) { Stop-Usage '--toolchain needs a value' }; $i++; $toolchain = $argv[$i] }
    '--timeout' { if ($i + 1 -ge $argv.Count) { Stop-Usage '--timeout needs minutes' }; $i++; $minutes = [int]$argv[$i] }
    default {
      if ($argv[$i].StartsWith('-')) { Stop-Usage "unknown option $($argv[$i])" }
      $trees += (Get-AbsPath $argv[$i])
    }
  }
}
if ($trees.Count -eq 0) { Stop-Usage 'usage: scripts\windows\check-many.ps1 [--toolchain T] [--timeout MIN] <worktree>... [-- check.ps1 options]' }

$summary = Join-Path $Root "$LogDir/check-many.md"
[IO.File]::WriteAllText($summary, ("# " + (Get-LogStamp) + "`r`n`r`n| Worktree | Branch | Result |`r`n|---|---|---|`r`n"), $Utf8NoBom)

$failed = $false
foreach ($tree in $trees) {
  $script = Join-Path $tree 'scripts\windows\check.ps1'
  if (-not (Test-Path -LiteralPath $script)) { Stop-Usage "$tree has no scripts\windows\check.ps1" }
  $branch = (Invoke-Captured 'git' @('-C', $tree, 'rev-parse', '--abbrev-ref', 'HEAD')).Out.Trim()
  $logDir = Join-Path $tree 'target\script-logs'
  [void](New-Item -ItemType Directory -Force -Path $logDir)
  $out = Join-Path $logDir 'check-many-run.log'
  Write-Output "== $branch ($tree)"

  $shell = Get-Command powershell.exe -ErrorAction SilentlyContinue
  if (-not $shell) { $shell = Get-Command pwsh -ErrorAction Stop }
  $inner = (ConvertTo-CmdArg $shell.Source) + ' -NoProfile -ExecutionPolicy Bypass -File ' + (ConvertTo-CmdArg $script)
  foreach ($option in $forward) { $inner += ' ' + (ConvertTo-CmdArg $option) }
  $inner += ' > ' + (ConvertTo-CmdArg $out) + ' 2>&1'
  # Through cmd, so the redirection keeps stdout and stderr in order.
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $(if ($env:ComSpec) { $env:ComSpec } else { 'cmd.exe' })
  $psi.Arguments = '/d /s /c "' + $inner + '"'
  $psi.WorkingDirectory = $tree
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.EnvironmentVariables['RUSTUP_TOOLCHAIN'] = $toolchain
  $psi.EnvironmentVariables['CARGO_TARGET_DIR'] = (Join-Path $tree 'target\own-check')
  $process = [Diagnostics.Process]::Start($psi)
  if ($process.WaitForExit($minutes * 60 * 1000)) {
    if ($process.ExitCode -eq 0) { $result = 'passed' } else { $result = "FAILED (exit $($process.ExitCode)), see $out"; $failed = $true }
  } else {
    # taskkill /T ends the whole tree: cmd, PowerShell, cargo and the test binaries.
    [void](Invoke-Captured 'taskkill' @('/F', '/T', '/PID', "$($process.Id)"))
    $result = "timed out after $minutes min (a hanging test? see $out)"
    $failed = $true
  }
  Write-Output "   $result"
  Add-Utf8Line $summary "| $tree | $branch | $result |"
}
Write-Output "table: $summary"
if ($failed) { exit 1 }
