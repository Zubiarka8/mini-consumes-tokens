#!/usr/bin/env bash
# CI-equivalent verification with a summary instead of a wall of output:
# `cargo test`, the CI clippy invocation (unwrap/expect/panic denied) and the
# `mct-eval` quality gate. Each step's full output goes to
# target/script-logs/check-<step>.log; the terminal gets one line per step,
# plus only the failing tests / lints / regressions when a step fails.
#
# Usage:
#   scripts/unix/check.sh                  # everything, like CI's build-test job
#   scripts/unix/check.sh -p mct-index     # tests + clippy for one crate, no eval
#   scripts/unix/check.sh --no-eval        # skip the mct-eval quality gate
#   scripts/unix/check.sh --only clippy    # one step: test | clippy | eval
#
# Exit status: 0 when every step passed, 1 otherwise.

source "$(dirname "$0")/lib.sh"

package=""
run_test=1 run_clippy=1 run_eval=1
while [ $# -gt 0 ]; do
  case "$1" in
    -p | --package) package="${2:?-p needs a crate name}"; run_eval=0; shift 2 ;;
    --no-eval) run_eval=0; shift ;;
    --only)
      run_test=0 run_clippy=0 run_eval=0
      case "${2:-}" in
        test) run_test=1 ;; clippy) run_clippy=1 ;; eval) run_eval=1 ;;
        *) die "--only takes test, clippy or eval" ;;
      esac
      shift 2 ;;
    -h | --help) sed -n '2,15p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

if [ -n "$package" ]; then scope=(-p "$package"); else scope=(--workspace); fi
failed=0

step_test() {
  local log="$LOG_DIR/check-test.log" status=0
  new_log "$log"
  cargo test "${scope[@]}" >>"$log" 2>&1 || status=$?
  local counts
  counts=$(awk '/^test result:/ {
      for (i = 1; i <= NF; i++) {
        if ($i ~ /^passed;?$/) p += $(i-1)
        if ($i ~ /^failed;?$/) f += $(i-1)
        if ($i ~ /^ignored;?$/) g += $(i-1)
      }
    } END { printf "%d passed, %d failed, %d ignored", p, f, g }' "$log")
  if [ "$status" -eq 0 ]; then
    echo "test    ok      $counts"
    return
  fi
  failed=1
  echo "test    FAILED  $counts  (full log: $log)"
  # Compile errors, then each failing test's panic message.
  grep -E -A12 '^error(\[E[0-9]+\])?:' "$log" \
    | grep -vE '^--$|^error: (test failed|could not compile)' | cap 60 "$log" || true
  awk '
    /^---- .* stdout ----$/ { name = $2; show = 1; n = 0; print "  ✗ " name; next }
    /^(failures:|---- )/ { show = 0 }
    show && n < 8 && NF && !/RUST_BACKTRACE/ { print "      " $0; n++ }
  ' "$log" | cap 80 "$log"
}

step_clippy() {
  local log="$LOG_DIR/check-clippy.log" status=0
  new_log "$log"
  cargo clippy "${scope[@]}" --all-targets --all-features -- \
    -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic \
    >>"$log" 2>&1 || status=$?
  if [ "$status" -eq 0 ]; then
    echo "clippy  ok      no warnings"
    return
  fi
  failed=1
  # Each diagnostic's headline and location, without the long help text or
  # cargo's own summary lines.
  local diags
  diags=$(awk '
    /^(error|warning)(\[|:)/ && !/aborting due to|generated [0-9]+ warning|could not compile|build failed/ {
      head = $0; getline loc; print "  " head; print "  " loc
    }
  ' "$log")
  echo "clippy  FAILED  $(printf '%s\n' "$diags" | grep -c '^  [ew]') diagnostic(s)  (full log: $log)"
  printf '%s\n' "$diags" | cap 60 "$log"
}

step_eval() {
  local log="$LOG_DIR/check-eval.log" status=0
  new_log "$log"
  cargo run -q -p mct-eval >>"$log" 2>&1 || status=$?
  local summary
  summary=$(awk -F'|' '
    /^\| Accuracy \|/ { acc = $3 }
    /^\| Tool catalog tokens \|/ { cat = $3 }
    END { gsub(/ /, "", acc); gsub(/ /, "", cat); printf "accuracy %s, catalog %s tokens", acc, cat }
  ' "$log")
  case "$status" in
    0)
      echo "eval    ok      no regressions ($summary)"
      if grep -q '^Improvements' "$log"; then
        echo "        improvements found — lock them in with: cargo run -p mct-eval -- --write-baseline"
      fi ;;
    1)
      failed=1
      echo "eval    FAILED  regressions against crates/mct-eval/baseline.json ($summary)"
      awk '/^## Against the baseline/ { on = 1; next } /^## / { on = 0 } on && /^- / { print "  " $0 }' "$log" | cap 40 "$log"
      echo "        intended change? refresh with: cargo run -p mct-eval -- --write-baseline" ;;
    *)
      failed=1
      echo "eval    ERROR   mct-eval could not run  (full log: $log)"
      grep -E '^(error|mct-eval:)' -A6 "$log" | cap 30 "$log" || tail -n 20 "$log" ;;
  esac
}

[ "$run_test" -eq 1 ] && step_test
[ "$run_clippy" -eq 1 ] && step_clippy
[ "$run_eval" -eq 1 ] && step_eval

if [ "$failed" -eq 0 ]; then
  echo "all checks passed"
else
  echo "some checks failed — full logs in $LOG_DIR"
fi
exit "$failed"
