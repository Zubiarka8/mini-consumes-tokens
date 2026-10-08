#!/usr/bin/env bash
# A pull request's state in a few lines: title and state, review decision and
# merge readiness, how many checks passed, failed or are still running (with the
# first failing names), and the latest comment. Replaces reading the full PR
# JSON. `gh` does the filtering, so jq is not needed.
#
# Usage:
#   scripts/unix/pr-status.sh <number>     # e.g. scripts/unix/pr-status.sh 127

source "$(dirname "$0")/lib.sh"

command -v gh >/dev/null 2>&1 || die "needs the GitHub CLI (gh)"
[ $# -eq 1 ] || die "usage: scripts/unix/pr-status.sh <number>"

# Check results come in two shapes: CheckRun (conclusion) and StatusContext (state).
gh pr view "$1" --json number,title,state,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup,reviews,comments --jq '
  def outcome: (.conclusion // .state // "");
  def failed: outcome as $o | ["FAILURE", "ERROR", "TIMED_OUT", "ACTION_REQUIRED"] | index($o) != null;
  def passed: outcome == "SUCCESS";
  "#\(.number) \(.title) [\(.state)\(if .isDraft then ", draft" else "" end)]",
  "review: \(if (.reviewDecision // "") == "" then "none" else .reviewDecision end) · merge: \(.mergeStateStatus)",
  "checks: \([.statusCheckRollup[]? | select(passed)] | length) passed, \([.statusCheckRollup[]? | select(failed)] | length) failed, \([.statusCheckRollup[]? | select(passed or failed | not)] | length) running",
  ([.statusCheckRollup[]? | select(failed) | (.name // .context)] | if length > 0 then "failing: " + (.[0:8] | join(", ")) else empty end),
  "reviews: \(.reviews | length), comments: \(.comments | length)",
  (.comments[-1]? // empty | "last comment by \(.author.login): " + (.body | gsub("\n"; " ") | .[0:120]))
'
