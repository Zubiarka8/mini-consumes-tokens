#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Review cleanup ==="

pkill -f "codex.*exec" 2>/dev/null || true
pkill -f "codex.*daemon" 2>/dev/null || true

if [[ -d ".review-output" ]]; then
  ARCHIVE=".review-output/archive-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "$ARCHIVE"
  cp .review-output/*.md "$ARCHIVE/" 2>/dev/null || true
  cp .review-output/reports/*.md "$ARCHIVE/" 2>/dev/null || true
  echo "Archived to $ARCHIVE/"
fi

echo "=== Source file check ==="
CHANGES=$(git status --short | grep -v "^??" | wc -l)
if [[ "$CHANGES" -gt 0 ]]; then
  echo "WARNING: $CHANGES tracked files modified during review"
  git status --short
  exit 1
else
  echo "No tracked files modified. Sandbox worked correctly."
fi

echo "=== Cleanup complete ==="
