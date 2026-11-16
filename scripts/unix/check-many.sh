#!/usr/bin/env bash
# Runs check.sh in several worktrees, one after the other, so a batch of PR
# branches is verified without two cargo runs fighting for the CPU. Each
# worktree gets its own Cargo target directory and the CI toolchain, and a
# time limit: a run over the limit is killed and reported, which is how a
# hanging test shows up instead of blocking the whole batch.
#
# Usage:
#   scripts/unix/check-many.sh [--toolchain <t>] [--timeout <minutes>] [--] <worktree>... [-- <check.sh options>]
#
# Defaults: toolchain 1.96.0 (the CI pin), 40 minutes per worktree.
# Output: one line per worktree, and a table in target/script-logs/check-many.md.
# Exit status: 0 every worktree passed, 1 some failed or timed out, 2 usage error.

source "$(dirname "$0")/lib.sh"

toolchain="1.96.0"
minutes=40
trees=""
forward=""
while [ $# -gt 0 ]; do
  case "$1" in
    --toolchain) [ $# -ge 2 ] || die "--toolchain needs a value"; toolchain="$2"; shift ;;
    --timeout) [ $# -ge 2 ] || die "--timeout needs minutes"; minutes="$2"; shift ;;
    --) shift; forward="$*"; break ;;
    -*) die "unknown option $1" ;;
    *) trees="$trees $(abspath "$1")" ;;
  esac
  shift
done
[ -n "$trees" ] || die "usage: scripts/unix/check-many.sh [--toolchain T] [--timeout MIN] <worktree>... [-- check.sh options]"

# Ends a process and everything it started (cargo, rustc, the test binaries).
kill_tree() {
  local child
  for child in $(pgrep -P "$1" 2>/dev/null); do kill_tree "$child"; done
  kill -TERM "$1" 2>/dev/null || true
}

SUMMARY="$ROOT/$LOG_DIR/check-many.md"
{
  printf '# %s\n\n' "$(log_stamp)"
  printf '| Worktree | Branch | Result |\n|---|---|---|\n'
} >"$SUMMARY"

failed=0
for tree in $trees; do
  [ -x "$tree/scripts/unix/check.sh" ] || die "$tree has no scripts/unix/check.sh"
  branch="$(git -C "$tree" rev-parse --abbrev-ref HEAD)"
  out="$tree/target/script-logs/check-many-run.log"
  mkdir -p "$tree/target/script-logs"
  echo "== $branch ($tree)"
  (
    cd "$tree"
    export RUSTUP_TOOLCHAIN="$toolchain" CARGO_TARGET_DIR="$tree/target/own-check"
    # shellcheck disable=SC2086
    scripts/unix/check.sh $forward >"$out" 2>&1 </dev/null &
    pid=$!
    waited=0
    while kill -0 "$pid" 2>/dev/null; do
      if [ "$waited" -ge $((minutes * 60)) ]; then
        kill_tree "$pid"
        touch "$out.timeout"
        break
      fi
      sleep 2
      waited=$((waited + 2))
    done
    wait "$pid" 2>/dev/null && status=0 || status=$?
    exit "$status"
  ) && status=0 || status=$?
  if [ -e "$out.timeout" ]; then
    rm -f "$out.timeout"
    result="timed out after ${minutes} min (a hanging test? see $out)"
    failed=1
  elif [ "$status" -eq 0 ]; then
    result="$(grep -E '^test ' "$out" | head -1 | sed 's/  */ /g' || true)"
    result="passed${result:+, $result}"
  else
    result="FAILED (exit $status), see $out"
    failed=1
  fi
  echo "   $result"
  printf '| %s | %s | %s |\n' "$tree" "$branch" "$result" >>"$SUMMARY"
done
echo "table: $SUMMARY"
exit "$failed"
