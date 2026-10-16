# Lists every place a new language crate has to be wired into (CONTRIBUTING.md
# "Adding a new language"), and which of them are still missing. Then runs
# the crate's own tests and the shared registry's (mct-languages) tests.
#
# Usage:
#   scripts\windows\new-language-check.ps1 go            # crate crates/mct-lang-go
#   scripts\windows\new-language-check.ps1 js-ts --readme-name JavaScript
#   scripts\windows\new-language-check.ps1 go --no-tests
#
# --readme-name is the language as README.md's "Supported languages" line
# spells it (default: the crate suffix, matched case-insensitively).
#
# Exit status: 1 if a required place (code, CI, fuzz harness) is missing or a
# test fails; README and checklist gaps are reported as warnings only.

$ScriptPath = $MyInvocation.MyCommand.Path
$ScriptArgs = $args
$argv = @($args)
. (Join-Path $PSScriptRoot 'lib.ps1')

$lang = ''
if ($argv.Count -gt 0) { $lang = [string]$argv[0] }
if (-not $lang -or $lang.StartsWith('-')) { Show-Help; exit 2 }
$readmeName = $lang; $runTests = $true
for ($i = 1; $i -lt $argv.Count; $i++) {
  switch -CaseSensitive ($argv[$i]) {
    '--readme-name' {
      $i++; if ($i -ge $argv.Count) { Stop-Usage '--readme-name needs a value' }
      $readmeName = [string]$argv[$i]
    }
    '--no-tests' { $runTests = $false }
    default { Stop-Usage "unknown argument: $($argv[$i]) (see --help)" }
  }
}

$crate = "mct-lang-$lang"
$c = [regex]::Escape($crate)
$ident = [regex]::Escape($crate.Replace('-', '_'))
$dir = "crates/$crate"
$script:missing = 0

# `Test-Place <required|warn> <description> <file|dir> <regex> [-IgnoreCase]`
# Case-sensitive like grep unless -IgnoreCase.
function Test-Place([string]$Level, [string]$What, [string]$File, [string]$Pattern, [switch]$IgnoreCase) {
  $path = Get-RepoPath $File
  $found = 0
  if (Test-Path -LiteralPath $path -PathType Container) {
    $files = @(Get-ChildItem -LiteralPath $path -Recurse -File)
  } elseif (Test-Path -LiteralPath $path -PathType Leaf) {
    $files = @(Get-Item -LiteralPath $path)
  } else {
    $files = @()
  }
  foreach ($f in $files) {
    foreach ($line in Get-Lines $f.FullName) {
      if ($IgnoreCase) { $hit = $line -match $Pattern } else { $hit = $line -cmatch $Pattern }
      if ($hit) { $found++; break }
    }
  }
  if ($found -gt 0) {
    Write-Host ('ok    {0,-46} {1}' -f $What, $File)
  } elseif ($Level -eq 'required') {
    Write-Host ('MISS  {0,-46} {1}' -f $What, $File)
    $script:missing = 1
  } else {
    Write-Host ('warn  {0,-46} {1}' -f $What, $File)
  }
}

Write-Host "wiring for ``$crate``:"
if (-not (Test-Path -LiteralPath (Get-RepoPath $dir) -PathType Container)) {
  Write-Host ('MISS  {0,-46} {1}' -f 'crate directory', $dir)
  exit 1
}
Test-Place required 'depends on mct-core'                     "$dir/Cargo.toml" '^mct-core'
# Full validated version, caret semantics (ADR-004): "0.25.1", not "0.25" or "=0.25.1".
Test-Place required 'tree-sitter grammar at a full x.y.z'     "$dir/Cargo.toml" '^tree-sitter-[a-z0-9-]+ = "[0-9]+\.[0-9]+\.[0-9]+"'
Test-Place required 'implements LanguageParser'               "$dir/src" 'impl +LanguageParser +for'
Test-Place required 'workspace member'                        'Cargo.toml' ('"' + [regex]::Escape($dir) + '"')
Test-Place required 'workspace dependency'                    'Cargo.toml' ('^' + $c + ' *=')
Test-Place required 'mct-languages/Cargo.toml dependency'     'crates/mct-languages/Cargo.toml' ('^' + $c)
Test-Place required 'mct-languages build_registry registration' 'crates/mct-languages/src/lib.rs' ($ident + '::')
Test-Place required 'tests/parse.rs'                          "$dir/tests/parse.rs" '#\[test\]'
Test-Place required 'syntax-error test (ParseError::Syntax)'  "$dir/tests/parse.rs" 'ParseError::Syntax'
Test-Place required 'fuzz harness (fuzz/Cargo.toml [workspace])' "$dir/fuzz/Cargo.toml" '^\[workspace\]'
Test-Place required 'fuzz target (fuzz/fuzz_targets/*.rs)'    "$dir/fuzz/fuzz_targets" 'fuzz_target!'
# A harness CI never runs is the gap CONTRIBUTING.md warns about.
Test-Place required 'ci.yml fuzz-smoke matrix entry'          '.github/workflows/ci.yml' ('^ *- ' + $c + '$')
# Escaped: names like `C++` or `C#` are not valid/literal regexes as-is.
Test-Place warn     'README.md supported languages'           'README.md' ('Supported languages:.*' + [regex]::Escape($readmeName)) -IgnoreCase
Test-Place warn     'internal/checklist.md coverage row'      'internal/checklist.md' ('`' + $c + '`')

$status = $script:missing
if ($runTests) {
  $log = "$LogDir/new-language-check-tests.log"
  New-Log $log
  $ok = (Invoke-Logged $log 'cargo' @('test', '-p', $crate)) -eq 0
  if ($ok) { $ok = (Invoke-Logged $log 'cargo' @('test', '-p', 'mct-languages')) -eq 0 }
  if ($ok) {
    $passed = (Get-TestCounts (Get-Lines $log)).Passed
    Write-Host "ok    tests ($crate + mct-languages registry): $passed passed"
  } else {
    $status = 1
    Write-Host "FAIL  tests - full log: $log"
    Write-Capped (Select-WithContext (Get-Lines $log) 'panicked at|^error(\[|:)' 3) 20 $log
  }
}
exit $status
