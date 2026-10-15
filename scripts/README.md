# Scripts

Inventario de scripts del repositorio, agrupados por dominio funcional.

## CI

- unix/check.sh: ejecuta los checks locales antes de commitear.
- unix/ci-failures.sh: resume fallos de CI recientes.

## PR y ramas

- unix/pr-body.sh: genera el cuerpo del PR.
- unix/pr-conflicts.sh: detecta conflictos con la rama base.
- unix/pr-status.sh: muestra el estado del PR actual.
- unix/publish-branch.sh: publica la rama actual.
- unix/branch-worktree.sh: crea un worktree para trabajo aislado.
- unix/branches-status.sh: estado de todas las ramas.

## Calidad

- unix/corpus-progress-check.sh: progreso de indexacion del corpus.
- unix/corpus-report.sh: reporte de calidad del corpus.
- unix/new-language-check.sh: valida un nuevo parser de lenguaje.
- unix/new-tool-check.sh: valida una nueva herramienta MCP.
- unix/parse-probe.sh: prueba de parsing sobre un archivo concreto.

## Rendimiento

- unix/token-report.sh: reporte de ahorro de tokens.
- unix/reindex-delay-smoke.sh: smoke test de indexacion incremental.

## Setup

- unix/install-hooks.sh: instala los git hooks.
- unix/reinstall.sh: reinstala el binario local.
- unix/release.sh: prepara una release.

## MCP

- unix/mcp-smoke.sh: smoke test del servidor MCP.

## Review

- codex/review/00-setup-dirs.sh: crea directorios de salida.
- codex/review/01-validate-config.sh: valida configuracion de Codex.
- codex/review/02-validate-agents.sh: valida archivos de agentes.
- codex/review/03-launch-native.sh: lanza revision nativa multi-agente.
- codex/review/04-launch-parallel.sh: lanza revision paralela manual.
- codex/review/05-aggregate.sh: consolida reportes.
- codex/review/06-cleanup.sh: limpia y archiva.

## Convenciones

- Los scripts imprimen un resumen corto en terminal.
- El output completo se redirige a target/script-logs/ o .review-output/logs/.
- Los scripts de Unix son compatibles con bash 3.2+ (macOS por defecto).
- Los scripts de Windows son PowerShell 5.1+.
- Cada script tiene un proposito unico.
- Los scripts no modifican el estado del repositorio salvo los que lo declaran.
- Los scripts de revision son estrictamente de solo lectura.
- Subcomandos disponibles con ./scripts/run.sh <subcomando>.
- Ayuda disponible con ./scripts/help.sh <categoria>.
