#!/usr/bin/env bash
# Parses files with this project's parsers without indexing anything and
# prints, per file, its symbol/relation counts or the first syntax error with
# the offending source line — for finding which construct a tree-sitter
# grammar rejects (write the suspect snippets to separate files, probe them
# all at once). Build output goes to target/script-logs/parse-probe.log.
#
# Usage:
#   scripts/unix/parse-probe.sh a.cpp b.cpp c.py
#
# Exit status: 0 when every file parsed, 1 otherwise.

source "$(dirname "$0")/lib.sh"

case "${1:-}" in
  "" ) die "which files? (see --help)" ;;
  -h | --help) sed -n '2,11p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
esac

log="$LOG_DIR/parse-probe.log"
new_log "$log"
if ! cargo build -p mct-cli >>"$log" 2>&1; then
  echo "build   FAILED  (full log: $log)"
  grep -E -A12 '^error(\[E[0-9]+\])?:' "$log" | cap 40 "$log" || true
  exit 1
fi
files=()
for f in "$@"; do files+=("$(abspath "$f")"); done
target/debug/mct-cli probe "${files[@]}" 2>&1 || exit 1
