#!/usr/bin/env bash
# Run the full repository check in an isolated Cargo target directory, continue
# through all test targets after failures, and write a Markdown result table.
# Full logs and the table live under target/script-logs/.

set -euo pipefail

SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPTS_DIR/../.." && pwd)"
cd "$ROOT"

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  cat <<'EOF'
Usage: scripts/unix/review-check.sh [check.sh options]

Runs scripts/unix/check.sh with --isolated-target and --no-fail-fast, then
writes target/script-logs/review-check-table.md. Options such as --no-eval,
--only, and -p are forwarded to check.sh.

Set MCT_CHECK_TARGET_DIR to override the isolated Cargo target directory.
EOF
  exit 0
fi

export MCT_CHECK_LOG_DIR="$ROOT/target/script-logs/review-check"
export MCT_CHECK_SUMMARY="$MCT_CHECK_LOG_DIR/check-summary.md"

set +e
"$SCRIPTS_DIR/check.sh" --isolated-target --no-fail-fast "$@"
check_status=$?
set -e

summary="$MCT_CHECK_SUMMARY"
table="$ROOT/target/script-logs/review-check-table.md"
target_dir="${MCT_CHECK_TARGET_DIR:-$ROOT/target/review-check}"
case "$target_dir" in
  /*) ;;
  *) target_dir="$ROOT/$target_dir" ;;
esac

{
  printf '# Review check results\n\n'
  printf -- '- Checkout: `%s`\n' "$ROOT"
  printf -- '- Cargo target: `%s`\n' "$target_dir"
  printf -- '- Script exit code: `%s`\n\n' "$check_status"
  printf '| Check | Command | Result | Details |\n| --- | --- | --- | --- |\n'
  awk '
    $1 == "fmt" || $1 == "test" || $1 == "clippy" || $1 == "eval" {
      result = $2
      details = $0
      sub(/^[^[:space:]]+[[:space:]]+[^[:space:]]+[[:space:]]*/, "", details)
      gsub(/\|/, "\\|", details)
      command = $1 == "fmt" ? "cargo fmt --all --check" : \
        ($1 == "test" ? "cargo test --workspace --no-fail-fast" : \
        ($1 == "clippy" ? "cargo clippy --workspace --all-targets --all-features" : "cargo run -q -p mct-eval"))
      printf "| %s | `%s` | %s | %s |\n", $1, command, result, details
    }
    $1 == "all" && $2 == "checks" { overall = "passed" }
    $1 == "some" && $2 == "checks" { overall = "failed" }
    END { if (overall == "") overall = "unknown"; printf "| Overall | | %s | exit %s |\n", overall, "'"$check_status"'" }
  ' "$summary"
  if [ -f "$MCT_CHECK_LOG_DIR/check-test.log" ] &&
    grep -q '^error: [0-9][0-9]* targets failed:' "$MCT_CHECK_LOG_DIR/check-test.log"; then
    printf '\nFailed test targets:\n\n'
    awk '/^error: [0-9][0-9]* targets failed:/ { listing = 1; next }
      listing && /^    `/ { sub(/^    `/, "- `"); sub(/`$/, "`"); print; next }
      listing { exit }' "$MCT_CHECK_LOG_DIR/check-test.log"
  fi
} >"$table"

printf 'table   %s\n' "${table#"$ROOT/"}"
exit "$check_status"
