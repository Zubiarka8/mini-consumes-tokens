#!/usr/bin/env bash
# One language's long-fixture corpus (issue #74) in one screen: runs its
# corpus tests (and its src/libraries/ tests), then prints the `mct-corpus` report — file/line/symbol/
# relation totals, the progress-table cells, and heuristic checks that list
# the usual parser bugs (module ending past EOF, locals taken as symbols,
# relations outside their owner, members without a parent, odd names…) —
# so reviewing a parser change doesn't mean reading expected.snap. Full
# cargo output goes to target/script-logs/corpus-report-{test,report}.log.
#
# Usage:
#   scripts/unix/corpus-report.sh cpp                    # mct-lang-cpp: tests + report
#   scripts/unix/corpus-report.sh cpp --bless            # regenerate expected.snap first
#   scripts/unix/corpus-report.sh cpp --update-progress  # also rewrite its counts in
#                                                        # internal/corpus-progress.md
#
# Exit status: 0 when the corpus tests passed, 1 otherwise.

source "$(dirname "$0")/lib.sh"

crate="" bless=0 progress=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bless) bless=1; shift ;;
    --update-progress) progress=1; shift ;;
    -h | --help) sed -n '2,17p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown argument: $1 (see --help)" ;;
    *) [ -z "$crate" ] || die "one language at a time"; crate="$1"; shift ;;
  esac
done
[ -n "$crate" ] || die "which language? e.g. scripts/unix/corpus-report.sh cpp"
case "$crate" in mct-*) ;; *) crate="mct-lang-$crate" ;; esac
[ -f "crates/$crate/tests/corpus.rs" ] || die "crates/$crate/tests/corpus.rs not found"

status=0
log="$LOG_DIR/corpus-report-test.log"
new_log "$log"
if [ "$bless" -eq 1 ]; then
  MCT_BLESS=1 cargo test -p "$crate" --test corpus >>"$log" 2>&1 || status=$?
else
  cargo test -p "$crate" --test corpus >>"$log" 2>&1 || status=$?
fi
# Library-pattern tests (src/libraries/<name>/) read the same corpus but are
# unit tests of the lib target, outside the `corpus` binary.
cargo test -p "$crate" --lib libraries:: >>"$log" 2>&1 || status=$?
counts=$(awk '/^test result:/ { for (i = 1; i <= NF; i++) {
    if ($i ~ /^passed;?$/) p += $(i-1); if ($i ~ /^failed;?$/) f += $(i-1) } }
  END { printf "%d passed, %d failed", p, f }' "$log")
if [ "$status" -eq 0 ]; then
  echo "tests   ok      $counts$([ "$bless" -eq 1 ] && echo ' (expected.snap blessed)')"
else
  echo "tests   FAILED  $counts  (full log: $log)"
  grep -E -A12 '^error(\[E[0-9]+\])?:' "$log" | grep -vE '^--$|^error: (test failed|could not compile)' | cap 40 "$log" || true
  awk '
    /^---- .* stdout ----$/ { name = $2; show = 1; n = 0; print "  ✗ " name; next }
    /^(failures:|---- )/ { show = 0 }
    show && n < 8 && NF && !/RUST_BACKTRACE/ { print "      " $0; n++ }
  ' "$log" | cap 60 "$log"
fi

log="$LOG_DIR/corpus-report-report.log"
new_log "$log"
env_args=()
[ "$progress" -eq 1 ] && env_args=(MCT_CORPUS_PROGRESS="$ROOT/internal/corpus-progress.md")
if env ${env_args[@]+"${env_args[@]}"} cargo test -p "$crate" --test corpus corpus_report \
  -- --ignored --nocapture >>"$log" 2>&1; then
  echo
  awk '/^corpus report:/ { on = 1 } on && /^(test |running |\.$)/ { on = 0 } on' "$log"
else
  echo "report  FAILED  (full log: $log)"
  status=1
fi
[ "$status" -eq 0 ] || exit 1
