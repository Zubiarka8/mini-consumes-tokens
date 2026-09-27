# Starts mct-mcp-server over stdio, sends `initialize` + `tools/list` like an
# MCP client, and reports whether it came up and what it advertises. This is
# the check behind Claude Code's opaque `CONNECTION_CLOSED`: when the server
# dies on startup, the real error (e.g. an index migrated by a newer branch)
# is printed here.
#
# Usage:
#   scripts\windows\mcp-smoke.ps1                     # the installed binary (%USERPROFILE%\.cargo\bin)
#   scripts\windows\mcp-smoke.ps1 --dev               # build and use target\debug
#   scripts\windows\mcp-smoke.ps1 --expect NAME       # also fail unless tool NAME is listed
#   scripts\windows\mcp-smoke.ps1 --list              # also print every tool name
#   scripts\windows\mcp-smoke.ps1 --bin PATH --root DIR
#
# Exit status: 0 when the server answered and every check passed, 1 otherwise.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$bin = Join-Path $CargoBin 'mct-mcp-server.exe'
$root = $Root
$dev = $false; $list = $false; $expect = @()
for ($i = 0; $i -lt $argv.Count; $i++) {
  $arg = $argv[$i]
  $value = $null; if ($i + 1 -lt $argv.Count) { $value = $argv[$i + 1] }
  switch -CaseSensitive ($arg) {
    '--dev' { $dev = $true }
    '--bin' { if (-not $value) { Stop-Usage '--bin needs a path' }; $bin = Get-AbsPath $value; $i++ }
    '--root' { if (-not $value) { Stop-Usage '--root needs a directory' }; $root = Get-AbsPath $value; $i++ }
    '--expect' { if (-not $value) { Stop-Usage '--expect needs a tool name' }; $expect += $value; $i++ }
    '--list' { $list = $true }
    { $_ -eq '-h' -or $_ -eq '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $arg (see --help)" }
  }
}

if ($dev) {
  $buildLog = "$LogDir/smoke-build.log"
  New-Log $buildLog
  if ((Invoke-Logged $buildLog 'cargo' @('build', '-q', '-p', 'mct-mcp-server')) -ne 0) {
    Write-Host "build FAILED - see $buildLog"
    Get-Lines $buildLog | Select-Object -Last 20 | ForEach-Object { Write-Host $_ }
    exit 1
  }
  $bin = Get-RepoPath 'target/debug/mct-mcp-server.exe'
}
if (-not (Test-Path -LiteralPath $bin -PathType Leaf)) {
  Stop-Usage "no server binary at $bin (install it with scripts\windows\reinstall.ps1, or pass --dev)"
}

$out = "$LogDir/smoke-stdout.jsonl"
$err = "$LogDir/smoke-stderr.log"
New-Log $err
# The server answers each request as it arrives and exits once stdin closes.
$requests = @(
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"mcp-smoke","version":"0"}}}'
  '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $bin
$psi.Arguments = '--root ' + (ConvertTo-CmdArg $root)
$psi.WorkingDirectory = $Root
$psi.UseShellExecute = $false
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
$stdout = ''; $stderr = ''
try {
  $p = [Diagnostics.Process]::Start($psi)
  $outTask = $p.StandardOutput.ReadToEndAsync()
  $errTask = $p.StandardError.ReadToEndAsync()
  try {
    $stdin = New-Object System.IO.StreamWriter($p.StandardInput.BaseStream, $Utf8NoBom)
    foreach ($r in $requests) { $stdin.Write("$r`n") }
    $stdin.Close()
  } catch {
    # The server already exited (e.g. a startup error); its stderr says why.
  }
  $p.WaitForExit()
  $stdout = $outTask.Result
  $stderr = $errTask.Result
} catch {
  $stderr = "failed to start ${bin}: $_"
}
[IO.File]::WriteAllText((Get-RepoPath $out), $stdout, $Utf8NoBom)
[IO.File]::AppendAllText((Get-RepoPath $err), $stderr, $Utf8NoBom)

$tools = $null
foreach ($line in ($stdout -split "`r?`n")) {
  if (-not $line.Trim()) { continue }
  try { $msg = $line | ConvertFrom-Json } catch { continue }
  if ($msg.id -eq 2 -and $null -ne $msg.result) { $tools = @($msg.result.tools) }
}

if ($null -eq $tools) {
  Write-Host "server FAILED to answer - binary: $bin"
  $errLines = @(Get-Lines $err)
  Write-Capped ($errLines | Where-Object { $_ -cnotmatch '^# | (INFO|DEBUG|TRACE) ' }) 20 $err
  if ($errLines | Where-Object { $_ -match 'migration number that is too high' }) {
    Write-Host '-> the index was migrated by a newer schema; rebuild it: scripts\windows\reinstall.ps1 --reindex'
  }
  exit 1
}

$names = @($tools | ForEach-Object { $_.name })
$bytes = 0
# A description still equal to server.rs's `#[tool(description = ...)]`
# fallback means tools.ttc's expansion never reached the client.
$fallbacks = @()
foreach ($t in $tools) {
  $d = [string]$t.description
  $bytes += [Text.Encoding]::UTF8.GetByteCount($d)
  if ($d.EndsWith('see tools.ttc')) { $fallbacks += $t.name }
}

$status = 0
Write-Host "server ok - $($names.Count) tool(s), $bytes description bytes ($bin)"
if ($fallbacks.Count -gt 0) {
  Write-Host "WARN  $($fallbacks.Count) tool(s) advertise the server.rs fallback description, not tools.ttc's"
}
foreach ($name in $expect) {
  if ($names -ccontains $name) {
    Write-Host "ok    $name is listed"
  } else {
    Write-Host "FAIL  $name is not listed - is the binary older than your change? (--dev, or scripts\windows\reinstall.ps1)"
    $status = 1
  }
}
if ($list) {
  $names | Sort-Object | ForEach-Object { Write-Host "  $_" }
}
exit $status
