#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Launching native multi-agent review ==="

pkill -f "codex.*daemon" 2>/dev/null || true
sleep 1

REVISION=$(git log -1 --format="%H")
echo "Workspace revision: $REVISION"

ROOT_PROMPT='Spawn exactly 5 subagents using the roles safety, concurrency, performance, dry, and security. Each subagent must use its configured model and reasoning effort. Wait for all 5 to complete. Then spawn exactly 1 subagent using the aggregator role. The aggregator must receive all 5 reports and consolidate them. Do not allow any subagent to spawn additional subagents. Do not exceed max_depth 1 or max_threads 6. Report the consolidated output.'

LOG=".review-output/logs/native-$(date +%Y%m%d-%H%M%S).log"

codex \
  --strict-config \
  --sandbox read-only \
  --ask-for-approval never \
  --model gpt-5.6-sol \
  "$ROOT_PROMPT" \
  2>&1 | tee "$LOG"

echo "=== Review complete. Log: $LOG ==="
