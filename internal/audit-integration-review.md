# Revisión de la integración: primera tanda de la auditoría

- **Rama:** `codex/audit-integration-review` (local, sin push ni PR).
- **Base auditada:** `1880048dd8274b0b3a7f82b07bedd3219bde5ca1`. La PR #90 sigue siendo su dependencia.
- **Integración recibida:** `a953dda70c0c0ff04d5434db5c450adcb78004e7`.
- **Corrección de esta revisión:** `3a049de`. El commit que añade este informe es el HEAD final de la rama y la base de la siguiente tanda.

## Commits revisados

| Entrega | Commit | Merge en la rama |
|---|---|---|
| Exclusiones (F03): recarga de `.mctignore`/`.gitignore` sin reiniciar | `52aa751` | `178104c` |
| Lua (F04): registro en la CLI y el servidor MCP | `39dd255` | `696b066` |
| Código muerto en Rust: usos como valor y atributos de test | `331218e` | `a953dda` |

Las tres ramas originales no se han tocado. El diff respecto a la base abarca 28 archivos, todos dentro del alcance de las tres entregas. No hay secretos, `.env`, `.mcp.json`, logs ni configuración personal. No cambian el esquema SQLite, el trait `LanguageParser` ni las firmas de herramientas MCP. Sí cambia una API pública interna de `mct-index`: `ExcludeSet::clone` ahora comparte las reglas, y se añaden `for_project`, `reload` e `is_rules_file`. Esa API no está en la lista que AGENTS.md obliga a pasar antes por un issue.

## Verificación real

| Comprobación | Resultado |
|---|---|
| `scripts/unix/check.sh` sobre `a953dda`, antes de corregir | test ok, **784 passed, 0 failed, 17 ignored**; clippy CI sin warnings; eval sin regresiones (accuracy 1.000) |
| `cargo fmt --all --check` + `scripts/unix/check.sh` sobre `3a049de` | fmt ok; test ok, **785 passed, 0 failed, 17 ignored**; clippy sin warnings; eval sin regresiones |
| Tests nuevos de Lua con los registros de la base (se quitó la línea `LuaParser` de ambos `build_registry` y luego se restauró) | Fallan `a_lua_file_is_probed_indexed_and_queryable_through_the_cli_registry` y `a_lua_file_is_indexed_with_its_symbols_and_calls` |
| Tests nuevos de Rust con el parser de la base (`git show 1880048:crates/mct-lang-rust/src/lib.rs`, luego restaurado) | `dead_code.rs`: fallan 3 de 4, los tres casos que la entrega corrige |
| Tests de exclusiones con `reload()` forzado a `false` (simula el comportamiento de la base, luego restaurado) | `exclude_reload.rs`: fallan 4 de 5. `background_watcher.rs`: fallan el test del watcher real y el de `changed_paths` |
| `scripts/unix/corpus-report.sh rust` sin `--bless` | 12 passed. Totales 239 / 817 (218), iguales a la fila de `internal/corpus-progress.md`. `references 42` coincide con las 42 filas añadidas al snapshot |

Los tests del watcher de macOS pasaron en este entorno sin problemas de permisos. Solo se usó `SDKROOT=…MacOSX26.5.sdk` en el entorno de cada comando. No se instalaron binarios globales. Las fuentes se exploraron con el MCP de este worktree, cuya ruta se confirmó con `list_symbols` sobre `crates/mct-index/tests/exclude_reload.rs`, un archivo que solo existe en esta rama. Las reproducciones de comportamiento se ejecutaron con los tests compilados localmente.

## Hallazgos resueltos

**R1. P3. Rust: la abreviatura de un patrón de struct no contaba como variable local.**
- **Dónde:** `crates/mct-lang-rust/src/lib.rs`, en `collect_identifiers`.
- **Desencadenante:** `let Config { root, .. } = c; root`, y la misma forma en parámetros, brazos de `match` (también `ref root`) y closures, en un archivo que además define `fn root`.
- **Efecto:** solo se reconocían nodos `identifier`, no `shorthand_field_identifier`. Cada uso posterior de la variable `root` se registraba como una referencia a la función `root`, y `find_dead_code` dejaba de mostrar una función que de verdad no se usa. Contradice la promesa de la entrega de que las variables de `let`, `match`, parámetros y closures tapan a la función del mismo nombre.
- **Corrección:** `3a049de` acepta también `shorthand_field_identifier`. La regresión es `a_struct_pattern_shorthand_binding_shadows_a_same_named_function`: antes de la corrección producía `["root", "root", "root", "root"]` y ahora ninguno. El snapshot del corpus no cambia.

**R2. P3. La documentación prometía más de lo que da.**
- **Dónde:** `crates/mct-index/src/dead_code.rs`, en `looks_like_test_name`.
- **Problema:** el texto presentaba la referencia de `#[test]` como si fuera una garantía.
- **Corrección:** ahora aclara que se resuelve por nombre en todo el proyecto, así que también oculta un símbolo del mismo nombre en otro sitio (un `fn parse` sin usar junto a un `#[test] fn parse`). Es el límite general de F02, no una comprobación de seguridad. Mismo commit.

## Hallazgos pendientes (ninguno bloquea esta integración)

**E1. P3. Exclusiones: `ExcludeSet::reload` lee, compara y sustituye en pasos separados.**
- **Dónde:** `crates/mct-index/src/exclude.rs`, `reload`.
- **Desencadenante:** el hilo del watcher (sin el lock del índice) y un reindex en curso (con el lock) recargan las reglas a la vez mientras se edita `.mctignore`.
- **Efecto:** gana la última escritura, así que durante un rato pueden quedar reglas antiguas. Un reindex en curso también puede recorrer el árbol con reglas mezcladas si el watcher las cambia a mitad del recorrido.
- **¿Puede quedar mal de forma permanente? No.** Cada recarga del watcher que detecta un cambio pone en cola un reindex completo. Ese reindex espera al lock y vuelve a leer los archivos al empezar. Una escritura antigua del hilo de reindex ocurre dentro de su lock, es decir, antes de ese reindex en cola. Si el watcher detecta que las reglas no cambiaron, `reindex_paths` las vuelve a comprobar con el lock tomado y hace un recorrido completo si ya no coinciden.
- **Tarea posterior:** mantener el lock de escritura durante toda la recarga. No se hizo aquí porque no hay una regresión determinista posible sin puntos de inyección.

**E2. P3. Exclusiones: si leer `.mctignore` falla, las reglas del proyecto se vacían.**
- **Dónde:** `read_ignore_file`.
- **Desencadenante:** el archivo no se puede leer (permisos) o se lee a mitad de escritura (un editor que lo trunca y luego lo reescribe).
- **Efecto:** las reglas del proyecto pasan a estar vacías y un reindex completo vuelve a indexar lo que el usuario había excluido. Las exclusiones de secretos incorporadas siguen activas. Con una escritura a medias se corrige solo en el siguiente evento. Si el archivo sigue sin poder leerse, el problema persiste. La semántica ya existía en el arranque; ahora puede darse con el servidor en marcha.
- **Tarea posterior:** conservar las reglas anteriores cuando la lectura falla por algo distinto de "el archivo no existe".

**RS1. Limitación conocida (F02, fuera de alcance): las relaciones se resuelven por nombre en todo el proyecto.**
- **Efecto:** las nuevas referencias por uso como valor y por `#[test]`/`#[bench]` ocultan en `find_dead_code` cualquier símbolo del mismo nombre en otro archivo o lenguaje. Además, `find_references`, `impact_analysis` y `build_context_pack` muestran referencias sintéticas del módulo `tests` hacia sus funciones de test. Esto es intencionado y ahora está documentado (R2).
- **Evidencia de que no hay regresión:** eval sin regresiones y los 784 tests pasan.
- Lo corresponde resolver a F02, no a esta integración.

**RS2. P3. Rust: variables ligadas fuera de cualquier `fn`.**
- **Desencadenante:** parámetros de closures en el inicializador de un `const`/`static`, por ejemplo `const F: fn(u32) -> u32 = |helper| helper;`, o identificadores dentro de `macro_rules!`.
- **Efecto:** esas variables no cuentan como locales, así que se registran como referencias a una `fn` del mismo archivo con ese nombre. El sombreado también ignora el orden y el bloque: basta una variable con ese nombre en cualquier punto de la función. Es una sobreaproximación documentada que solo puede ocultar referencias, nunca inventarlas.
- Poco frecuente. Tarea posterior.

**L1. Informativo. Lua y publicación.**
- `mct-cli` añade `mct-mcp-server` como dev-dependency solo por ruta. `cargo publish` la elimina, igual que ya hace con `mct-corpus`.
- `mct-lang-lua` es publicable (no tiene `publish = false`) y tiene versión en `[workspace.dependencies]`.
- El test `every_language_crate_in_the_workspace_is_registered_in_production` lee `../` desde `CARGO_MANIFEST_DIR`, así que solo funciona dentro del workspace. Nunca se ejecuta desde un `.crate` empaquetado.
- La fixture `omni-app/luatools/build.lua` se modificó (`return tostring(target)`) para tener una llamada que verificar. Los conteos actualizados (28→29 archivos, 101→104 símbolos, cobertura `lua` 1 archivo / 3 símbolos) son coherentes con los resultados de los tests.

## Decisión

**LISTA PARA SIGUIENTE TANDA.**

Condiciones:
1. La suite conjunta, clippy de CI, formato y el eval pasan sobre el HEAD de esta rama.
2. Cada entrega tiene tests que fallan con el comportamiento anterior.
3. El único bug claro dentro del alcance (R1) está corregido con su regresión.
4. Los pendientes E1, E2 y RS2 son P3 y convergen solos o son poco frecuentes. RS1 pertenece a F02.

Nada requiere un issue previo por cambio de esquema, de `LanguageParser` o de firmas MCP. La publicación sigue pendiente de la coordinación y de la PR #90.
