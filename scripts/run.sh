#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

CMD="${1:-}"
shift || true

case "$CMD" in
  help)          ./scripts/help.sh "$@" ;;
  check)         ./scripts/unix/check.sh "$@" ;;
  ci)            ./scripts/unix/check.sh "$@" && ./scripts/unix/ci-failures.sh "$@" ;;
  pr-body)       ./scripts/unix/pr-body.sh "$@" ;;
  pr-status)     ./scripts/unix/pr-status.sh "$@" ;;
  corpus)        ./scripts/unix/corpus-report.sh "$@" ;;
  tokens)        ./scripts/unix/token-report.sh "$@" ;;
  mcp)           ./scripts/unix/mcp-smoke.sh "$@" ;;
  review)
    ./scripts/codex/review/00-setup-dirs.sh
    ./scripts/codex/review/01-validate-config.sh
    ./scripts/codex/review/02-validate-agents.sh
    ./scripts/codex/review/03-launch-native.sh
    ;;
  review-parallel)
    ./scripts/codex/review/00-setup-dirs.sh
    ./scripts/codex/review/01-validate-config.sh
    ./scripts/codex/review/02-validate-agents.sh
    ./scripts/codex/review/04-launch-parallel.sh
    ./scripts/codex/review/05-aggregate.sh
    ;;
  review-cleanup)
    ./scripts/codex/review/06-cleanup.sh
    ;;
  "")
    echo "Available subcommands:"
    echo "  help             Show this help"
    echo "  check            Run local checks"
    echo "  ci               Run checks and CI failures"
    echo "  pr-body          Generate PR body"
    echo "  pr-status        Show PR status"
    echo "  corpus           Corpus report"
    echo "  tokens           Token savings report"
    echo "  mcp              MCP smoke test"
    echo "  review           Full native multi-agent review"
    echo "  review-parallel  Full parallel review"
    echo "  review-cleanup   Cleanup review artifacts"
    ;;
  *)
    echo "Unknown subcommand: $CMD"
    echo "Run ./scripts/run.sh help for available subcommands."
    exit 1
    ;;
esac
