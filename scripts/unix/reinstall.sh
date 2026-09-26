#!/usr/bin/env bash
# Reinstalls mct-mcp-server and mct-cli from this checkout into ~/.cargo/bin,
# makes sure .mct-index/index.sqlite3 matches this checkout's schema, and
# smoke-tests the installed server. Run it after pulling, switching branches
# or merging a PR that changed the server — then `/mcp` in Claude Code to
# reconnect.
#
# Usage:
#   scripts/unix/reinstall.sh               # install both, fix the index only if needed
#   scripts/unix/reinstall.sh --reindex     # also rebuild the index from scratch
#   scripts/unix/reinstall.sh --semantic    # server built with --features semantic
#   scripts/unix/reinstall.sh --server-only # skip mct-cli
#
# The index is fully derived from source, so deleting it is always safe; it
# is only deleted when --reindex is given or its schema is newer than this
# checkout's (the `migration number that is too high` startup failure).

source "$(dirname "$0")/lib.sh"

reindex=0 semantic=0 cli=1
while [ $# -gt 0 ]; do
  case "$1" in
    --reindex) reindex=1; shift ;;
    --semantic) semantic=1; shift ;;
    --server-only) cli=0; shift ;;
    -h | --help) sed -n '2,17p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

install() {
  local crate="$1"; shift
  local log="$LOG_DIR/reinstall-$crate.log"
  new_log "$log"
  if cargo install --locked --force --path "crates/$crate" "$@" >>"$log" 2>&1; then
    echo "installed $crate $(git describe --always --dirty 2>/dev/null) → $CARGO_BIN"
  else
    echo "install FAILED for $crate — full log: $log"
    grep -E -A8 '^error' "$log" | cap 30 "$log" || tail -n 20 "$log"
    exit 1
  fi
}

if [ "$semantic" -eq 1 ]; then
  install mct-mcp-server --features semantic
else
  install mct-mcp-server
fi
[ "$cli" -eq 1 ] && install mct-cli

index="$ROOT/.mct-index/index.sqlite3"
cli_bin="$CARGO_BIN/mct-cli"
[ -x "$cli_bin" ] || cli_bin=""

if [ "$reindex" -eq 0 ] && [ -f "$index" ] && [ -n "$cli_bin" ]; then
  new_log "$LOG_DIR/reinstall-status.log"
  if "$cli_bin" --root "$ROOT" status >>"$LOG_DIR/reinstall-status.log" 2>&1; then
    echo "index ok — schema matches this checkout"
  elif grep -q 'migration number that is too high' "$LOG_DIR/reinstall-status.log"; then
    echo "index was migrated by a newer schema — rebuilding it"
    reindex=1
  else
    echo "WARN  mct-cli status failed — see $LOG_DIR/reinstall-status.log"
  fi
fi

if [ "$reindex" -eq 1 ] || [ ! -f "$index" ]; then
  rm -f "$index" "$index-wal" "$index-shm"
  new_log "$LOG_DIR/reinstall-init.log"
  if [ -n "$cli_bin" ]; then
    "$cli_bin" --root "$ROOT" init >>"$LOG_DIR/reinstall-init.log" 2>&1 \
      || { echo "index rebuild FAILED — see $LOG_DIR/reinstall-init.log"; exit 1; }
  else
    cargo run -q -p mct-cli -- --root "$ROOT" init >>"$LOG_DIR/reinstall-init.log" 2>&1 \
      || { echo "index rebuild FAILED — see $LOG_DIR/reinstall-init.log"; exit 1; }
  fi
  echo "index rebuilt — $(grep -m1 -i 'complete' "$LOG_DIR/reinstall-init.log" || true)"
fi

"$SCRIPTS_DIR/mcp-smoke.sh"
echo "done — run /mcp in Claude Code to reconnect"
