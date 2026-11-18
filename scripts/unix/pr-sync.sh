#!/usr/bin/env bash
# Brings a PR branch up to date: merges its base (the PR's base branch, or main)
# into it, in the worktree where the branch is checked out (a new sibling
# worktree when it is not checked out anywhere). The two append-only ledgers
# that nearly every PR touches, the phase tracker and errors/HISTORY.md, are
# resolved by keeping both sides; any other conflict stops the script with the
# merge left in progress for a manual fix. Never forces anything.
#
# Usage:
#   scripts/unix/pr-sync.sh <branch> [--base <branch>] [--push] [--no-fetch]
#
#   --base   merge origin/<branch> instead of the PR's base (default: the base
#            of the PR for <branch> when gh knows it, else main)
#   --push   plain `git push origin HEAD` after a clean merge
#
# Exit status: 0 up to date or merged, 1 conflicts left for a manual fix,
# 2 usage or tool error.

source "$(dirname "$0")/lib.sh"

# Files whose conflicts are "both sides added entries": kept as ours then theirs.
UNION_FILES="docs/plans/2026-11-10-phase-2-3-tracker.md errors/HISTORY.md"

branch=""
base=""
push=0
fetch=1
while [ $# -gt 0 ]; do
  case "$1" in
    --base) [ $# -ge 2 ] || die "--base needs a branch"; base="$2"; shift ;;
    --push) push=1 ;;
    --no-fetch) fetch=0 ;;
    -*) die "unknown option $1" ;;
    *) [ -z "$branch" ] || die "one branch only"; branch="$1" ;;
  esac
  shift
done
[ -n "$branch" ] || die "usage: scripts/unix/pr-sync.sh <branch> [--base <branch>] [--push] [--no-fetch]"

[ "$fetch" -eq 0 ] || git fetch -q origin
if [ -z "$base" ] && command -v gh >/dev/null 2>&1; then
  base="$(gh pr view "$branch" --json baseRefName --jq .baseRefName 2>/dev/null || true)"
fi
base="${base:-main}"
git rev-parse --verify -q "origin/$base" >/dev/null || die "origin/$base does not exist"

# The worktree that has the branch checked out, if any.
worktree="$(git worktree list --porcelain | awk -v ref="refs/heads/$branch" '
  $1 == "worktree" { path = substr($0, 10) }
  $1 == "branch" && $2 == ref { print path; exit }')"
if [ -z "$worktree" ]; then
  worktree="$(dirname "$ROOT")/$(basename "$ROOT")-sync-$(printf '%s' "$branch" | tr '/' '-')"
  [ ! -e "$worktree" ] || die "$worktree exists but is not a worktree of $branch"
  if git rev-parse --verify -q "refs/heads/$branch" >/dev/null; then
    git worktree add -q "$worktree" "$branch"
  else
    git rev-parse --verify -q "origin/$branch" >/dev/null || die "no branch $branch, local or on origin"
    git worktree add -q -B "$branch" "$worktree" "origin/$branch"
  fi
  echo "worktree: $worktree (new)"
else
  echo "worktree: $worktree"
fi
cd "$worktree"

if [ ! -f "$(git rev-parse --git-dir)/MERGE_HEAD" ]; then
  [ -z "$(git status --porcelain)" ] || die "$worktree has uncommitted changes"
  # A checkout that lags behind origin/<branch> (the branch moved from another
  # clone or worktree) is brought up to date first; one that has diverged is
  # not touched, since the push would be rejected.
  if git rev-parse --verify -q "origin/$branch" >/dev/null && ! git merge-base --is-ancestor "origin/$branch" HEAD; then
    git merge-base --is-ancestor HEAD "origin/$branch" || die "$branch has diverged from origin/$branch; reconcile it first"
    git merge --ff-only -q "origin/$branch"
    echo "fast-forwarded $branch to origin/$branch"
  fi
  if git merge-base --is-ancestor "origin/$base" HEAD; then
    echo "$branch already contains origin/$base"
  else
    git merge --no-edit "origin/$base" >/dev/null 2>&1 || true
  fi
fi

# Conflicts: resolve the ledgers, report the rest.
rest=""
for file in $(git diff --name-only --diff-filter=U); do
  case " $UNION_FILES " in
    *" $file "*)
      awk '
        /^<<<<<<< /      { inconf = 1; next }
        inconf && /^\|\|\|\|\|\|\| / { skip = 1; next }
        inconf && /^=======$/        { skip = 0; next }
        inconf && /^>>>>>>> /        { inconf = 0; next }
        !skip { print }' "$file" >"$file.union" && mv "$file.union" "$file"
      git add "$file"
      echo "kept both sides: $file" ;;
    *) rest="$rest $file" ;;
  esac
done
if [ -n "$rest" ]; then
  echo "conflicts to fix by hand in $worktree:"
  for file in $rest; do echo "  $file"; done
  exit 1
fi
if [ -f "$(git rev-parse --git-dir)/MERGE_HEAD" ]; then
  git commit -q --no-edit
  echo "merged origin/$base into $branch: $(git rev-parse --short HEAD)"
fi

if [ "$push" -eq 1 ]; then
  git push origin HEAD 2>&1 | tail -1
fi
