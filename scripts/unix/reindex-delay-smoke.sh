#!/usr/bin/env bash
# Black-box check of `mct-mcp-server --reindex-delay <VALUE>` (issue #196):
# runs the ignored end-to-end tests in crates/mct-mcp-server/tests/reindex_delay_e2e.rs,
# which spawn the real binary over stdio against a temp project and verify that
# a saved file is indexed after the delay (not before), that `off` never
# auto-updates, and that invalid values / `--reindex-debounce-ms` are rejected.
#
# Usage:
#   scripts/unix/reindex-delay-smoke.sh            # run the tests
#
# Exit status: 0 when every test passed, 1 otherwise (full log under
# target/script-logs/reindex-delay-smoke.log).

source "$(dirname "$0")/lib.sh"

case "${1:-}" in
  -h | --help) sed -n '2,11p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "") ;;
  *) die "unknown argument: $1 (see --help)" ;;
esac

log="$LOG_DIR/reindex-delay-smoke.log"
new_log "$log"
if cargo test -p mct-mcp-server --test reindex_delay_e2e -- --ignored >>"$log" 2>&1; then
  grep -E '^test result' "$log" | tail -n 1
  echo "reindex-delay smoke ok"
else
  grep -E '^test |panicked|error' "$log" | cap 30 "$log"
  echo "reindex-delay smoke FAILED — see $log"
  exit 1
fi
