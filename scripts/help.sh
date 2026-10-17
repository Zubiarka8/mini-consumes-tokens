#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

CATEGORY="${1:-}"

if [[ -n "$CATEGORY" ]]; then
  case "$CATEGORY" in
    review)
      echo "=== Review ==="
      echo "  codex/review/00-setup-dirs.sh     Crea directorios de salida"
      echo "  codex/review/01-validate-config.sh  Valida configuracion de Codex"
      echo "  codex/review/02-validate-agents.sh  Valida archivos de agentes"
      echo "  codex/review/03-launch-native.sh    Lanza revision nativa multi-agente"
      echo "  codex/review/04-launch-parallel.sh  Lanza revision paralela manual"
      echo "  codex/review/05-aggregate.sh        Consolida reportes"
      echo "  codex/review/06-cleanup.sh          Limpia y archiva"
      ;;
    ci)
      echo "=== CI ==="
      echo "  unix/check.sh            Checks locales"
      echo "  unix/ci-failures.sh      Fallos de CI recientes"
      ;;
    pr)
      echo "=== PR ==="
      echo "  unix/pr-body.sh          Cuerpo del PR"
      echo "  unix/pr-conflicts.sh     Conflictos con rama base"
      echo "  unix/pr-status.sh        Estado del PR"
      echo "  unix/publish-branch.sh   Publica rama"
      ;;
    quality)
      echo "=== Quality ==="
      echo "  unix/corpus-progress-check.sh  Progreso del corpus"
      echo "  unix/corpus-report.sh          Reporte de corpus"
      echo "  unix/new-language-check.sh     Valida nuevo lenguaje"
      echo "  unix/new-tool-check.sh         Valida nueva herramienta"
      ;;
    perf)
      echo "=== Performance ==="
      echo "  unix/token-report.sh           Ahorro de tokens"
      echo "  unix/reindex-delay-smoke.sh    Smoke de indexacion"
      ;;
    setup)
      echo "=== Setup ==="
      echo "  unix/install-hooks.sh   Instala hooks"
      echo "  unix/reinstall.sh       Reinstala binario"
      echo "  unix/release.sh         Prepara release"
      ;;
    *)
      echo "Unknown category: $CATEGORY"
      echo "Available: review ci pr quality perf setup"
      exit 1
      ;;
  esac
  exit 0
fi

echo "=== Mini Consumers Tokens - Scripts ==="
echo ""
echo "Categorias disponibles:"
echo "  review    Revision multi-agente con Codex"
echo "  ci        Checks locales y fallos de CI"
echo "  pr        Gestion de pull requests"
echo "  quality   Calidad de corpus y lenguajes"
echo "  perf      Reportes de rendimiento"
echo "  setup     Instalacion y hooks"
echo ""
echo "Uso: ./scripts/help.sh <categoria>"
