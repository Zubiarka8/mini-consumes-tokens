# Parses files with this project's parsers without indexing anything and
# prints, per file, its symbol/relation counts or the first syntax error with
# the offending source line -- for finding which construct a tree-sitter
# grammar rejects (write the suspect snippets to separate files, probe them
# all at once). Build output goes to target/script-logs/parse-probe.log.
#
# Usage:
#   scripts\windows\parse-probe.ps1 a.cpp b.cpp c.py
#
# Exit status: 0 when every file parsed, 1 otherwise.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

if ($argv.Count -eq 0) { Stop-Usage 'which files? (see --help)' }
if ($argv[0] -in '-h', '--help') { Show-Help; exit 0 }

$log = "$LogDir/parse-probe.log"
New-Log $log
if ((Invoke-Logged $log 'cargo' @('build', '-p', 'mct-cli')) -ne 0) {
  Write-Host "build   FAILED  (full log: $log)"
  Write-Capped (Select-WithContext (Get-Lines $log) '^error(\[E[0-9]+\])?:' 12) 40 $log
  exit 1
}
$files = @($argv | ForEach-Object { Get-AbsPath $_ })
& (Get-RepoPath 'target/debug/mct-cli.exe') probe @files
if ($LASTEXITCODE -ne 0) { exit 1 }
