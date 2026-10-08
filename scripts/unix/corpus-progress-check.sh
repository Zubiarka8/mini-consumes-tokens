#!/usr/bin/env bash
# Checks issue #74's progress table against GitHub: every row whose status is
# "In PR" or "Done" must name a PR that is merged (Done) or still open (In PR).
# The status is set by hand, so it goes stale as soon as a PR merges and nobody
# edits the row; this makes that visible before picking the next language.
#
# Usage:
#   scripts/unix/corpus-progress-check.sh          # one line per row, exit 1 if any is stale
#
# Needs the GitHub CLI (`gh`) authenticated for this repository. Offline checks
# of the table's own consistency run as a test in CI (repo_ledgers.rs).

source "$(dirname "$0")/lib.sh"

command -v gh >/dev/null 2>&1 || die "needs the GitHub CLI (gh)"
PROGRESS="internal/corpus-progress.md"
[ -f "$PROGRESS" ] || die "$PROGRESS not found"

# `language<TAB>status<TAB>PR number` for each language row, status without bold markers.
rows="$(awk -F'|' '
  /^\|/ && /`mct-lang-/ {
    gsub(/^ +| +$/, "", $2); gsub(/\*/, "", $4); gsub(/^ +| +$/, "", $4);
    gsub(/^ +#| +$/, "", $5);
    printf "%s\t%s\t%s\n", $2, $4, $5
  }' "$PROGRESS")"

repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
stale=0
while IFS="$(printf '\t')" read -r language status pr; do
  case "$status" in
    "In PR" | "Done") ;;
    *) continue ;;
  esac
  if [ -z "$pr" ]; then
    echo "✗ $language: status $status but no PR number"
    stale=$((stale + 1))
    continue
  fi
  state="$(gh pr view "$pr" --repo "$repo" --json state --jq .state)"
  case "$status:$state" in
    "In PR:MERGED")
      echo "✗ $language: PR #$pr is merged; set the row to Done" ;;
    "In PR:OPEN") echo "✓ $language: PR #$pr is open" ;;
    "Done:MERGED") echo "✓ $language: PR #$pr is merged" ;;
    *)
      echo "✗ $language: status $status but PR #$pr is $state"
      ;;
  esac
  case "$status:$state" in
    "In PR:OPEN" | "Done:MERGED") ;;
    *) stale=$((stale + 1)) ;;
  esac
done <<EOF
$rows
EOF

if [ "$stale" -gt 0 ]; then
  echo "$stale row(s) out of date in $PROGRESS"
  exit 1
fi
echo "all started rows match their PR state"
