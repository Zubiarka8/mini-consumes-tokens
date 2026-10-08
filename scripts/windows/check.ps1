# CI-equivalent verification with a summary instead of a wall of output:
# `cargo fmt --check`, `cargo test`, the CI clippy invocation (unwrap/expect/panic
# denied) and the `mct-eval` quality gate. Each step's full output goes to
# target/script-logs/check-<step>.log; the terminal gets one line per step,
# plus only the failing tests / lints / regressions when a step fails. The
# status lines are also kept in target/script-logs/check-summary.md, which
# scripts\windows\pr-body.ps1 reads for a PR's verification section.
#
# Usage:
#   scripts\windows\check.ps1                  # everything, like CI's build-test job
#   scripts\windows\check.ps1 -p mct-index     # fmt + tests + clippy for one crate, no eval
#   scripts\windows\check.ps1 --no-eval        # skip the mct-eval quality gate
#   scripts\windows\check.ps1 --only clippy    # one step: fmt | test | clippy | eval
#
# Exit status: 0 when every step passed, 1 otherwise.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$package = ''
$runFmt = $true; $runTest = $true; $runClippy = $true; $runEval = $true
for ($i = 0; $i -lt $argv.Count; $i++) {
  switch -CaseSensitive ($argv[$i]) {
    { $_ -in '-p', '--package' } {
      $i++; if ($i -ge $argv.Count) { Stop-Usage '-p needs a crate name' }
      $package = $argv[$i]; $runEval = $false
    }
    '--no-eval' { $runEval = $false }
    '--only' {
      $i++; $runFmt = $false; $runTest = $false; $runClippy = $false; $runEval = $false
      switch -CaseSensitive ($(if ($i -lt $argv.Count) { $argv[$i] } else { '' })) {
        'fmt' { $runFmt = $true }
        'test' { $runTest = $true }
        'clippy' { $runClippy = $true }
        'eval' { $runEval = $true }
        default { Stop-Usage '--only takes fmt, test, clippy or eval' }
      }
    }
    { $_ -in '-h', '--help' } { Show-Help; exit 0 }
    default { Stop-Usage "unknown argument: $($argv[$i]) (see --help)" }
  }
}

if ($package) { $scope = @('-p', $package) } else { $scope = @('--workspace') }
$script:failed = 0
$SummaryLog = "$LogDir/check-summary.md"
New-Log $SummaryLog

# One status line: printed for the terminal and kept for the PR body.
function Say {
  param([string]$Text)
  Write-Host $Text
  Add-Utf8Line (Get-RepoPath $SummaryLog) $Text
}

function Step-Fmt {
  $log = "$LogDir/check-fmt.log"
  New-Log $log
  $status = Invoke-Logged $log 'cargo' @('fmt', '--all', '--check')
  if ($status -eq 0) {
    Say 'fmt     ok      no formatting changes'
    return
  }
  $script:failed = 1
  $root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path + '\'
  $files = @(Get-Lines $log | Where-Object { $_ -cmatch '^Diff in ' } | ForEach-Object {
      ($_ -replace '^Diff in ', '') -replace ' at line [0-9]+:$', '' -replace [regex]::Escape($root), ''
    } | Sort-Object -Unique)
  Say "fmt     FAILED  $($files.Count) file(s) not formatted  (full log: $log)"
  Write-Capped $files 30 $log
}

function Step-Test {
  $log = "$LogDir/check-test.log"
  New-Log $log
  $status = Invoke-Logged $log 'cargo' (@('test') + $scope)
  $lines = Get-Lines $log
  $c = Get-TestCounts $lines
  $counts = '{0} passed, {1} failed, {2} ignored' -f $c.Passed, $c.Failed, $c.Ignored
  if ($status -eq 0) {
    Say "test    ok      $counts"
    return
  }
  $script:failed = 1
  Say "test    FAILED  $counts  (full log: $log)"
  # Compile errors, then each failing test's panic message.
  $errors = Select-WithContext $lines '^error(\[E[0-9]+\])?:' 12 |
    Where-Object { $_ -cnotmatch '^error: (test failed|could not compile)' }
  Write-Capped $errors 60 $log
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
  Write-Capped $panics 80 $log
}

function Step-Clippy {
  $log = "$LogDir/check-clippy.log"
  New-Log $log
  $status = Invoke-Logged $log 'cargo' (@('clippy') + $scope + @(
      '--all-targets', '--all-features', '--',
      '-D', 'warnings', '-D', 'clippy::unwrap_used', '-D', 'clippy::expect_used', '-D', 'clippy::panic'))
  if ($status -eq 0) {
    Say 'clippy  ok      no warnings'
    return
  }
  $script:failed = 1
  # Each diagnostic's headline and location, without the long help text or
  # cargo's own summary lines.
  $lines = @(Get-Lines $log)
  $diags = New-Object System.Collections.Generic.List[string]
  $heads = 0
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -cmatch '^(error|warning)(\[|:)' -and
        $line -cnotmatch 'aborting due to|generated [0-9]+ warning|could not compile|build failed') {
      $diags.Add("  $line"); $heads++
      $i++
      if ($i -lt $lines.Count) { $diags.Add("  $($lines[$i])") }
    }
  }
  Say "clippy  FAILED  $heads diagnostic(s)  (full log: $log)"
  Write-Capped $diags 60 $log
}

function Step-Eval {
  $log = "$LogDir/check-eval.log"
  New-Log $log
  $status = Invoke-Logged $log 'cargo' @('run', '-q', '-p', 'mct-eval')
  $lines = @(Get-Lines $log)
  $acc = ''; $cat = ''
  foreach ($line in $lines) {
    if ($line -cmatch '^\| Accuracy \|') { $acc = ($line.Split('|')[2]) -replace ' ', '' }
    if ($line -cmatch '^\| Tool catalog tokens \|') { $cat = ($line.Split('|')[2]) -replace ' ', '' }
  }
  $summaryLine = "accuracy $acc, catalog $cat tokens"
  switch ($status) {
    0 {
      Say "eval    ok      no regressions ($summaryLine)"
      if ($lines | Where-Object { $_ -cmatch '^Improvements' }) {
        Write-Host '        improvements found - lock them in with: cargo run -p mct-eval -- --write-baseline'
      }
    }
    1 {
      $script:failed = 1
      Say "eval    FAILED  regressions against crates/mct-eval/baseline.json ($summaryLine)"
      $on = $false
      $regressions = foreach ($line in $lines) {
        if ($line -cmatch '^## Against the baseline') { $on = $true; continue }
        if ($line -cmatch '^## ') { $on = $false }
        if ($on -and $line -cmatch '^- ') { "  $line" }
      }
      Write-Capped $regressions 40 $log
      Write-Host '        intended change? refresh with: cargo run -p mct-eval -- --write-baseline'
    }
    default {
      $script:failed = 1
      Say "eval    ERROR   mct-eval could not run  (full log: $log)"
      $errors = Select-WithContext $lines '^(error|mct-eval:)' 6
      if (@($errors).Count -gt 0) { Write-Capped $errors 30 $log }
      else { $lines | Select-Object -Last 20 | ForEach-Object { Write-Host $_ } }
    }
  }
}

if ($runFmt) { Step-Fmt }
if ($runTest) { Step-Test }
if ($runClippy) { Step-Clippy }
if ($runEval) { Step-Eval }

if ($script:failed -eq 0) {
  Say 'all checks passed'
} else {
  Say "some checks failed - full logs in $LogDir"
}
exit $script:failed
