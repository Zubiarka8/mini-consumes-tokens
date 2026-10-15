#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Launching parallel review (5 workers) ==="

mkdir -p .review-output/reports .review-output/logs

REVISION=$(git log -1 --format="%H")
echo "Workspace revision: $REVISION"

declare -A ROLE_PROMPTS=(
  [safety]="Revisa seguridad de memoria, bloques unsafe, FFI con tree-sitter, unwrap, expect, panic, aritmetica e indices. Alcance: mct-lang-*, mct-core. Solo lectura."
  [concurrency]="Revisa concurrencia, async, data races, locks, cancelacion, cache MCP. Alcance: mct-index, mct-mcp-server, mct-core. Solo lectura."
  [performance]="Revisa performance: allocations, tree-sitter, SQLite, cache MCP, ONNX, compilacion. Alcance: mct-index, mct-lang-*, mct-cli, mct-mcp-server. Solo lectura."
  [dry]="Revisa duplicacion cross-crate entre mct-lang-* y arquitectura de abstracciones. Alcance: todo el workspace. Solo lectura."
  [security]="Revisa input no confiable, DoS, path traversal, symlinks, inyeccion, secretos. Alcance: mct-cli, mct-mcp-server, mct-index. Solo lectura."
)

PIDS=()
ROLES=()

for role in safety concurrency performance dry security; do
  echo "Spawning worker: $role"
  codex exec \
    --sandbox read-only \
    --model gpt-5.6-sol \
    --config "model_reasoning_effort=high" \
    -o ".review-output/reports/$role.md" \
    "${ROLE_PROMPTS[$role]}" \
    > ".review-output/logs/$role.log" 2>&1 &
  PIDS+=($!)
  ROLES+=("$role")
done

echo "Waiting for ${#PIDS[@]} workers..."
FAILED=0
for i in "${!PIDS[@]}"; do
  if ! wait "${PIDS[$i]}"; then
    echo "WORKER FAILED: ${ROLES[$i]}"
    FAILED=$((FAILED + 1))
  fi
done

if [[ "$FAILED" -gt 0 ]]; then
  echo "=== $FAILED workers failed. Check logs. ==="
  exit 1
fi

echo "=== All 5 workers complete ==="
echo "Reports in .review-output/reports/"
