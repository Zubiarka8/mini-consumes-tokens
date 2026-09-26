#!/usr/bin/env bash
# Lists every place a new MCP tool has to be wired into, and which of them
# still don't mention it — the checklist that otherwise surfaces one failing
# test at a time. Then runs the two tests that guard the catalog (TTC
# coverage and the description-byte budget).
#
# Usage:
#   scripts/unix/new-tool-check.sh build_context_pack
#   scripts/unix/new-tool-check.sh build_context_pack --no-tests
#
# Exit status: 1 if a required (code) place is missing or a catalog test
# fails; documentation gaps are reported as warnings only.

source "$(dirname "$0")/lib.sh"

tool="${1:-}"
[ -n "$tool" ] && [ "${tool#-}" = "$tool" ] || { sed -n '2,12p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 2; }
run_tests=1
[ "${2:-}" = "--no-tests" ] && run_tests=0

server=crates/mct-mcp-server/src/server.rs
missing=0

# `check <required|doc> <description> <file|dir> <extended regex> [awk range start]`
# With a range start, only the lines from that pattern to the next `];` are
# searched (a Rust const array).
check() {
  local level="$1" what="$2" file="$3" pattern="$4" range="${5:-}" found
  if [ -d "$file" ]; then
    found=$(grep -rlE "$pattern" "$file" | grep -c . || true)
  elif [ -n "$range" ]; then
    found=$(awk -v start="$range" '$0 ~ start { on = 1 } on { print } on && /^\];/ { exit }' "$file" | grep -cE "$pattern" || true)
  else
    found=$(grep -cE "$pattern" "$file" 2>/dev/null || true)
  fi
  if [ "${found:-0}" -gt 0 ]; then
    printf 'ok    %-44s %s\n' "$what" "$file"
  elif [ "$level" = required ]; then
    printf 'MISS  %-44s %s\n' "$what" "$file"
    missing=1
  else
    printf 'warn  %-44s %s\n' "$what" "$file"
  fi
}

echo "wiring for \`$tool\`:"
check required "#[tool] method"                   "$server" "pub async fn $tool\\("
check required "TTC entry (TOOL line)"            crates/mct-mcp-server/src/tools.ttc "^TOOL $tool\$"
check required "ttc::KNOWN_TOOL_NAMES"            crates/mct-mcp-server/src/ttc.rs "^ *\"$tool\"," "KNOWN_TOOL_NAMES"
check required "TOOL_CATEGORIES group"            "$server" "\"$tool\"" "^const TOOL_CATEGORIES"
check required "batch dispatch (run_batch_query)" "$server" "^ *\"$tool\" =>"
check doc      "a test under tests/"              crates/mct-mcp-server/tests "\"$tool\""
check doc      "mct-eval suite case"              crates/mct-eval/suite.json "\"tool\": *\"$tool\""
check doc      "README.md"                        README.md "$tool"
check doc      "CLAUDE.md tool table"             CLAUDE.md "^\\| .*\`$tool\`"
check doc      "MCP protocol spec"                docs/01-architecture/mcp-protocol-spec.md "$tool"

known=$(awk '/^pub const KNOWN_TOOL_NAMES/ { on = 1; next } on && /^\];/ { exit } on && /"/ { n++ } END { print n + 0 }' crates/mct-mcp-server/src/ttc.rs)
readme=$(grep -oE '\*\*[0-9]+ MCP tools' README.md | grep -oE '[0-9]+' || true)
if [ -n "$readme" ] && [ "$readme" != "$known" ]; then
  echo "warn  README.md says $readme MCP tools, KNOWN_TOOL_NAMES has $known"
fi
limit=$(grep -oE 'description_bytes < [0-9_]+' crates/mct-mcp-server/tests/catalog.rs | grep -oE '[0-9_]+$' | tr -d _ || true)

status=$missing
if [ "$run_tests" -eq 1 ]; then
  log="$LOG_DIR/new-tool-check-tests.log"
  if cargo test -p mct-mcp-server --lib ttc >"$log" 2>&1 && cargo test -p mct-mcp-server --test catalog >>"$log" 2>&1; then
    echo "ok    catalog tests (TTC coverage, description bytes < ${limit:-?})"
  else
    status=1
    echo "FAIL  catalog tests — full log: $log"
    grep -E 'panicked at|grew to|drifted|no TTC entry|error(\[|:)' -A2 "$log" | cap 20 "$log" || true
    echo "      a new tool that needs the room: raise the limit in crates/mct-mcp-server/tests/catalog.rs, with a note"
  fi
  echo "next  scripts/unix/check.sh — the mct-eval gate fails on catalog-token growth;"
  echo "      if intended, refresh with: cargo run -p mct-eval -- --write-baseline"
fi
exit "$status"
