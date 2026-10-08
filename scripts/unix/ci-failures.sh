#!/usr/bin/env bash
# Why a CI run failed, in one screen: the jobs that failed and the first error
# lines of each, instead of the full `gh run view --log-failed` output. The
# full log goes to target/script-logs/ci-failures.log.
#
# Usage:
#   scripts/unix/ci-failures.sh              # latest failed run on the current branch
#   scripts/unix/ci-failures.sh <run-id>     # a specific run
#
# Exit status: 0 when there is a failure to report (or none to report), 2 on usage errors.

source "$(dirname "$0")/lib.sh"

command -v gh >/dev/null 2>&1 || die "needs the GitHub CLI (gh)"
LOG="$LOG_DIR/ci-failures.log"
MAX_LINES=40

run="${1:-}"
if [ -z "$run" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD)"
  run="$(gh run list --branch "$branch" --status failure --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  if [ -z "$run" ]; then
    echo "no failed CI run on branch $branch"
    exit 0
  fi
fi

new_log "$LOG"
gh run view "$run" --log-failed >>"$LOG" 2>&1 || true

echo "CI run $run failed in:"
gh run view "$run" --json jobs --jq '.jobs[] | select(.conclusion == "failure") | "  - " + .name' |
  tee -a "$LOG" | cut -c1-160

# Error headlines: compiler errors, panics and failed assertions, with the line
# after each (a panic's message, an assertion's left/right). `gh --log-failed`
# prefixes every line with job, step and timestamp, and a thread id differs per
# run, so both are stripped before de-duplicating.
headlines="$(awk -F'\t' '{ print $NF }' "$LOG" |
  sed -E 's/^[0-9T:.-]+Z //; s/\([0-9]+\)//' |
  grep -E -A1 'error(\[[A-Z0-9]+\])?:|panicked at|assertion .*failed|^test .* FAILED' || true)"
if [ -z "$headlines" ]; then
  echo "no error headline matched; read $LOG"
else
  echo "first errors:"
  printf '%s\n' "$headlines" | grep -v '^--$' | sed 's/^ *//' | awk '!seen[$0]++' |
    cap "$MAX_LINES" "$LOG" | cut -c1-200
fi
