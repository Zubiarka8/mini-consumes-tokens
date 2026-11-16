#!/usr/bin/env bash
# Every open PR in one screen: merge state, base branch when it is not main,
# and the CI checks passed, failed, still running. By default only the PRs that
# need attention (conflicting, blocked, failing or running) are listed. Uses
# one small `gh` query per PR instead of one huge GraphQL query, which times
# out (HTTP 504) on repositories with many PRs; each call is retried.
#
# Usage:
#   scripts/unix/pr-overview.sh [--all] [--limit N]
#
# Exit status: 0 nothing failing or conflicting, 1 some PR fails or conflicts,
# 2 usage or tool error. Needs gh.

source "$(dirname "$0")/lib.sh"

command -v gh >/dev/null 2>&1 || die "needs the GitHub CLI (gh)"
all=0
limit=100
while [ $# -gt 0 ]; do
  case "$1" in
    --all) all=1 ;;
    --limit) [ $# -ge 2 ] || die "--limit needs a number"; limit="$2"; shift ;;
    *) die "usage: scripts/unix/pr-overview.sh [--all] [--limit N]" ;;
  esac
  shift
done

# Runs a gh command up to three times; prints its output or nothing.
retry() {
  local attempt out
  for attempt in 1 2 3; do
    if out="$("$@" 2>/dev/null)"; then printf '%s\n' "$out"; return 0; fi
    sleep "$attempt"
  done
  return 1
}

LOG="$LOG_DIR/pr-overview.log"
new_log "$LOG"
list="$(retry gh pr list --state open --limit "$limit" --json number,headRefName,baseRefName,mergeStateStatus \
  --jq '.[] | "\(.number)\t\(.mergeStateStatus)\t\(.baseRefName)\t\(.headRefName)"')" ||
  die "could not list the open PRs"

bad=0
shown=0
total=0
while IFS="$(printf '\t')" read -r number state base head; do
  [ -n "$number" ] || continue
  total=$((total + 1))
  counts="$(retry gh pr checks "$number" --json bucket \
    --jq '[.[].bucket] | group_by(.) | map("\(.[0])=\(length)") | join(" ")' || echo "checks unavailable")"
  stack=""
  [ "$base" = "main" ] || stack=" base=$base"
  line="#$number $state$stack $head [$counts]"
  printf '%s\n' "$line" >>"$LOG"
  case "$state:$counts" in
    DIRTY:* | *fail=*) bad=1 ;;
  esac
  if [ "$all" -eq 1 ] || [ "$state" != "CLEAN" ]; then
    echo "$line"
    shown=$((shown + 1))
  fi
done <<EOF
$list
EOF
echo "$shown of $total open PR(s) listed; full list in $LOG"
exit "$bad"
