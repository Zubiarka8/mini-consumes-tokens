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

**Resueltas:**

- [x] Decisión: fuzzing excluido de CI en Windows por problema de ASan en PATH — resuelto: fuzzing corre solo en Linux/macOS, build+test normal sí corre en Windows sin excepción. Detalle técnico completo (por qué falla tanto con el sanitizer `address` por defecto como con `--sanitizer none`) documentado como comentario largo en `.github/workflows/ci.yml` junto al job `fuzz-smoke`, no solo aquí — la razón real vive en el archivo que la aplica. Revisar en el futuro solo si aparece una alternativa de fuzzing nativa de Windows más simple que configurar el `PATH` de ASan manualmente.

## Cobertura de lenguajes

| Lenguaje | Estado | Crate | Notas |
|---|---|---|---|
| Rust | **Implementado** | `ccm-lang-rust` | funciones, structs, enums, traits, impls (métodos con parent = tipo), type alias, const/static, use imports, calls. 4 tests. |
| Python | **Implementado** | `ccm-lang-python` | funciones, clases (con bases → Extends), métodos, decoradores (capturados como References), import/from-import, calls. 5 tests. |
| JavaScript/TypeScript | Sin empezar | `ccm-lang-js-ts` (no existe aún) | — |
| Java | **Implementado** | `ccm-lang-java` | clases, interfaces, enums (constantes como `Field`), métodos (**sobrecarga verificada**: dos `add(int,int)`/`add(double,double)` quedan como 2 `SymbolRecord` distintos, cada uno con su propia línea — no se colapsan), campos, clases anidadas (parent correcto, no se confunden con la clase contenedora), `extends`/`implements`, imports estándar y estáticos, llamadas a método con receptor (se usa solo el nombre del método, sin resolución de tipo — igual que Rust/Python/Lua). 12 tests: 6 parser (`tests/parse.rs`) + 4 integración vía `ccm-index` real (`tests/index_integration.rs`, fixture `billing-app/` de 3 archivos) + 2 nuevos en `ccm-index/tests/exclude.rs` (ver abajo). Registrado en producción: `ccm-mcp-server/src/registry.rs` y `ccm-cli/src/main.rs` (2 líneas cada uno, en `build_registry` — el punto de integración documentado en `CONTRIBUTING.md` desde la sesión 1, no una fuga de arquitectura). `git diff` verificado: **0 líneas en `core/src/` e `index/src/`**; `mcp-server/src/` y `cli/src/` solo tienen esas 2 líneas de registro cada uno (ningún otro archivo de esos crates tocado). Benchmark de tokens corrido — ver `benchmarks/token-benchmark.md` (90–92% de reducción en los 3 queries canónicos). Fuzzing: ver fila de cobertura de fuzzing más abajo. |
| C# | **Implementado** | `ccm-lang-csharp` | clases, interfaces, structs, namespaces, métodos (**sobrecarga verificada**, igual que Java), propiedades (**`get`/`set` quedan como un solo símbolo `Field`, no dos métodos separados** — verificado con propiedad automática y con accessors custom), campos, herencia/interfaces (heurística: primer identificador de `base_list` = Extends, el resto = Implements — C# no distingue esto sintácticamente y esta gramática no expone resolución de tipos; limitación aceptada, misma clase que la decisión de LSP diferido), `using` (incluyendo `using static`), llamadas a método. 12 tests: 6 parser + 4 integración vía `ccm-index` real (fixture `billing-app/` de 3 archivos) + 2 en `ccm-index/tests/exclude.rs`. Registrado en producción igual que Java (2 líneas en cada `build_registry`). `git diff`: mismo resultado limpio que Java. Patrones de exclusión de secretos (`appsettings.*.json`, `*.pfx`, `*.snk`) — **ya existían desde la sesión 1**, no fue necesario agregarlos; se agregó cobertura de test que no existía (`ccm-index/tests/exclude.rs`, toca `ccm-index/tests/`, no `ccm-index/src/`). Benchmark de tokens corrido — ver `benchmarks/token-benchmark.md` (91–92% de reducción). |
| C++ | Sin empezar | `ccm-lang-cpp` (no existe aún) | detectado como pendiente (`.cpp/.cc/.cxx/.h/.hpp/.hh`). |
| Go | Sin empezar | `ccm-lang-go` (no existe aún) | detectado como pendiente (`.go`). |
| **Arquitectura de plugin validada con lenguaje externo de prueba (Lua)** | **Implementado** | `ccm-lang-lua` | Lua elegido por no estar en el set inicial de 7. Extrae funciones nombradas (`function foo()`, `local function foo()`), funciones anónimas asignadas a variable (`local x = function() end`), tablas usadas como módulo/clase (`local M = {}`), métodos con nombre punteado o de método (`function M.new()`, `function M:greet()`, incluso anidado `Foo.Bar.baz`), y `require(...)` como Imports. 10 tests: 6 a nivel de parser (`tests/parse.rs`) + 4 de integración corriendo `find_symbol`/`find_references`/`find_calls`/`find_callers` a través del `ccm-index` real (`tests/index_integration.rs`) contra un fixture de 3 archivos (`tests/fixtures/inventory-app/`, ~30 líneas reales, no un archivo de 5 líneas). **`git diff` verificado: 0 líneas tocadas en `crates/ccm-core/src/`, `crates/ccm-index/src/` ni `crates/ccm-mcp-server/src/`** — el único cambio fuera de `ccm-lang-lua/` es la línea que agrega el crate a `members` en el `Cargo.toml` raíz (inevitable, es un archivo de workspace, no de esos tres crates) y el `Cargo.lock`. Sin hallazgo de fuga de abstracción que reportar — el trait cubrió Lua sin cambios. **Deliberadamente NO registrado** en `ccm-mcp-server/src/registry.rs` ni `ccm-cli/src/main.rs` — Lua es lenguaje de prueba de arquitectura, no un lenguaje soportado en producción esta sesión; el registro "central" que se ejercita es el tipo `ccm_core::LanguageRegistry` en sí, instanciado dentro de los tests de `ccm-lang-lua`. Fuzzing: campaña de 60s vía `cargo-fuzz` (nightly + nightly instalado esta sesión), 167 062 ejecuciones, 343 edges de cobertura, **0 crashes**; corpus semilla = los 3 archivos del fixture. Ver Decisión abierta #4 sobre el setup de `cargo-fuzz` en Windows. |

Repo poliglota de prueba: validado manualmente (Rust + Python en el mismo árbol, `ccm-cli init`/`status` reportan cobertura separada por lenguaje) — falta fijarlo como fixture de test de integración versionado. Explícitamente diferido hasta tener 4-5 lenguajes reales para que sea representativo (no solo Rust+Python+Java+C#).

Nota menor sin resolver (no bloqueante): `ccm-index/src/indexer.rs::KNOWN_PENDING_LANGUAGES` todavía lista las extensiones `.java`/`.cs` como "lenguaje pendiente" para el reporte de `get_indexing_status` — ahora son entradas muertas (nunca se alcanzan, porque `registry.for_extension` ya resuelve esas extensiones a un parser real antes de llegar a ese chequeo), no un bug funcional, pero quedaron desactualizadas. No se tocó esta sesión para mantener `index/src/` en 0 cambios; limpiar en la próxima sesión que toque `ccm-index`.

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
- [x] Tests: 52 tests pasando (`ccm-index` 8 [5 + 3 nuevos de `exclude.rs`], `ccm-lang-rust` 4, `ccm-lang-python` 5, `ccm-mcp-server` 5, `ccm-lang-lua` 10, `ccm-lang-java` 10, `ccm-lang-csharp` 10)
- [x] Verificado (esta sesión, ya no es solo afirmación): 0 `unwrap()`/`panic!` en rutas de procesamiento de input **no confiable** del repo indexado. Corrida real de `cargo clippy --workspace --all-targets --all-features -- -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic`: **100 hallazgos totales** — **8 en `src/` (código de producción)**, todos ya revisados y son `expect()` seguros por construcción sobre valores garantizados en tiempo de compilación o por invariante ya chequeado (setup de gramática tree-sitter estática, patrones glob hardcodeados en `ccm-index/src/exclude.rs`, y un `.expect("checked above")` en `ccm-index/src/indexer.rs:167` sobre un `Option` ya validado un par de líneas antes) — ninguno sobre datos de un repo de terceros; **92 en `tests/`/`examples/`** (esperado y aceptable, `unwrap()` en tests es una aserción de fallo-rápido, no código de producción). Ver fila nueva en "Otros pendientes" — esto es el insumo real para la sesión de housekeeping, no corregido esta sesión salvo que sea trivial (ninguno lo era).
- [ ] JavaScript/TypeScript, C++, Go — crates de lenguaje pendientes (Java y C# implementados esta sesión, ver tabla de cobertura arriba)
- [x] Crate de lenguaje externo de prueba (Lua) validando arquitectura de plugin sin tocar `core`/`index`/`mcp-server` (`git diff` verificado — ver tabla de cobertura arriba)
- [x] Fuzzing (`cargo-fuzz`) para los 5 crates de lenguaje existentes: `ccm-lang-lua` (sesión anterior, 167 062 ejecuciones/0 crashes), `ccm-lang-rust` (5 921 ejec./0 crashes), `ccm-lang-python` (49 352 ejec./0 crashes), `ccm-lang-java` (52 367 ejec./0 crashes), `ccm-lang-csharp` (24 583 ejec./0 crashes) — campañas cortas (20-60s) de verificación, no hallazgo buscado. Pendiente solo para lenguajes que aún no tienen crate (JS/TS, C++, Go) — se configura junto con cada uno.
- [x] `cargo-audit` en CI — job separado (`.github/workflows/ci.yml::cargo-audit`), solo Linux. Dos pasadas: reporte completo de toda severidad (informational, no bloquea — pass 1, `cargo audit || true`) y una segunda pasada con `severity_threshold = "high"` en un `.cargo/audit.toml` generado en el propio job, cuyo exit code es el que sí bloquea (advisories sin CVSS explícito no quedan filtrados por el umbral y siguen bloqueando — fail-safe deliberado, documentado como comentario en el workflow). Corrida local de referencia (2026-09-13, `cargo audit --json`): **0 vulnerabilidades** sobre 148 dependencias, base de datos de 1243 advisories.
- [x] CI multiplataforma (GitHub Actions: Linux/macOS/Windows) — `.github/workflows/ci.yml::build-test`, matriz de 3 SO, toolchain fijado a `1.96.0` (no `stable` flotante), cache vía `Swatinem/rust-cache@v2` por SO. Corre `cargo build --workspace --all-targets`, `cargo test --workspace` (incluye los tests de integración con fixtures reales de los 5 lenguajes en los 3 SO — primera vez que se confirma tree-sitter + SQLite en Windows vía CI, no solo en este equipo de desarrollo) y `cargo clippy --workspace --all-targets --all-features -- -D warnings` (bloquea regresiones futuras; 0 warnings hoy, ver fila de arriba y "Otros pendientes"). Job adicional `fuzz-smoke`: matriz Linux/macOS × 5 crates de lenguaje, campañas cortas (30s) confirmando que cada harness corre sin crash — **deliberadamente ausente en Windows**, razón documentada como comentario largo en el propio workflow (no solo en este checklist). **Verificado en vivo**: repo creado en GitHub (`Zubiarka8/codebase-context-mcp`, privado) y el workflow corrió realmente — ver enlace/resultado abajo.
- [x] Benchmark de tokens (con MCP vs Read/Grep/Glob) corrido para Java y C# — ver `benchmarks/token-benchmark.md` (90-92% de reducción de caracteres en los 3 queries canónicos). Criterio general de "3+ lenguajes" **sigue pendiente** hasta correr el mismo harness (ya language-agnóstico, `crates/ccm-cli/examples/token_benchmark.rs`) sobre un tercer lenguaje (p. ej. Rust o Python, implementados pero aún no benchmarkeados).
- [ ] Repos de integración fijados como fixtures versionados (uno por lenguaje + poliglota)
- [x] `README.md`, `CONTRIBUTING.md`, `LICENSE` (Apache-2.0 for the project's original code and documentation), `.gitignore`
- [x] Primer commit git (`e01fc80` — workspace scaffold + Rust/Python)
- [ ] `CHANGELOG.md`
- [ ] Binarios de release multiplataforma + publicación en crates.io

## Otros pendientes

- [ ] **Warnings de clippy detectados hoy en el código existente**: **0** con el set de lints por defecto (`cargo clippy --workspace --all-targets --all-features -- -D warnings`, exactamente el comando que corre en CI — corrida real el 2026-09-13, no estimada; por eso `-D warnings` se pudo agregar a CI esta misma sesión sin romper nada). Con lints de restricción no incluidos en el set por defecto (`-W clippy::unwrap_used -W clippy::expect_used -W clippy::panic`, corridos manualmente esta sesión, no forman parte del job de CI): **100 hallazgos** (8 en `src/`, ya revisados — ver fila correspondiente en "Entregable general"; 92 en `tests/`/`examples/`, no de producción). Insumo real para la sesión de housekeeping pendiente — no corregido esta sesión salvo trivial.
- [ ] `ccm-index/src/indexer.rs::KNOWN_PENDING_LANGUAGES` tiene entradas muertas para `.java`/`.cs` desde que esos lenguajes se implementaron (nunca se alcanzan en el código, `registry.for_extension` ya resuelve esas extensiones antes). No funcional, solo desactualizado — limpiar la próxima vez que se toque `ccm-index`.
- [ ] La primera línea de "Entregable general" (lista de crates del workspace) solo menciona los 6 crates originales de la sesión 1 — falta agregar `ccm-lang-lua`, `ccm-lang-java`, `ccm-lang-csharp`. Cosmético, no bloqueante.
