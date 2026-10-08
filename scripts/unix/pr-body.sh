#!/usr/bin/env bash
# A pull request description draft whose Verification section is copied from the
# logs of the last scripts/unix/check.sh run, and whose token figures from the
# last scripts/unix/token-report.sh run, so the numbers come from a run and not
# from memory. Nothing is run here: run check.sh (and token-report.sh for a PR
# that claims a token change) first.
#
# Usage:
#   scripts/unix/pr-body.sh               # writes target/script-logs/pr-body.md
#   scripts/unix/pr-body.sh <file>        # writes to <file> instead
#
# Warns when a log was made on another commit than HEAD, or on a dirty tree:
# the numbers are then not for the code in the PR. Exit status: 0, or 2 when
# there is no check log to copy from.

source "$(dirname "$0")/lib.sh"

out="$LOG_DIR/pr-body.md"
[ $# -ge 1 ] && out="$(abspath "$1")"
check="$LOG_DIR/check-summary.md"
tokens="$LOG_DIR/token-report.md"
[ -f "$check" ] || die "no $check: run scripts/unix/check.sh first"

head_rev="$(git rev-parse --short HEAD)"
warn() { echo "warning: $*" >&2; }

# A log's stamp ends with `branch@commit[-dirty]`; the commit must be HEAD's.
stamp_of() { head -n1 "$1" | sed -E 's/^(<!-- )?//; s/( -->)?$//'; }
check_stamp="$(stamp_of "$check")"
stale=""
# The stamp is `# <command> — <date> — <branch>@<commit>`; keep the last part for display.
revision="${check_stamp##* — }"
case "$check_stamp" in *"@$head_rev"*) ;; *) stale="check ran on another commit than HEAD ($head_rev)" ;; esac
case "$check_stamp" in *-dirty) stale="${stale:+$stale; }check ran on a dirty tree" ;; esac
case "$check_stamp" in
  *"--only"* | *" -p "* | *"--no-eval"*) stale="${stale:+$stale; }check ran partially (${check_stamp%% — *}), not every step" ;;
esac
if [ -f "$tokens" ]; then
  case "$(stamp_of "$tokens")" in *"@$head_rev"*) ;; *) stale="${stale:+$stale; }token figures are from another commit than HEAD" ;; esac
fi
[ -n "$stale" ] && warn "$stale; rerun the scripts before opening the PR"

{
  echo "## Summary"
  echo ""
  echo "<!-- what changes and why -->"
  echo ""
  echo "## Verification"
  echo ""
  [ -n "$stale" ] && echo "<!-- WARNING: $stale -->"
  echo '```text'
  tail -n +2 "$check" | grep -v '^$'
  echo '```'
  echo ""
  echo "Measured on \`$revision\` with \`scripts/unix/check.sh\`."
  if [ -f "$tokens" ]; then
    echo ""
    echo "## Token measurements"
    echo ""
    # The report's own headings are level 2; under this PR's sections they become level 3.
    tail -n +2 "$tokens" | sed -E 's/^## /### /'
  fi
  echo ""
  echo "## Not in scope"
  echo ""
  echo "<!-- what this PR does not do -->"
  echo ""
  echo "🤖 Generated with [Claude Code](https://claude.com/claude-code)"
} >"$out"

echo "wrote $out"
