# One language's long-fixture corpus (issue #74) in one screen: runs its
# corpus tests (and its src/libraries/ tests), then prints the `mct-corpus` report -- file/line/symbol/
# relation totals, the progress-table cells, and heuristic checks that list
# the usual parser bugs (module ending past EOF, locals taken as symbols,
# relations outside their owner, members without a parent, odd names...) --
# so reviewing a parser change doesn't mean reading expected.snap. Full
# cargo output goes to target/script-logs/corpus-report-{test,report}.log.
#
# Usage:
#   scripts\windows\corpus-report.ps1 cpp                    # mct-lang-cpp: tests + report
#   scripts\windows\corpus-report.ps1 cpp --bless            # regenerate expected.snap first
#   scripts\windows\corpus-report.ps1 cpp --update-progress  # also rewrite its counts in
#                                                            # internal/corpus-progress.md
#
# Exit status: 0 when the corpus tests passed, 1 otherwise.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$crate = ''; $bless = $false; $progress = $false
foreach ($a in $argv) {
  switch -CaseSensitive ($a) {
    '--bless' { $bless = $true }
    '--update-progress' { $progress = $true }
    { $_ -in '-h', '--help' } { Show-Help; exit 0 }
    { $_ -like '-*' } { Stop-Usage "unknown argument: $a (see --help)" }
    default {
      if ($crate) { Stop-Usage 'one language at a time' }
      $crate = $a
    }
  }
}
if (-not $crate) { Stop-Usage 'which language? e.g. scripts\windows\corpus-report.ps1 cpp' }
if ($crate -notlike 'mct-*') { $crate = "mct-lang-$crate" }
if (-not (Test-Path -LiteralPath (Get-RepoPath "crates/$crate/tests/corpus.rs"))) {
  Stop-Usage "crates/$crate/tests/corpus.rs not found"
}

$failed = $false
$log = "$LogDir/corpus-report-test.log"
New-Log $log
if ($bless) { $env:MCT_BLESS = '1' }
try {
  $status = Invoke-Logged $log 'cargo' @('test', '-p', $crate, '--test', 'corpus')
} finally {
  Remove-Item Env:MCT_BLESS -ErrorAction SilentlyContinue
}
# Library-pattern tests (src/libraries/<name>/) read the same corpus but are
# unit tests of the lib target, outside the `corpus` binary.
$libStatus = Invoke-Logged $log 'cargo' @('test', '-p', $crate, '--lib', 'libraries::')
if ($libStatus -ne 0) { $status = $libStatus }
$lines = @(Get-Lines $log)
$c = Get-TestCounts $lines
$counts = '{0} passed, {1} failed' -f $c.Passed, $c.Failed
if ($status -eq 0) {
  $note = ''; if ($bless) { $note = ' (expected.snap blessed)' }
  Write-Host "tests   ok      $counts$note"
} else {
  $failed = $true
  Write-Host "tests   FAILED  $counts  (full log: $log)"
  $errors = Select-WithContext $lines '^error(\[E[0-9]+\])?:' 12 |
    Where-Object { $_ -cnotmatch '^error: (test failed|could not compile)' }
  Write-Capped $errors 40 $log
  $panics = New-Object System.Collections.Generic.List[string]
  $show = $false; $n = 0
  foreach ($line in $lines) {
    if ($line -cmatch '^---- (.*) stdout ----$') {
      $panics.Add("  x $($Matches[1])"); $show = $true; $n = 0; continue
    }
    if ($line -cmatch '^(failures:|---- )') { $show = $false }
    if ($show -and $n -lt 8 -and $line.Trim() -and $line -cnotmatch 'RUST_BACKTRACE') {
      $panics.Add("      $line"); $n++
    }
  }
  Write-Capped $panics 60 $log
}

$log = "$LogDir/corpus-report-report.log"
New-Log $log
if ($progress) { $env:MCT_CORPUS_PROGRESS = Get-RepoPath 'internal/corpus-progress.md' }
try {
  $status = Invoke-Logged $log 'cargo' @('test', '-p', $crate, '--test', 'corpus', 'corpus_report',
    '--', '--ignored', '--nocapture')
} finally {
  Remove-Item Env:MCT_CORPUS_PROGRESS -ErrorAction SilentlyContinue
}
if ($status -eq 0) {
  Write-Host ''
  $on = $false
  foreach ($line in (Get-Lines $log)) {
    if ($line -cmatch '^corpus report:') { $on = $true }
    if ($on -and $line -cmatch '^(test |running |\.$)') { $on = $false }
    if ($on) { Write-Host $line }
  }
} else {
  Write-Host "report  FAILED  (full log: $log)"
  $failed = $true
}
if ($failed) { exit 1 }
