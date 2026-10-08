#!/usr/bin/env bash
# A new branch in its own worktree, cut from origin/main (or another base), in a
# sibling directory of this checkout, so concurrent work never shares a working
# tree. Fetches origin first. Refuses to reuse an existing branch or path.
#
# Usage:
#   scripts/unix/branch-worktree.sh <branch>            # from origin/main
#   scripts/unix/branch-worktree.sh <branch> <base>     # from another ref
#
# Prints the directory to cd into. Exit status: 0 on success, 2 on usage or conflict.

source "$(dirname "$0")/lib.sh"

[ $# -ge 1 ] || die "usage: scripts/unix/branch-worktree.sh <branch> [base]"
branch="$1"
base="${2:-origin/main}"
git check-ref-format --branch "$branch" >/dev/null 2>&1 || die "invalid branch name: $branch"
git rev-parse --verify --quiet "refs/heads/$branch" >/dev/null && die "branch $branch already exists"

# <checkout>-<branch with / as ->, next to the checkout.
path="$(dirname "$ROOT")/$(basename "$ROOT")-$(echo "$branch" | tr '/' '-')"
[ -e "$path" ] && die "$path already exists"

git fetch origin --quiet || die "git fetch origin failed"
git rev-parse --verify --quiet "$base" >/dev/null || die "unknown base: $base"
git worktree add -b "$branch" "$path" "$base" >/dev/null || die "git worktree add failed"

echo "created branch $branch from $base"
echo "cd \"$path\""
