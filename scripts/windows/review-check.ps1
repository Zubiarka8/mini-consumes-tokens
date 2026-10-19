# Run the full repository check in an isolated Cargo target directory, continue
# through all test targets after failures, and write a Markdown result table.
# Full logs and the table live under target\script-logs\.
#
# Usage:
#   scripts\windows\review-check.ps1 [check.ps1 options]
#
# Runs scripts\windows\check.ps1 with --isolated-target and --no-fail-fast, then
# writes target\script-logs\review-check-table.md. Options such as --no-eval,
# --only and -p are forwarded to check.ps1.
#
# Set MCT_CHECK_TARGET_DIR to override the isolated Cargo target directory.
#
# Exit status: that of check.ps1.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

if ($argv.Count -gt 0 -and $argv[0] -in '-h', '--help') { Show-Help; exit 0 }

$checkLogDir = 'target/script-logs/review-check'
$env:MCT_CHECK_LOG_DIR = $checkLogDir
$env:MCT_CHECK_SUMMARY = "$checkLogDir/check-summary.md"

& (Join-Path $PSScriptRoot 'check.ps1') --isolated-target --no-fail-fast @argv
$checkStatus = $LASTEXITCODE
Remove-Item Env:MCT_CHECK_LOG_DIR, Env:MCT_CHECK_SUMMARY -ErrorAction SilentlyContinue

$summary = Get-RepoPath $env:MCT_CHECK_SUMMARY
$table = Get-RepoPath 'target/script-logs/review-check-table.md'
if ($env:MCT_CHECK_TARGET_DIR) { $targetDir = $env:MCT_CHECK_TARGET_DIR } else { $targetDir = 'target\review-check' }
if (-not [IO.Path]::IsPathRooted($targetDir)) { $targetDir = Join-Path $Root $targetDir }

$commands = @{
  'fmt'    = 'cargo fmt --all --check'
  'test'   = 'cargo test --workspace --no-fail-fast'
  'clippy' = 'cargo clippy --workspace --all-targets --all-features'
  'eval'   = 'cargo run -q -p mct-eval'
}
$out = New-Object System.Collections.Generic.List[string]
$out.Add('# Review check results')
$out.Add('')
$out.Add("- Checkout: ``$Root``")
$out.Add("- Cargo target: ``$targetDir``")
$out.Add("- Script exit code: ``$checkStatus``")
$out.Add('')
$out.Add('| Check | Command | Result | Details |')
$out.Add('| --- | --- | --- | --- |')
$overall = 'unknown'
foreach ($line in (Get-Lines $summary)) {
  if ($line -cmatch '^(fmt|test|clippy|eval)\s+(\S+)\s*(.*)$') {
    $details = $Matches[3] -replace '\|', '\|'
    $out.Add("| $($Matches[1]) | ``$($commands[$Matches[1]])`` | $($Matches[2]) | $details |")
  }
  if ($line -cmatch '^all checks') { $overall = 'passed' }
  if ($line -cmatch '^some checks') { $overall = 'failed' }
}
$out.Add("| Overall | | $overall | exit $checkStatus |")

$testLog = Get-RepoPath "$checkLogDir/check-test.log"
if (Test-Path $testLog) {
  $failed = New-Object System.Collections.Generic.List[string]
  $listing = $false
  foreach ($line in (Get-Lines $testLog)) {
    if ($line -cmatch '^error: [0-9]+ targets failed:') { $listing = $true; continue }
    if ($listing -and $line -cmatch '^    `(.*)`$') { $failed.Add("- ``$($Matches[1])``"); continue }
    if ($listing) { break }
  }
  if ($failed.Count -gt 0) {
    $out.Add('')
    $out.Add('Failed test targets:')
    $out.Add('')
    foreach ($f in $failed) { $out.Add($f) }
  }
}
[IO.File]::WriteAllLines($table, $out, $Utf8NoBom)

Write-Host "table   target/script-logs/review-check-table.md"
exit $checkStatus
