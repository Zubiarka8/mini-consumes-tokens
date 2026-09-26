#!/usr/bin/env bash
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
#   scripts/unix/token-report.sh               # everything
#   scripts/unix/token-report.sh --no-eval     # skip mct-eval (the slowest part)
#   scripts/unix/token-report.sh --markdown F  # also write the summary to F, for a PR body
#
# Exit status: 0 when every measurement ran, 1 if any failed to run.

source "$(dirname "$0")/lib.sh"

run_eval=1 markdown=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-eval) run_eval=0; shift ;;
    --markdown) markdown="$(abspath "${2:?--markdown needs a file}")"; shift 2 ;;
    -h | --help) sed -n '2,17p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

report="$LOG_DIR/token-report.md"
printf '<!-- %s -->\n' "$(log_stamp)" >"$report"
failed=0
say() { printf '%s\n' "$*" | tee -a "$report"; }
ran() {
  # `ran <log> <command...>`: runs the command into the log, reports a failure.
  local log="$1"; shift
  new_log "$log"
  if "$@" >>"$log" 2>&1; then return 0; fi
  failed=1
  say "  (failed to run — see $log)"
  return 1
}

say "## MCP tool vs grep+read, per language (3 canonical queries each)"
say ""
log="$LOG_DIR/token-report-languages.log"
if ran "$log" cargo run -q -p mct-cli --example token_benchmark; then
  awk -F'|' '
    function flush() {
      if (lang != "") printf "  %-24s %5.1f%% – %5.1f%% fewer\n", lang, lo, hi
    }
    /^## / { flush(); lang = $0; sub(/^## /, "", lang); sub(/ \(.*$/, "", lang); lo = 101; hi = -1; next }
    /%[ ]*\|[ ]*$/ {
      r = $(NF - 1); gsub(/[ %]/, "", r); r += 0
      if (r < lo) lo = r; if (r > hi) hi = r
    }
    END { flush() }
  ' "$log" | tee -a "$report"
fi

say ""
say "## Response formats (format_benchmark, approx tokens)"
say ""
log="$LOG_DIR/token-report-formats.log"
if ran "$log" cargo run -q -p mct-mcp-server --example format_benchmark; then
  awk '
    /^== / { name = $2 " " $3 " " $4; sub(/ ==$/, "", name) }
    /^  JSON:/ { json = $(NF - 1) }
    /^  TOON:/ { toon = $(NF - 1) }
    /^  existing text:/ { text = $(NF - 1) }
    /^  reduction vs existing text/ {
      printf "  %-26s JSON ~%-5s  TOON ~%-5s  text ~%-5s  (TOON vs text: %s)\n", name, json, toon, text, $(NF - 1) " tokens"
    }
  ' "$log" | tee -a "$report"
fi

say ""
say "## Composite tools vs the same calls made separately"
say ""
tests=$(grep -l '% fewer' crates/mct-mcp-server/tests/*.rs 2>/dev/null || true)
if [ -z "$tests" ]; then
  say "  (no test prints a \"% fewer\" measurement)"
fi
for file in $tests; do
  name="$(basename "$file" .rs)"
  log="$LOG_DIR/token-report-test-$name.log"
  if ran "$log" cargo test -q -p mct-mcp-server --test "$name" -- --nocapture --test-threads 1; then
    # Timings vary run to run; keep only the token figures.
    grep -E '% fewer' "$log" | sed -E 's/ in [0-9.]+[µnm]?s//g; s/^[.]+//; s/^/  /' | tee -a "$report"
  fi
done

if [ "$run_eval" -eq 1 ]; then
  say ""
  say "## Totals (mct-eval suite)"
  say ""
  log="$LOG_DIR/token-report-eval.log"
  # Exit 1 is a quality regression, not a failure to measure.
  new_log "$log"
  cargo run -q -p mct-eval >>"$log" 2>&1 || true
  if grep -q '^| Tool catalog tokens' "$log"; then
    awk -F'|' '
      /^\| Response tokens/ { gsub(/ /, "", $3); printf "  responses, all cases       %s tokens\n", $3 }
      /^\| Tool catalog tokens/ { gsub(/ /, "", $3); printf "  tool catalog               %s tokens\n", $3 }
    ' "$log" | tee -a "$report"
  else
    failed=1
    say "  (failed to run — see $log)"
  fi
fi

if [ -n "$markdown" ]; then
  cp "$report" "$markdown"
  echo ""
  echo "wrote $markdown"
fi
exit "$failed"
