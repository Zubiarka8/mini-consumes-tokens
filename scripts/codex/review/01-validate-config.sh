#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Validating Codex configuration ==="

if ! command -v codex >/dev/null 2>&1; then
  echo "ERROR: codex not found in PATH"
  exit 1
fi
echo "Codex version: $(codex --version 2>&1 | head -1)"

DAEMON_PID=$(pgrep -f "codex.*daemon" 2>/dev/null || true)
if [[ -n "$DAEMON_PID" ]]; then
  echo "WARNING: Codex daemon running (PID: $DAEMON_PID). Stopping..."
  pkill -f "codex.*daemon" 2>/dev/null || true
  sleep 2
fi

if [[ ! -f "Cargo.toml" ]]; then
  echo "ERROR: Not in workspace root (no Cargo.toml)"
  exit 1
fi

for f in .codex/config.toml AGENTS.md; do
  if [[ ! -f "$f" ]]; then
    echo "ERROR: $f missing"
    exit 1
  fi
  echo "  OK $f"
done

for role in safety concurrency performance dry security aggregator; do
  f=".codex/agents/$role.toml"
  if [[ ! -f "$f" ]]; then
    echo "ERROR: $f missing"
    exit 1
  fi
  echo "  OK $f"
done

REVISION=$(git log -1 --format="%H")
echo "Workspace revision: $REVISION"

CHANGES=$(git status --short | wc -l)
if [[ "$CHANGES" -gt 0 ]]; then
  echo "WARNING: $CHANGES uncommitted changes present"
  git status --short
fi

echo "=== Config validation complete ==="
