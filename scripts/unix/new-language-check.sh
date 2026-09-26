#!/usr/bin/env bash
# Lists every place a new language crate has to be wired into (CONTRIBUTING.md
# "Adding a new language"), and which of them are still missing. Then runs
# the crate's own tests and mct-cli's registry tests.
#
# Usage:
#   scripts/unix/new-language-check.sh go            # crate crates/mct-lang-go
#   scripts/unix/new-language-check.sh js-ts --readme-name JavaScript
#   scripts/unix/new-language-check.sh go --no-tests
#
# --readme-name is the language as README.md's "Supported languages" line
# spells it (default: the crate suffix, matched case-insensitively).
#
# Exit status: 1 if a required (code/CI) place is missing or a test fails;
# documentation and fuzzing gaps are reported as warnings only.

source "$(dirname "$0")/lib.sh"

lang="${1:-}"
[ -n "$lang" ] && [ "${lang#-}" = "$lang" ] || { sed -n '2,16p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 2; }
shift
readme_name="$lang" run_tests=1
while [ $# -gt 0 ]; do
  case "$1" in
    --readme-name) readme_name="${2:?--readme-name needs a value}"; shift 2 ;;
    --no-tests) run_tests=0; shift ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

crate="mct-lang-$lang"
ident="$(printf '%s' "$crate" | tr - _)"
dir="crates/$crate"
missing=0

# `check <required|warn> <description> <file|dir> <extended regex> [-i]`
check() {
  local level="$1" what="$2" file="$3" pattern="$4" flags="${5:-}" found=0
  if [ -d "$file" ]; then
    found=$(grep -rlE $flags "$pattern" "$file" 2>/dev/null | grep -c . || true)
  elif [ -f "$file" ]; then
    found=$(grep -cE $flags "$pattern" "$file" || true)
  fi
  if [ "${found:-0}" -gt 0 ]; then
    printf 'ok    %-46s %s\n' "$what" "$file"
  elif [ "$level" = required ]; then
    printf 'MISS  %-46s %s\n' "$what" "$file"
    missing=1
  else
    printf 'warn  %-46s %s\n' "$what" "$file"
  fi
}

echo "wiring for \`$crate\`:"
if [ ! -d "$dir" ]; then
  echo "MISS  crate directory                                  $dir"
  exit 1
fi
check required "depends on mct-core"                     "$dir/Cargo.toml" '^mct-core'
check required "depends on a tree-sitter grammar"        "$dir/Cargo.toml" '^tree-sitter-'
check required "implements LanguageParser"               "$dir/src" 'impl +LanguageParser +for'
check required "workspace member"                        Cargo.toml "\"$dir\""
check required "workspace dependency"                    Cargo.toml "^$crate *="
check required "mct-mcp-server/Cargo.toml dependency"    crates/mct-mcp-server/Cargo.toml "^$crate"
check required "mct-mcp-server registry.rs registration" crates/mct-mcp-server/src/registry.rs "$ident::"
check required "mct-cli/Cargo.toml dependency"           crates/mct-cli/Cargo.toml "^$crate"
check required "mct-cli build_registry registration"     crates/mct-cli/src/main.rs "$ident::"
check required "tests/parse.rs"                          "$dir/tests/parse.rs" '#\[test\]'
check required "syntax-error test (ParseError::Syntax)"  "$dir/tests/parse.rs" 'ParseError::Syntax'
check warn     "fuzz harness (fuzz/Cargo.toml [workspace])" "$dir/fuzz/Cargo.toml" '^\[workspace\]'
check warn     "fuzz target (fuzz/fuzz_targets/*.rs)"    "$dir/fuzz/fuzz_targets" 'fuzz_target!'
if [ -d "$dir/fuzz" ]; then
  # A harness CI never runs is the gap CONTRIBUTING.md warns about.
  check required "ci.yml fuzz-smoke matrix entry"        .github/workflows/ci.yml "^ *- $crate\$"
else
  check warn     "ci.yml fuzz-smoke matrix entry"        .github/workflows/ci.yml "^ *- $crate\$"
fi
# Escaped: names like `C++` or `C#` are not valid/literal regexes as-is.
readme_re="$(printf '%s' "$readme_name" | sed 's/[][\.*^$+?(){}|/]/\\&/g')"
check warn     "README.md supported languages"           README.md "Supported languages:.*$readme_re" -i
check warn     "internal/checklist.md coverage row"      internal/checklist.md "\`$crate\`"

status=$missing
if [ "$run_tests" -eq 1 ]; then
  log="$LOG_DIR/new-language-check-tests.log"
  new_log "$log"
  if cargo test -p "$crate" >>"$log" 2>&1 \
    && cargo test -p mct-cli --bin mct-cli registry >>"$log" 2>&1; then
    passed=$(awk '/^test result:/ { for (i = 1; i <= NF; i++) if ($i ~ /^passed;?$/) p += $(i-1) } END { print p + 0 }' "$log")
    echo "ok    tests ($crate + mct-cli registry): $passed passed"
  else
    status=1
    echo "FAIL  tests — full log: $log"
    grep -E 'panicked at|^error(\[|:)' -A3 "$log" | cap 20 "$log" || true
  fi
fi
exit "$status"
