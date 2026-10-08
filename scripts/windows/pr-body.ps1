# A pull request description draft whose Verification section is copied from the
# logs of the last scripts\windows\check.ps1 run, and whose token figures from
# the last scripts\windows\token-report.ps1 run, so the numbers come from a run
# and not from memory. Nothing is run here: run check.ps1 (and token-report.ps1
# for a PR that claims a token change) first.
#
# Usage:
#   scripts\windows\pr-body.ps1               # writes target\script-logs\pr-body.md
#   scripts\windows\pr-body.ps1 <file>        # writes to <file> instead
#
# Warns when a log was made on another commit than HEAD, on a dirty tree, or by
# a partial run (-p, --only, --no-eval): the numbers are then not for the PR.
# Exit status: 0, or 2 when there is no check log to copy from.

$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$out = if ($argv.Count -ge 1) { Get-AbsPath $argv[0] } else { Get-RepoPath "$LogDir/pr-body.md" }
$checkRel = "$LogDir/check-summary.md"
$tokensRel = "$LogDir/token-report.md"
if (-not (Test-Path -LiteralPath (Get-RepoPath $checkRel))) {
  Stop-Usage "no $checkRel: run scripts\windows\check.ps1 first"
}

$headRev = (Get-Git @('rev-parse', '--short', 'HEAD')) | Select-Object -First 1
$warnings = New-Object System.Collections.Generic.List[string]

# A log's stamp is `<command> <em dash> <date> <em dash> <branch>@<commit>[-dirty]`.
function Get-Stamp([string]$Rel) {
  $first = (Get-Lines $Rel) | Select-Object -First 1
  return ($first -replace '^(<!-- )?# ?', '' -replace '( -->)?$', '')
}
$checkStamp = Get-Stamp $checkRel
$revision = $checkStamp.Substring($checkStamp.LastIndexOf(" $EmDash ") + 3)
$command = $checkStamp.Substring(0, $checkStamp.IndexOf(" $EmDash "))

if (-not $revision.Contains("@$headRev")) { $warnings.Add("check ran on another commit than HEAD ($headRev)") }
if ($revision.EndsWith('-dirty')) { $warnings.Add('check ran on a dirty tree') }
if ($command -match '--only|(^| )-p |--no-eval') { $warnings.Add("check ran partially ($command), not every step") }
if (Test-Path -LiteralPath (Get-RepoPath $tokensRel)) {
  $tokenStamp = Get-Stamp $tokensRel
  $tokenRevision = $tokenStamp.Substring($tokenStamp.LastIndexOf(" $EmDash ") + 3)
  if (-not $tokenRevision.Contains("@$headRev")) { $warnings.Add('token figures are from another commit than HEAD') }
}
foreach ($w in $warnings) { Write-Warning $w }

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add('## Summary'); $lines.Add('')
$lines.Add('<!-- what changes and why -->'); $lines.Add('')
$lines.Add('## Verification'); $lines.Add('')
if ($warnings.Count -gt 0) { $lines.Add('<!-- WARNING: ' + ($warnings -join '; ') + ' -->') }
$lines.Add('```text')
Get-Lines $checkRel | Select-Object -Skip 1 | Where-Object { $_ -ne '' } | ForEach-Object { $lines.Add($_) }
$lines.Add('```'); $lines.Add('')
$lines.Add("Measured on ``$revision`` with ``scripts\windows\check.ps1``."); $lines.Add('')
if (Test-Path -LiteralPath (Get-RepoPath $tokensRel)) {
  $lines.Add('## Token measurements'); $lines.Add('')
  # The report's own headings are level 2; under this PR's sections they become level 3.
  Get-Lines $tokensRel | Select-Object -Skip 1 | ForEach-Object { $lines.Add(($_ -replace '^## ', '### ')) }
  $lines.Add('')
}
$lines.Add('## Not in scope'); $lines.Add('')
$lines.Add('<!-- what this PR does not do -->'); $lines.Add('')
$lines.Add("$([char]::ConvertFromUtf32(0x1F916)) Generated with [Claude Code](https://claude.com/claude-code)")

[IO.File]::WriteAllText($out, (($lines -join "`n") + "`n"), $Utf8NoBom)
Write-Output "wrote $out"
