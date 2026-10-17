#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"

echo "=== Aggregating review reports ==="

REVISION=$(git log -1 --format="%H")
REPORTS_DIR=".review-output/reports"
MISSING=0

for role in safety concurrency performance dry security; do
  f="$REPORTS_DIR/$role.md"
  if [[ ! -f "$f" ]]; then
    echo "MISSING: $f"
    MISSING=$((MISSING + 1))
  else
    SIZE=$(stat -f%z "$f" 2>/dev/null || stat -c%s "$f" 2>/dev/null || echo "0")
    if [[ "$SIZE" -lt 100 ]]; then
      echo "WARNING: $f is very small ($SIZE bytes). May be truncated."
    else
      echo "  OK $f ($SIZE bytes)"
    fi
  fi
done

if [[ "$MISSING" -gt 0 ]]; then
  echo "=== ERROR: $MISSING reports missing. Consolidation INCOMPLETE. ==="
  exit 1
fi

AGG_PROMPT="Eres el agente agregador. Recibes 5 reportes de revision del workspace mini-consumes-tokens (revision: $REVISION). Los archivos estan en $REPORTS_DIR/: safety.md, concurrency.md, performance.md, dry.md, security.md. Tu trabajo: 1) deduplicar hallazgos que apunten al mismo archivo:linea, 2) resolver contradicciones, 3) verificar leyendo el codigo real, 4) clasificar cada hallazgo como confirmado, probable, no verificado o descartado, 5) normalizar severidad, 6) priorizar en 4 fases con checkboxes, 7) incluir resumen ejecutivo y apendice de verificacion. No anadas hallazgos nuevos. No edites archivos."

codex exec \
  --sandbox read-only \
  --model gpt-5.6-sol \
  --config "model_reasoning_effort=medium" \
  -o ".review-output/aggregated.md" \
  "$AGG_PROMPT" \
  > ".review-output/logs/aggregator.log" 2>&1

echo "=== Aggregation complete ==="
echo "Output: .review-output/aggregated.md"
