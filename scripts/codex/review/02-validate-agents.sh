#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Validating agent files ==="

REQUIRED_FIELDS=("name" "description" "model" "model_reasoning_effort" "sandbox_mode" "developer_instructions")
FAILED=0

for f in .codex/agents/*.toml; do
  echo "--- $f ---"
  for field in "${REQUIRED_FIELDS[@]}"; do
    if grep -q "^${field} " "$f" 2>/dev/null; then
      echo "  OK $field"
    else
      echo "  MISSING $field"
      FAILED=$((FAILED + 1))
    fi
  done

  if grep -q '^sandbox_mode = "read-only"' "$f"; then
    echo "  OK sandbox_mode read-only"
  else
    echo "  ERROR sandbox_mode is NOT read-only"
    FAILED=$((FAILED + 1))
  fi
done

echo "=== Checking config.toml limits ==="
grep -E "max_depth|max_threads|multi_agent" .codex/config.toml || echo "  (no limits found)"

echo "=== Checking conflicts with rules.md ==="
if [[ -f "rules.md" ]]; then
  CONFLICTS=$(grep -c -i "delegate\|MCP\|sandbox\|approval" rules.md 2>/dev/null || echo "0")
  echo "rules.md mentions: $CONFLICTS"
  if [[ "$CONFLICTS" -gt 0 ]]; then
    grep -n -i "delegate\|MCP\|sandbox\|approval" rules.md | head -10
  fi
fi

if [[ "$FAILED" -gt 0 ]]; then
  echo "=== Validation FAILED: $FAILED issues ==="
  exit 1
fi

echo "=== Agent validation complete ==="
