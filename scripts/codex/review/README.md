# Revision multi-agente con Codex

Este directorio contiene el flujo de revision multi-agente del workspace mini-consumes-tokens.

## Estructura

- .codex/config.toml: configuracion global de subagentes.
- .codex/agents/*.toml: definicion de los 5 revisores y el agregador.
- AGENTS.md: instrucciones para el agente raiz.
- scripts/codex/review/: scripts de orquestacion.
- .review-output/: reportes y logs generados.

## Roles

- safety: memoria, FFI, tree-sitter, unsafe, unwrap, aritmetica.
- concurrency: async, data races, locks, cancelacion, cache MCP.
- performance: allocations, SQLite, tree-sitter, ONNX, compilacion.
- dry: duplicacion cross-crate entre mct-lang-*.
- security: input no confiable, DoS, path traversal, inyeccion.
- aggregator: deduplica, verifica y prioriza los 5 reportes.

## Orden de ejecucion

1. ./scripts/codex/review/00-setup-dirs.sh
2. ./scripts/codex/review/01-validate-config.sh
3. ./scripts/codex/review/02-validate-agents.sh
4. ./scripts/codex/review/03-launch-native.sh (modo nativo)
   o ./scripts/codex/review/04-launch-parallel.sh (modo paralelo manual)
5. Si el agregador no se lanzo: ./scripts/codex/review/05-aggregate.sh
6. ./scripts/codex/review/06-cleanup.sh

## Requisitos

- Codex CLI 0.162.1 o superior.
- Modelo gpt-5.6-sol disponible.
- Daemon de Codex detenido antes de lanzar.
- Estado del repo apuntado antes de la revision.

## Salida

- .review-output/aggregated.md: reporte consolidado.
- .review-output/reports/: reportes individuales.
- .review-output/logs/: logs de cada worker.

## Advertencias

- La revision es de solo lectura. El sandbox debe ser read-only.
- No permitir que los subagentes spawneen mas subagentes.
- No superar max_depth 1 ni max_threads 6.
- Si un worker falla, la consolidacion es incompleta.
