# Serves the local project dashboard (`mct-cli web`) and opens it in the
# default browser. Keep it running; Ctrl-C stops the server.
#
# Usage:
#   scripts\windows\web.ps1                    # the installed mct-cli (%USERPROFILE%\.cargo\bin)
#   scripts\windows\web.ps1 --dev              # build and run this checkout (cargo run)
#   scripts\windows\web.ps1 --port N           # use exactly this port (0 = any free port)
#   scripts\windows\web.ps1 --no-obsidian      # do not regenerate the Obsidian vault
#   scripts\windows\web.ps1 --no-open          # print the URL but do not open a browser
#   scripts\windows\web.ps1 --root DIR         # project to serve (default: this checkout)
#
# The output always shows the concrete link: `dashboard: <url>` when it is
# serving, `dashboard: <url> (NOT running)` with the expected link when it is not.
#
# Exit status: 0 on a clean stop, 1 when the server failed to start, 2 on a usage error.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$root = $Root
$port = $null; $dev = $false; $obsidian = $true; $openBrowser = $true
for ($i = 0; $i -lt $argv.Count; $i++) {
  $arg = $argv[$i]
  $value = $null; if ($i + 1 -lt $argv.Count) { $value = $argv[$i + 1] }
  switch -CaseSensitive ($arg) {
    '--dev' { $dev = $true }
    '--port' { if (-not $value) { Stop-Usage '--port needs a number' }; $port = $value; $i++ }
    '--no-obsidian' { $obsidian = $false }
    '--no-open' { $openBrowser = $false }
    '--root' { if (-not $value) { Stop-Usage '--root needs a directory' }; $root = Get-AbsPath $value; $i++ }
    { $_ -eq '-h' -or $_ -eq '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $arg (see --help)" }
  }
}

# The link this project is (or would be) served on: --port, else the port
# remembered in .mct-index/web-port, else the default 8787.
$expPort = $port
if (-not $expPort) {
  $portFile = Join-Path $root '.mct-index/web-port'
  if (Test-Path -LiteralPath $portFile -PathType Leaf) { $expPort = ([IO.File]::ReadAllText($portFile)).Trim() }
}
if (-not $expPort) { $expPort = '8787' }
$expected = if ($expPort -eq '0') { 'http://localhost:<free port>/' } else { "http://localhost:$expPort/" }

$installed = Join-Path $CargoBin 'mct-cli.exe'
$cliArgs = @('--root', $root, 'web')
if ($port) { $cliArgs += @('--port', $port) }
if (-not $obsidian) { $cliArgs += '--no-obsidian' }
if (-not $dev -and (Test-Path -LiteralPath $installed -PathType Leaf)) {
  $exe = $installed
} else {
  $exe = 'cargo'
  $cliArgs = @('run', '-q', '-p', 'mct-cli', '--') + $cliArgs
}

$log = "$LogDir/web.log"
$errLog = "$LogDir/web-stderr.log"
New-Log $log
Set-Content -LiteralPath (Get-RepoPath $errLog) -Value '' -Encoding UTF8
$argLine = ($cliArgs | ForEach-Object { ConvertTo-CmdArg $_ }) -join ' '
$proc = Start-Process -FilePath $exe -ArgumentList $argLine -WorkingDirectory $Root `
  -NoNewWindow -PassThru `
  -RedirectStandardOutput (Get-RepoPath $log) -RedirectStandardError (Get-RepoPath $errLog)

try {
  # `cargo run` may compile first, so allow up to a minute before giving up.
  $url = $null
  for ($n = 0; $n -lt 600 -and -not $url; $n++) {
    $text = ((Get-Lines $log) + (Get-Lines $errLog)) -join "`n"
    if ($text -match 'http://(localhost|127\.0\.0\.1):\d+/?') { $url = $Matches[0]; break }
    if ($proc.HasExited) {
      Write-Host "web FAILED to start - see $log and $errLog"
      Write-Capped ((Get-Lines $log) + (Get-Lines $errLog) | Where-Object { $_ -notmatch '^# ' }) 20 $log
      if ($text -match 'schema is incompatible') {
        Write-Host '-> the index was built by another version; close other MCP clients, then: scripts\windows\reinstall.ps1 --reindex'
      }
      Write-Host "dashboard: $expected (NOT running)"
      exit 1
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $url) { Write-Host "web FAILED: no URL after 60s - see $log"; Write-Host "dashboard: $expected (NOT running)"; exit 1 }

  Write-Host "dashboard: $url   (Ctrl-C to stop)"
  if ($openBrowser) { Start-Process $url }
  $proc.WaitForExit()
  Write-Host "dashboard stopped: $url"
} finally {
  if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}
