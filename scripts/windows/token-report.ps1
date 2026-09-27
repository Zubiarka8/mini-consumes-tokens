# One-screen summary of every token measurement this repo has, for a PR that
# claims to reduce tokens (or to check one didn't grow them):
#   - MCP tool vs grep+read, per language   (mct-cli example token_benchmark)
#   - TOON vs JSON vs the current text      (mct-mcp-server example format_benchmark)
#   - composite tools vs separate calls     (any mct-mcp-server test printing "% fewer",
#                                            e.g. tests/batch.rs)
#   - tool catalog and response totals      (mct-eval)
# Full output of each run goes to target/script-logs/token-report-*.log.
#
# Usage:
#   scripts\windows\token-report.ps1               # everything
#   scripts\windows\token-report.ps1 --no-eval     # skip mct-eval (the slowest part)
#   scripts\windows\token-report.ps1 --markdown F  # also write the summary to F, for a PR body
#
# Exit status: 0 when every measurement ran, 1 if any failed to run.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$runEval = $true; $markdown = ''
for ($i = 0; $i -lt $argv.Count; $i++) {
  switch -CaseSensitive ($argv[$i]) {
    '--no-eval' { $runEval = $false }
    '--markdown' {
      $i++; if ($i -ge $argv.Count) { Stop-Usage '--markdown needs a file' }
      $markdown = Get-AbsPath $argv[$i]
    }
    { $_ -eq '-h' -or $_ -eq '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $($argv[$i]) (see --help)" }
  }
}

$report = Get-RepoPath "$LogDir/token-report.md"
[IO.File]::WriteAllText($report, "<!-- $(Get-LogStamp) -->`n", $Utf8NoBom)
$script:failed = 0
function Out-Report([string]$Text) {
  Write-Host $Text
  Add-Utf8Line $report $Text
}
# `Invoke-Measured <log> <exe> <args>`: runs the command into the log,
# reports a failure; returns whether it succeeded.
function Invoke-Measured([string]$Log, [string]$Exe, [string[]]$Arguments) {
  New-Log $Log
  if ((Invoke-Logged $Log $Exe $Arguments) -eq 0) { return $true }
  $script:failed = 1
  Out-Report "  (failed to run - see $Log)"
  return $false
}
# Whitespace-separated fields of a line, like awk's default $1..$NF.
function Get-Fields([string]$Line) { return ,@($Line.Trim() -split '\s+') }

Out-Report '## MCP tool vs grep+read, per language (3 canonical queries each)'
Out-Report ''
$log = "$LogDir/token-report-languages.log"
if (Invoke-Measured $log 'cargo' @('run', '-q', '-p', 'mct-cli', '--example', 'token_benchmark')) {
  $lang = ''; $lo = 101.0; $hi = -1.0
  $flush = {
    if ($lang) {
      Out-Report ([string]::Format([Globalization.CultureInfo]::InvariantCulture,
          '  {0,-24} {1,5:F1}% - {2,5:F1}% fewer', $lang, $lo, $hi))
    }
  }
  foreach ($line in Get-Lines $log) {
    if ($line -cmatch '^## ') {
      . $flush
      $lang = ($line -replace '^## ', '') -replace ' \(.*$', ''
      $lo = 101.0; $hi = -1.0
      continue
    }
    if ($line -cmatch '%[ ]*\|[ ]*$') {
      $parts = $line.Split('|')
      $r = 0.0
      [void][double]::TryParse(($parts[$parts.Length - 2] -replace '[ %]', ''),
        [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$r)
      if ($r -lt $lo) { $lo = $r }
      if ($r -gt $hi) { $hi = $r }
    }
  }
  . $flush
}

Out-Report ''
Out-Report '## Response formats (format_benchmark, approx tokens)'
Out-Report ''
$log = "$LogDir/token-report-formats.log"
if (Invoke-Measured $log 'cargo' @('run', '-q', '-p', 'mct-mcp-server', '--example', 'format_benchmark')) {
  $name = ''; $json = ''; $toon = ''; $text = ''
  foreach ($line in Get-Lines $log) {
    $f = Get-Fields $line
    $prev = ''; if ($f.Count -ge 2) { $prev = $f[$f.Count - 2] }
    if ($line -cmatch '^== ') { $name = (($f[1..([Math]::Min(3, $f.Count - 1))]) -join ' ') -replace ' ==$', '' }
    elseif ($line -cmatch '^  JSON:') { $json = $prev }
    elseif ($line -cmatch '^  TOON:') { $toon = $prev }
    elseif ($line -cmatch '^  existing text:') { $text = $prev }
    elseif ($line -cmatch '^  reduction vs existing text') {
      Out-Report ('  {0,-26} JSON ~{1,-5}  TOON ~{2,-5}  text ~{3,-5}  (TOON vs text: {4} tokens)' -f $name, $json, $toon, $text, $prev)
    }
  }
}

Out-Report ''
Out-Report '## Composite tools vs the same calls made separately'
Out-Report ''
$tests = @(Get-ChildItem -Path (Get-RepoPath 'crates/mct-mcp-server/tests') -Filter '*.rs' -File -ErrorAction SilentlyContinue |
  Where-Object { Get-Lines $_.FullName | Where-Object { $_.Contains('% fewer') } | Select-Object -First 1 } |
  Sort-Object Name)
if ($tests.Count -eq 0) {
  Out-Report '  (no test prints a "% fewer" measurement)'
}
foreach ($file in $tests) {
  $name = $file.BaseName
  $log = "$LogDir/token-report-test-$name.log"
  if (Invoke-Measured $log 'cargo' @('test', '-q', '-p', 'mct-mcp-server', '--test', $name, '--', '--nocapture', '--test-threads', '1')) {
    # Timings vary run to run; keep only the token figures.
    foreach ($line in Get-Lines $log) {
      if (-not $line.Contains('% fewer')) { continue }
      $line = $line -replace ' in [0-9.]+[\u00B5nm]?s', '' -replace '^[.]+', ''
      Out-Report "  $line"
    }
  }
}

if ($runEval) {
  Out-Report ''
  Out-Report '## Totals (mct-eval suite)'
  Out-Report ''
  $log = "$LogDir/token-report-eval.log"
  # Exit 1 is a quality regression, not a failure to measure.
  New-Log $log
  [void](Invoke-Logged $log 'cargo' @('run', '-q', '-p', 'mct-eval'))
  $lines = @(Get-Lines $log)
  if ($lines | Where-Object { $_.StartsWith('| Tool catalog tokens') }) {
    foreach ($line in $lines) {
      if ($line.StartsWith('| Response tokens')) { Out-Report ('  responses, all cases       {0} tokens' -f ($line.Split('|')[2] -replace ' ', '')) }
      if ($line.StartsWith('| Tool catalog tokens')) { Out-Report ('  tool catalog               {0} tokens' -f ($line.Split('|')[2] -replace ' ', '')) }
    }
  } else {
    $script:failed = 1
    Out-Report "  (failed to run - see $log)"
  }
}

if ($markdown) {
  Copy-Item -LiteralPath $report -Destination $markdown -Force
  Write-Host ''
  Write-Host "wrote $markdown"
}
exit $script:failed
