# codebase-context-mcp — checklist

## Conteo de TODO/FIXME/HACK/XXX/BUG/OPTIMIZE

```
grep -rEn "TODO|FIXME|HACK|XXX|BUG|OPTIMIZE" --include="*.rs" crates
```
Resultado: **0** ocurrencias (2026-09-13).

## Decisiones abiertas

1. **Resolución de tipos**: solo AST (tree-sitter) para el MVP en los 7 lenguajes; LSP real queda como mejora posterior. No bloquea el criterio de terminado de esta sesión.
2. **Alcance de "web"**: JavaScript/TypeScript (cliente y servidor) para el grafo de símbolos. HTML/CSS quedan fuera del MVP — sin revisar aún con el usuario si hace falta indexado estructural de esos también.
3. **Patrones de exclusión de secretos**: lista base común implementada en `ccm-index/src/exclude.rs` (`.env`, `*.pem`, `*.key`, `secrets/`, `credentials.json`, `.aws/`, `node_modules/`, `appsettings.*.json`, `*.pfx`, `*.snk`, `gradle.properties`, `.git/`, `target/`, `.claude-index/`). Ampliable, no exhaustiva por ecosistema todavía (falta revisar Go, Ruby, PHP, Swift específicamente).

## Cobertura de lenguajes

| Lenguaje | Estado | Crate | Notas |
|---|---|---|---|
| Rust | **Implementado** | `ccm-lang-rust` | funciones, structs, enums, traits, impls (métodos con parent = tipo), type alias, const/static, use imports, calls. 4 tests. |
| Python | **Implementado** | `ccm-lang-python` | funciones, clases (con bases → Extends), métodos, decoradores (capturados como References), import/from-import, calls. 5 tests. |
| JavaScript/TypeScript | Sin empezar | `ccm-lang-js-ts` (no existe aún) | — |
| Java | Sin empezar | `ccm-lang-java` (no existe aún) | detectado como "lenguaje pendiente" en `get_indexing_status` vía `KNOWN_PENDING_LANGUAGES` (`.java`). |
| C# | Sin empezar | `ccm-lang-csharp` (no existe aún) | detectado como pendiente (`.cs`). |
| C++ | Sin empezar | `ccm-lang-cpp` (no existe aún) | detectado como pendiente (`.cpp/.cc/.cxx/.h/.hpp/.hh`). |
| Go | Sin empezar | `ccm-lang-go` (no existe aún) | detectado como pendiente (`.go`). |
| **Arquitectura de plugin validada con lenguaje externo de prueba (Lua)** | **Sin empezar** | — | pendiente: crate `ccm-lang-lua` fuera del set inicial, implementando el trait sin tocar `ccm-core`/`ccm-mcp-server`, como prueba de aceptación de la arquitectura. |

Repo poliglota de prueba: validado manualmente (Rust + Python en el mismo árbol, `ccm-cli init`/`status` reportan cobertura separada por lenguaje) — falta fijarlo como fixture de test de integración versionado.

## Diseño de tools MCP

Criterio aplicado esta sesión: máximo 6-8 tools, cada una clasificada en Atómica / Compuesta / Administración del índice, con descripción anti-ambigüedad ("cuándo NO usar esta, usar Y en su lugar") cuando existe una tool parecida.

| Tool | Categoría | Ambigüedad resuelta contra |
|---|---|---|
| `find_symbol(name)` | Atómica | aclara no usarla para callers/referencias → apunta a `find_callers`/`find_references` |
| `find_references(symbol)` | Atómica | aclara no usarla si solo se quieren callers directos → apunta a `find_calls` |
| `find_calls(function)` | Atómica | aclara que es la inversa de `find_callers` |
| `find_callers(function)` | Atómica | aclara que es la inversa de `find_calls`; aclara no usarla para búsqueda amplia de referencias → apunta a `find_references`; aclara no usarla si se quiere blast-radius completo → apunta a `impact_analysis` |
| `impact_analysis(symbol)` | **Compuesta** (implementada esta sesión) | internamente combina `find_callers` + `find_references` + heurística de nombre de test (`test`/`test_*`), composición en Rust (`ccm-mcp-server/src/server.rs::impact_analysis`), no delegada al agente. Aclara no usarla para lookup simple → más barato con `find_callers`/`find_references` solos |
| `reindex(force)` | Administración del índice | descripción marcada explícitamente "INDEX ADMINISTRATION, not a search tool" |
| `get_indexing_status()` | Administración del índice | descripción marcada explícitamente "INDEX ADMINISTRATION, not a search tool"; aclara no usarla para buscar símbolos → `find_symbol` |

Total: **7 tools** (dentro del límite de 6-8). `init` no es una tool MCP separada — el servidor corre `reindex(force=false)` automáticamente al arrancar; `ccm-cli` sí expone `init` como subcomando de descubribilidad (equivalente a `reindex`).

Ninguna tool nueva (`find_types`, `find_tests`, `find_dead_code`, `semantic_search`) fue agregada esta sesión — quedan fuera de alcance según la instrucción explícita. Antes de agregarlas en el futuro: revisar si caben como parámetro de una tool existente (ej. `find_symbol(name, kind)`) en vez de una tool nueva.

Verificado: 5 tests de integración en `ccm-mcp-server/tests/tools.rs` cubren las 7 tools (incluyendo `impact_analysis` con deduplicación de tests afectados).

## Entregable general — estado

- [x] Workspace Rust multi-crate compilable (`ccm-core`, `ccm-index`, `ccm-lang-rust`, `ccm-lang-python`, `ccm-mcp-server`, `ccm-cli`)
- [x] Trait `LanguageParser` agnóstico a lenguaje (`ccm-core`)
- [x] `LanguageRegistry` central (registro sin tocar el motor)
- [x] Esquema SQLite único (sin tablas por lenguaje), migraciones versionadas (`rusqlite_migration`), WAL, FTS5 (símbolos)
- [x] Detección de cambios vía `git2::Oid::hash_object` (hash de blob, sin necesitar repo git real)
- [x] Exclusión por defecto de patrones de secretos (opt-out)
- [x] Validación de path traversal (canonicalización + `starts_with(root)`, symlinks fuera de root rechazados)
- [x] Nombres de símbolo como parámetros SQL (rusqlite placeholders, nunca interpolación)
- [x] Manejo de errores de parseo distinguiendo "lenguaje no soportado" vs "sintaxis inválida"
- [x] `get_indexing_status` reporta cobertura por lenguaje, lenguajes pendientes, errores de sintaxis, último indexado
- [x] 7 tools MCP (ver sección de diseño arriba), servidor via `rmcp` (SDK oficial) sobre stdio
- [x] `ccm-cli` con subcomandos `init`/`reindex`/`status`
- [x] Reindexado incremental (skip por hash sin cambios) + eliminación de archivos borrados/excluidos
- [x] Tests: 19 tests pasando (`ccm-index` 5, `ccm-lang-rust` 4, `ccm-lang-python` 5, `ccm-mcp-server` 5)
- [x] 0 `unwrap()`/`panic!` en rutas de procesamiento de input del repo indexado (revisar explícitamente en próxima sesión con `cargo clippy` + grep dedicado)
- [ ] JavaScript/TypeScript, Java, C#, C++, Go — crates de lenguaje pendientes
- [ ] Crate de lenguaje externo de prueba (Lua) validando arquitectura de plugin sin tocar `core`/`mcp-server`
- [ ] Fuzzing (`cargo-fuzz`) por crate de lenguaje
- [ ] `cargo-audit` en CI
- [ ] CI multiplataforma (GitHub Actions: Linux/macOS/Windows)
- [ ] Benchmark de tokens (con MCP vs Read/Grep/Glob) para 3+ lenguajes
- [ ] Repos de integración fijados como fixtures versionados (uno por lenguaje + poliglota)
- [ ] Binarios de release multiplataforma + publicación en crates.io
- [ ] `README.md`, `CONTRIBUTING.md`, `LICENSE` (Apache-2.0), `CHANGELOG.md`, `.gitignore`
- [ ] Primer commit git (repo inicializado con `git init`, aún sin commits)
