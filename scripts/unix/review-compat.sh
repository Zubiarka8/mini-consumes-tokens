#!/usr/bin/env bash
# Live compatibility tests of review decisions: this checkout's `mct-cli web`
# and an older one (default e68aec9, origin/main before shared review
# decisions) save decisions in the same temporary project, at the same time.
# Runs crates/mct-cli/tests/review_compat.rs, which is ignored by default.
#
# The older binary is built once from a `git archive` of the commit under
# target/review-compat/<commit>/ (never a worktree, never this checkout's
# index or review files) and reused by later runs.
#
# Usage:
#   scripts/unix/review-compat.sh             # against e68aec9
#   scripts/unix/review-compat.sh <commit>    # against another commit
#
# Exit status: 0 when every test passed, 1 when one failed, 2 on usage or
# build errors.

source "$(dirname "$0")/lib.sh"

usage="usage: scripts/unix/review-compat.sh [<commit>]"
case "${1:-}" in
  -h | --help)
    echo "$usage"
    exit 0 ;;
  -*) die "$usage" ;;
esac
base="${1:-e68aec9}"
git rev-parse --verify --quiet "$base^{commit}" >/dev/null || die "unknown commit: $base"

dir="target/review-compat/$base"
binary="$ROOT/$dir/target/release/mct-cli"
log="$LOG_DIR/review-compat.log"
new_log "$log"

if [ ! -x "$binary" ]; then
  echo "building mct-cli at $base (first run only, a few minutes)…"
  rm -rf "$dir/src"
  mkdir -p "$dir/src"
  git archive "$base" | tar -x -C "$dir/src" || die "cannot archive $base"
  CARGO_TARGET_DIR="$dir/target" cargo build --release \
    --manifest-path "$dir/src/Cargo.toml" -p mct-cli --bin mct-cli >>"$log" 2>&1 \
    || die "build of $base failed; see $log"
fi

status=0
MCT_COMPAT_MA="$binary" cargo test -p mct-cli --test review_compat -- --ignored \
  >>"$log" 2>&1 || status=1
grep -E "^test .* \.\.\. |^test result" "$log" || true
if [ "$status" -eq 0 ]; then
  echo "review compatibility against $base: ok (log: $log)"
else
  echo "review compatibility against $base: FAILED (log: $log)"
fi
exit "$status"
