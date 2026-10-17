#!/usr/bin/env bash
# Makes every project-local skill available in a worktree. Worktrees only get
# what is committed, so a skill that is untracked in the main checkout (or a
# worktree cut from an older commit) would be missing for any agent working
# there. This copies the missing skills from the primary checkout's
# .agents/skills and makes sure .claude/skills points at them.
#
# Never overwrites a file the target already has, never deletes anything, and
# skips caches. Safe to run repeatedly.
#
# Usage:
#   scripts/unix/sync-skills.sh              # this checkout
#   scripts/unix/sync-skills.sh <worktree>   # another worktree
#
# Exit status: 0 on success, 2 on usage or a missing source.

source "$(dirname "$0")/lib.sh"

[ $# -le 1 ] || die "usage: scripts/unix/sync-skills.sh [worktree]"
target="${1:-$ROOT}"
[ -d "$target" ] || die "not a directory: $target"
target="$(cd "$target" && pwd)"

# The primary checkout is the first entry of `git worktree list`.
primary="$(git worktree list --porcelain | sed -n 's/^worktree //p' | head -n1)"
source_dir="$primary/.agents/skills"
[ -d "$source_dir" ] || die "no skills in the primary checkout: $source_dir"

mkdir -p "$target/.agents/skills"
if [ "$source_dir" != "$target/.agents/skills" ]; then
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --ignore-existing --exclude __pycache__ --exclude .DS_Store \
      "$source_dir/" "$target/.agents/skills/"
  else
    (cd "$source_dir" && find . -type f ! -path '*/__pycache__/*' ! -name .DS_Store -print0) |
      while IFS= read -r -d '' f; do
        [ -e "$target/.agents/skills/$f" ] && continue
        mkdir -p "$(dirname "$target/.agents/skills/$f")"
        cp -p "$source_dir/$f" "$target/.agents/skills/$f"
      done
  fi
fi

# .claude/skills -> ../.agents/skills (tracked once .gitignore un-ignores it).
mkdir -p "$target/.claude"
[ -e "$target/.claude/skills" ] || [ -L "$target/.claude/skills" ] ||
  ln -s ../.agents/skills "$target/.claude/skills"

count="$(find "$target/.agents/skills" -mindepth 2 -maxdepth 2 -name SKILL.md | wc -l | tr -d ' ')"
echo "skills available in $target: $count (.agents/skills, .claude/skills)"
