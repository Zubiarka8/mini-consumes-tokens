#!/usr/bin/env bash
# What hasn't reached origin yet: local branches with commits missing from their
# upstream (or from every remote branch, when they have no upstream), diverged
# branches, worktrees with uncommitted changes, and stashes. Only `git fetch`
# touches the remote; nothing is pushed or changed.
#
# Usage:
#   scripts/unix/branches-status.sh          # only branches with pending work
#   scripts/unix/branches-status.sh --all    # every local branch, one line each
#
# "new vs main" counts the branch's commits whose change is not already on
# origin/main; publish-branch.sh refuses a branch where that is zero.
# Exit status: 0 (this script reports, it does not check).

source "$(dirname "$0")/lib.sh"

all=0
case "${1:-}" in
  "") ;;
  --all) all=1 ;;
  -h | --help)
    echo "usage: scripts/unix/branches-status.sh [--all]"
    exit 0 ;;
  *) die "usage: scripts/unix/branches-status.sh [--all]" ;;
esac

git fetch --all --prune --quiet || die "git fetch failed"

echo "branches:"
while IFS= read -r b; do
  # A branch cut from main and tracking it has no remote of its own.
  up="$(git rev-parse --abbrev-ref "$b@{upstream}" 2>/dev/null || true)"
  [ "$up" = origin/main ] && up=""
  newmain="$(git cherry origin/main "$b" | grep -c '^+' || true)"

  if [ -n "$up" ]; then
    read -r ahead behind <<<"$(git rev-list --left-right --count "$b...$up")"
    if [ "$ahead" -gt 0 ] && [ "$behind" -gt 0 ]; then
      state="diverged from $up: $ahead ahead, $behind behind"
    elif [ "$ahead" -gt 0 ]; then
      state="$ahead commit(s) not on $up"
    else
      state=""
    fi
  else
    unpushed="$(git rev-list --count "$b" --not --remotes)"
    if [ "$unpushed" -gt 0 ]; then
      state="no remote branch; $unpushed commit(s) on no remote"
    else
      state=""
    fi
  fi

  if [ -z "$state" ]; then
    [ "$all" = 1 ] || continue
    state="nothing pending"
  fi
  printf '  %-48s %s (new vs main: %s)\n' "$b" "$state" "$newmain"
done < <(git for-each-ref --format='%(refname:short)' refs/heads)

echo "worktrees with uncommitted changes:"
dirty=0
# One "path<TAB>branch" line per worktree, from the porcelain blocks.
while IFS=$'\t' read -r wt wtb; do
  n="$(git -C "$wt" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
  if [ "$n" -gt 0 ]; then
    printf '  %s (%s): %s changed\n' "$wt" "$wtb" "$n"
    dirty=1
  fi
done < <(git worktree list --porcelain | awk '
  /^worktree / { w = substr($0, 10) }
  /^branch /   { b = substr($0, 19) }
  /^$/         { print w "\t" (b == "" ? "detached" : b); b = "" }
')
[ "$dirty" = 1 ] || echo "  none"

echo "stashes: $(git stash list | wc -l | tr -d ' ')"
