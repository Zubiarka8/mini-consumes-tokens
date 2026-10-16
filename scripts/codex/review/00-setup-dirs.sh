#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Setting up review directories ==="

mkdir -p .review-output
mkdir -p .review-output/logs
mkdir -p .review-output/reports

echo "Created:"
echo "  .review-output/"
echo "  .review-output/logs/"
echo "  .review-output/reports/"

echo "=== Setup complete ==="
