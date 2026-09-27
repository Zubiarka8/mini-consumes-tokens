# Lists every place a new MCP tool has to be wired into, and which of them
# still don't mention it - the checklist that otherwise surfaces one failing
# test at a time. Then runs the two tests that guard the catalog (TTC
# coverage and the description-byte budget).
#
# Usage:
#   scripts\windows\new-tool-check.ps1 build_context_pack
#   scripts\windows\new-tool-check.ps1 build_context_pack --no-tests
#
# Exit status: 1 if a required (code) place is missing or a catalog test
# fails; documentation gaps are reported as warnings only.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$tool = ''
if ($argv.Count -gt 0) { $tool = [string]$argv[0] }
if (-not $tool -or $tool.StartsWith('-')) { Show-Help; exit 2 }
$runTests = -not ($argv.Count -gt 1 -and $argv[1] -ceq '--no-tests')
$t = [regex]::Escape($tool)

$server = 'crates/mct-mcp-server/src/server.rs'
$script:missing = 0

# `Test-Place <required|doc> <description> <file|dir> <regex> [range start]`
# With a range start, only the lines from that pattern to the next `];` are
# searched (a Rust const array). Matching is case-sensitive, like grep.
function Test-Place([string]$Level, [string]$What, [string]$File, [string]$Pattern, [string]$Range = '') {
  $path = Get-RepoPath $File
  $found = 0
  if (Test-Path -LiteralPath $path -PathType Container) {
    foreach ($f in Get-ChildItem -LiteralPath $path -Recurse -File) {
      if (Get-Lines $f.FullName | Where-Object { $_ -cmatch $Pattern } | Select-Object -First 1) { $found++ }
    }
  } elseif ($Range) {
    $on = $false
    foreach ($line in Get-Lines $path) {
      if ($line -cmatch $Range) { $on = $true }
      if ($on -and $line -cmatch $Pattern) { $found++ }
      if ($on -and $line -cmatch '^\];') { break }
    }
  } else {
    $found = @(Get-Lines $path | Where-Object { $_ -cmatch $Pattern }).Count
  }
  if ($found -gt 0) {
    Write-Host ('ok    {0,-44} {1}' -f $What, $File)
  } elseif ($Level -eq 'required') {
    Write-Host ('MISS  {0,-44} {1}' -f $What, $File)
    $script:missing = 1
  } else {
    Write-Host ('warn  {0,-44} {1}' -f $What, $File)
  }
}

Write-Host "wiring for ``$tool``:"
Test-Place required '#[tool] method'                   $server "pub async fn $t\("
Test-Place required 'TTC entry (TOOL line)'            'crates/mct-mcp-server/src/tools.ttc' ('^TOOL ' + $t + '$')
Test-Place required 'ttc::KNOWN_TOOL_NAMES'            'crates/mct-mcp-server/src/ttc.rs' ('^ *"' + $t + '",') 'KNOWN_TOOL_NAMES'
Test-Place required 'TOOL_CATEGORIES group'            $server ('"' + $t + '"') '^const TOOL_CATEGORIES'
Test-Place required 'batch dispatch (run_batch_query)' $server ('^ *"' + $t + '" =>')
Test-Place doc      'a test under tests/'              'crates/mct-mcp-server/tests' ('"' + $t + '"')
Test-Place doc      'mct-eval suite case'              'crates/mct-eval/suite.json' ('"tool": *"' + $t + '"')
Test-Place doc      'README.md'                        'README.md' $t
Test-Place doc      'CLAUDE.md tool table'             'CLAUDE.md' ('^\| .*`' + $t + '`')
Test-Place doc      'MCP protocol spec'                'docs/01-architecture/mcp-protocol-spec.md' $t

$known = 0; $on = $false
foreach ($line in Get-Lines 'crates/mct-mcp-server/src/ttc.rs') {
  if ($line -cmatch '^pub const KNOWN_TOOL_NAMES') { $on = $true; continue }
  if ($on -and $line -cmatch '^\];') { break }
  if ($on -and $line.Contains('"')) { $known++ }
}
$readme = ''
foreach ($line in Get-Lines 'README.md') {
  if ($line -cmatch '\*\*([0-9]+) MCP tools') { $readme = $Matches[1]; break }
}
if ($readme -and $readme -ne "$known") {
  Write-Host "warn  README.md says $readme MCP tools, KNOWN_TOOL_NAMES has $known"
}
$limit = '?'
foreach ($line in Get-Lines 'crates/mct-mcp-server/tests/catalog.rs') {
  if ($line -cmatch 'description_bytes < ([0-9_]+)') { $limit = $Matches[1] -replace '_', ''; break }
}

$status = $script:missing
if ($runTests) {
  $log = "$LogDir/new-tool-check-tests.log"
  New-Log $log
  $ok = (Invoke-Logged $log 'cargo' @('test', '-p', 'mct-mcp-server', '--lib', 'ttc')) -eq 0
  if ($ok) { $ok = (Invoke-Logged $log 'cargo' @('test', '-p', 'mct-mcp-server', '--test', 'catalog')) -eq 0 }
  if ($ok) {
    Write-Host "ok    catalog tests (TTC coverage, description bytes < $limit)"
  } else {
    $status = 1
    Write-Host "FAIL  catalog tests - full log: $log"
    Write-Capped (Select-WithContext (Get-Lines $log) 'panicked at|grew to|drifted|no TTC entry|error(\[|:)' 2) 20 $log
    Write-Host '      a new tool that needs the room: raise the limit in crates/mct-mcp-server/tests/catalog.rs, with a note'
  }
  Write-Host 'next  scripts\windows\check.ps1 - the mct-eval gate fails on catalog-token growth;'
  Write-Host '      if intended, refresh with: cargo run -p mct-eval -- --write-baseline'
}
exit $status
