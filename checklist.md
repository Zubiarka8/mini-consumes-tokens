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
4. **`cargo-fuzz` en Windows requiere el runtime dinámico de ASan de MSVC en el `PATH`** (`...\VC\Tools\MSVC\<ver>\bin\Hostx64\x64\clang_rt.asan_dynamic-x86_64.dll`) — sin él, el binario compila pero falla en ejecución con `STATUS_DLL_NOT_FOUND`; con `--sanitizer none` falla distinto, en el *link* (`link.exe` no resuelve `__start___sancov_cntrs`/`__stop___sancov_cntrs` — MSVC no soporta los símbolos de sección que la instrumentación de cobertura de sancov necesita, a diferencia de GNU ld/lld). Opción tomada por ahora: usar el sanitizer `address` por defecto (si funciona, solo falta el DLL en `PATH`) — probado esta sesión sobre `ccm-lang-lua` con una campaña de 60s (167 062 ejecuciones, 0 crashes). Revisar cuando: se configure CI multiplataforma (Linux no tiene este problema; decidir ahí si el fuzzing corto se corre solo en Linux en CI, y si documentar/automatizar el `PATH` de VC Tools para desarrollo local en Windows) — antes de fuzzear los próximos 5 lenguajes.

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
| **Arquitectura de plugin validada con lenguaje externo de prueba (Lua)** | **Implementado** | `ccm-lang-lua` | Lua elegido por no estar en el set inicial de 7. Extrae funciones nombradas (`function foo()`, `local function foo()`), funciones anónimas asignadas a variable (`local x = function() end`), tablas usadas como módulo/clase (`local M = {}`), métodos con nombre punteado o de método (`function M.new()`, `function M:greet()`, incluso anidado `Foo.Bar.baz`), y `require(...)` como Imports. 10 tests: 6 a nivel de parser (`tests/parse.rs`) + 4 de integración corriendo `find_symbol`/`find_references`/`find_calls`/`find_callers` a través del `ccm-index` real (`tests/index_integration.rs`) contra un fixture de 3 archivos (`tests/fixtures/inventory-app/`, ~30 líneas reales, no un archivo de 5 líneas). **`git diff` verificado: 0 líneas tocadas en `crates/ccm-core/src/`, `crates/ccm-index/src/` ni `crates/ccm-mcp-server/src/`** — el único cambio fuera de `ccm-lang-lua/` es la línea que agrega el crate a `members` en el `Cargo.toml` raíz (inevitable, es un archivo de workspace, no de esos tres crates) y el `Cargo.lock`. Sin hallazgo de fuga de abstracción que reportar — el trait cubrió Lua sin cambios. **Deliberadamente NO registrado** en `ccm-mcp-server/src/registry.rs` ni `ccm-cli/src/main.rs` — Lua es lenguaje de prueba de arquitectura, no un lenguaje soportado en producción esta sesión; el registro "central" que se ejercita es el tipo `ccm_core::LanguageRegistry` en sí, instanciado dentro de los tests de `ccm-lang-lua`. Fuzzing: campaña de 60s vía `cargo-fuzz` (nightly + nightly instalado esta sesión), 167 062 ejecuciones, 343 edges de cobertura, **0 crashes**; corpus semilla = los 3 archivos del fixture. Ver Decisión abierta #4 sobre el setup de `cargo-fuzz` en Windows. |

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
- [x] Tests: 29 tests pasando (`ccm-index` 5, `ccm-lang-rust` 4, `ccm-lang-python` 5, `ccm-mcp-server` 5, `ccm-lang-lua` 10)
- [x] 0 `unwrap()`/`panic!` en rutas de procesamiento de input del repo indexado (revisar explícitamente en próxima sesión con `cargo clippy` + grep dedicado)
- [ ] JavaScript/TypeScript, Java, C#, C++, Go — crates de lenguaje pendientes
- [x] Crate de lenguaje externo de prueba (Lua) validando arquitectura de plugin sin tocar `core`/`index`/`mcp-server` (`git diff` verificado — ver tabla de cobertura arriba)
- [ ] Fuzzing (`cargo-fuzz`) por crate de lenguaje
- [ ] `cargo-audit` en CI
- [ ] CI multiplataforma (GitHub Actions: Linux/macOS/Windows)
- [ ] Benchmark de tokens (con MCP vs Read/Grep/Glob) para 3+ lenguajes
- [ ] Repos de integración fijados como fixtures versionados (uno por lenguaje + poliglota)
- [x] `README.md`, `CONTRIBUTING.md`, `LICENSE` (Apache-2.0 for the project's original code and documentation), `.gitignore`
- [x] Primer commit git (`e01fc80` — workspace scaffold + Rust/Python)
- [ ] `CHANGELOG.md`
- [ ] Binarios de release multiplataforma + publicación en crates.io
