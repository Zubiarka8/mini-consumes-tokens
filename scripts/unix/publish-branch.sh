#!/usr/bin/env bash
# Pushes a local branch to origin and opens a PR against main. Refuses (exit 1)
# when the push would need a force, or when the branch has no commits whose
# change is missing from main. If a PR is already open for the branch, it only
# pushes. Needs gh, authenticated.
#
# Usage:
#   scripts/unix/publish-branch.sh <branch> [--title <title>] [--dry-run]
#
# The remote branch is the branch's upstream when that is on origin (so a branch
# tracking an older name keeps it), otherwise the same name. Default title: the
# branch's last commit subject. The PR body lists the branch's commits and ends
# with the Claude Code footer. Exit status: 0 ok, 1 refused or failed, 2 usage.

source "$(dirname "$0")/lib.sh"

usage() {
  echo "usage: scripts/unix/publish-branch.sh <branch> [--title <title>] [--dry-run]"
}
refuse() {
  echo "refused: $*" >&2
  exit 1
}

case "${1:-}" in
  "" | -h | --help)
    usage
    [ "${1:-}" ] && exit 0
    exit 2 ;;
esac
branch="$1"
shift

title=""
dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --title)
      [ $# -ge 2 ] || { usage >&2; exit 2; }
      title="$2"
      shift 2 ;;
    --dry-run)
      dry=1
      shift ;;
    *)
      usage >&2
      exit 2 ;;
  esac
done

git rev-parse --verify --quiet "refs/heads/$branch" >/dev/null || die "no local branch $branch"
command -v gh >/dev/null || die "needs gh"
git fetch origin --quiet || die "git fetch origin failed"

upstream="$(git rev-parse --abbrev-ref "$branch@{upstream}" 2>/dev/null || true)"
[ "$upstream" = origin/main ] && upstream=""
case "$upstream" in
  origin/*) head="${upstream#origin/}" ;;
  *) head="$branch" ;;
esac

# Fast-forward only: origin's branch must already be an ancestor of ours.
if git rev-parse --verify --quiet "refs/remotes/origin/$head" >/dev/null; then
  git merge-base --is-ancestor "origin/$head" "$branch" ||
    refuse "origin/$head has commits $branch lacks; pushing would need a force. Rebase $branch onto it, or publish under another name."
fi

new="$(git cherry origin/main "$branch" | grep -c '^+' || true)"
[ "$new" -gt 0 ] || refuse "$branch has no commits whose change is missing from main; nothing to open a PR for"

pr="$(gh pr list --head "$head" --state open --json number --jq '.[0].number' 2>/dev/null || true)"

if [ "$dry" = 1 ]; then
  echo "dry run: would push $branch to origin/$head ($new commit(s) new vs main)"
  if [ -n "$pr" ]; then
    echo "dry run: PR #$pr is already open; would only push"
  else
    echo "dry run: would open a PR from $head into main, titled: ${title:-$(git log -1 --format=%s "$branch")}"
  fi
  exit 0
fi

git push -u origin "$branch:refs/heads/$head" || refuse "git push failed"

if [ -n "$pr" ]; then
  echo "pushed $branch to origin/$head; PR #$pr is already open"
  exit 0
fi

[ -n "$title" ] || title="$(git log -1 --format=%s "$branch")"
body="$(printf 'Commits:\n%s\n\n%s\n' \
  "$(git log --format='- %s' "origin/main..$branch")" \
  '🤖 Generated with [Claude Code](https://claude.com/claude-code)')"
url="$(gh pr create --base main --head "$head" --title "$title" --body "$body")" ||
  refuse "gh pr create failed (the branch is pushed; open the PR by hand or re-run)"

echo "pushed $branch to origin/$head ($new commit(s) new vs main)"
echo "PR: $url"
