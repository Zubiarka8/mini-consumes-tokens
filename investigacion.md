# Investigación — optimización de tokens y precisión del MCP

> **Estado**: documento de análisis, no de implementación. No se ha modificado
> código de producto para generarlo.
> **Fecha**: 2026-09-18 · **Rama**: `refactor/mct-rename` · **Commit base**: `abcb478`
>
> Relación con otros documentos del repo:
> - `ISSUES_PENDING.md` — 3 bugs de producto ya pinchados por tests `#[ignore]`d.
>   Aquí se **referencian**, no se reabren.
> - `ROADMAP.md` — congelado en `9ca9ab5` (pre-0.2.0). Su candidato #10
>   ("rendimiento a escala") planteaba 3 *sospechas* sin medir. Este documento
>   **mide dos de ellas y corrige una** (ver §C1).

---

## 1. Método

Todas las cifras son medidas, no estimadas. Se obtuvieron hablando JSON-RPC
directamente contra el binario real del servidor:

```sh
call() {
  printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"p","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "$1" | ./target/debug/mct-mcp-server.exe --root . 2>/dev/null | tail -n 1
}
call '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | wc -c
```

Conversión usada en todo el documento: **~4 bytes ≈ 1 token** (misma heurística
que `benchmarks/token-benchmark.md`). No es un tokenizer real; sirve para ver
el orden de magnitud, que es lo que importa aquí.

Estado del índice en el momento de medir: **663 archivos, 5.544 símbolos**.

---

## 2. Resumen ejecutivo

| # | Hallazgo | Medida | Impacto | Esfuerzo |
|---|----------|--------|---------|----------|
| A4 | `.claude/worktrees/` se indexa entero: 2 copias completas del repo | 3.652 de 5.544 símbolos (65,9 %) son ruido duplicado | **Alto** | Pequeño |
| A2 | `get_project_overview` sin argumentos | **167.864 B ≈ 42.000 tokens** en una sola llamada | **Alto** | Pequeño |
| A3 | `get_indexing_status` | 21.113 B ≈ 5.278 tokens, de los cuales 20.425 B (97 %) son el volcado de dependencias | **Alto** | Pequeño |
| B1 | Resolución de relaciones solo por nombre global | `find_callers("main")` devuelve PHP y Python, cero Rust | **Alto** | Medio |
| C1 | `WalkDir` sin poda de directorios | desciende a `target/` (45.220 archivos) y `.git/` (1.725) en cada reindex | **Alto** | Pequeño |
| A1 | Coste fijo de `tools/list` en cada sesión | 12.873 B ≈ 3.218 tokens antes de la primera query | Medio | Medio |
| B5 | Heurística archivo-vs-directorio rota con puntos | `list_symbols(".claude")` → "No symbols found" con 3.652 símbolos dentro | Medio | Trivial |
| B3 | Tabla FTS5 mantenida por triggers y **nunca consultada** | coste de escritura en cada reindex, cero beneficio de query | Medio | Medio |
| B2 | `to_symbol_id` se escribe (1 query SQL por relación) y nunca se lee | N queries por archivo sin ningún consumidor | Medio | Pequeño |
| C9 | El watcher observa el root completo, incluido `target/` | un `cargo build` dispara miles de eventos | Medio | Pequeño |
| D1 | El vault Obsidian es invisible para `get_project_overview` | `overview("docs")` = 18 módulos, **100 % "(no top-level symbols)"** | Medio | Medio |

---

## 3. Bloque A — Consumo de tokens

### A1. Coste fijo de arranque: 3.218 tokens antes de la primera pregunta

`tools/list` devuelve **12.873 bytes**. Desglose real por tool:

| Tool | `description` (B) | `inputSchema` (B) | Total (B) |
|---|---|---|---|
| `list_symbols` | 708 | 1.149 | 1.949 |
| `impact_analysis` | 593 | 1.024 | 1.710 |
| `find_callers` | 468 | 1.098 | 1.658 |
| `find_references` | 381 | 1.093 | 1.582 |
| `get_project_overview` | 609 | 793 | 1.515 |
| `find_calls` | 233 | 1.095 | 1.418 |
| `get_file_skeleton` | 840 | 418 | 1.383 |
| `find_symbol` | 366 | 481 | 923 |
| `reindex` | 543 | 284 | 897 |
| `get_indexing_status` | 434 | 36 | 552 |

Dos observaciones concretas:

1. **Esquemas casi idénticos triplicados**: `find_calls`, `find_callers` y
   `find_references` comparten los mismos cuatro parámetros
   (`function`/`symbol`, `limit`, `offset`, `depth`) con las mismas
   descripciones largas de `depth` y `offset` → **3.286 B (~820 tokens)** de
   esquema prácticamente duplicado. Acortar las descripciones de `depth` y
   `offset` a una línea y remitir a la documentación de la tool recorta
   ~500-600 tokens sin perder información accionable.
2. Las descripciones son largas **a propósito** (evitan que el agente elija la
   tool equivocada) y eso vale la pena. El recorte debe atacar los *parámetros
   repetidos*, no las descripciones de las tools.

**Recomendación**: objetivo `tools/list` ≤ 9.000 B (~2.250 tokens). Añadir un
test que falle si el payload supera ese presupuesto — igual que
`docs/03-performance/limits-spec.md` fija límites de producto.

### A2. `get_project_overview` cuesta ~42.000 tokens por defecto

Medido en este repo:

| Llamada | Bytes | ~Tokens |
|---|---|---|
| `get_project_overview({})` | 167.864 | ~42.000 |
| `get_project_overview({include_relations:false})` | 45.760 | ~11.440 |
| `get_project_overview({path:"crates"})` | 164.294 | ~41.000 |

La descripción de la tool dice literalmente *"TOKEN-SAVING project digest"* y
*"the cheapest way to get oriented"*. Una llamada sin argumentos consume más
contexto que leer 40 archivos completos.

Causas, todas en `crates/mct-mcp-server/src/server.rs:533-600` y
`crates/mct-mcp-server/src/format.rs:overview`:

- `include_relations` **por defecto es `true`** (`unwrap_or(true)`), y cada
  símbolo emite sus callers como sub-líneas. Es el 73 % del payload.
- `max_symbols_per_module` (por defecto 8) acota los símbolos **por módulo**,
  pero **nada acota el número de módulos** (225 aquí) ni el total de bytes.
- Los módulos sin símbolos emiten igualmente una línea `(no top-level symbols)`
  (ver D1).

**Recomendación**:
1. `include_relations` por defecto a `false`. Es el cambio de una línea con
   mayor ratio impacto/esfuerzo de todo el documento.
2. Añadir un **presupuesto de bytes** (p. ej. 24.000 B) que trunque por
   módulos y lo declare en la cabecera: `(120 of 225 modules shown — pass
   `path` to narrow)`. `limit` cuenta filas, no bytes; un presupuesto de bytes
   es lo que realmente protege el contexto.
3. Ordenar los módulos por densidad de símbolos antes de truncar, para que lo
   que sobrevive al presupuesto sea lo informativo.

### A3. `get_indexing_status`: 97 % del payload es el volcado de dependencias

21.113 B ≈ 5.278 tokens, de los cuales **20.425 B** son la sección
`Dependencies detected`: cada `Cargo.toml` del workspace con todas sus
dependencias, una por línea. Son 30+ manifiestos que repiten los mismos 20
`mct-lang-*`.

Es una tool de diagnóstico ("¿está fresco el índice?") que cuesta más que la
mayoría de las queries reales.

**Recomendación**: resumir por defecto
(`Dependencies: 34 manifests, 312 declared (18 unique external)`) y exponer el
detalle tras un parámetro explícito `verbose: true` o, mejor, sacarlo a
`mct-cli --root . status --deps`, que es donde un humano lo consulta.

### A4. El índice contiene dos copias completas del repo

`.claude/worktrees/` aloja dos worktrees de agente
(`agent-a91ea5ed15bdbe91b`, `agent-af80f4d6cd52dd3bb`), cada uno con el árbol
entero de `crates/` y `docs/`.

Medido:

- `list_symbols(".claude/worktrees")` → **3.652 símbolos**, el **65,9 %** de
  los 5.544 del índice.
- `find_references("reindex")` → 234 hits, de los cuales **156 (67 %)**
  apuntan a `.claude/worktrees/`.
- `find_symbol("reindex")` → **12 definiciones** donde deberían ser ~4.

Esto no es solo ruido de tokens: es **ruido de precisión**. Cada resultado
llega con dos tercios de entradas que el usuario no puede editar y que el
agente puede confundir con el código real.

`.gitignore` ya excluye `.claude/` (línea 19) y `.mct-index/` (línea 4), pero
**el indexador no lee `.gitignore` en absoluto**, pese a que el proyecto ya
depende de `git2`.

**Recomendación (la más rentable del documento)**:
1. Inmediato: añadir `**/.claude{,/**}` y `**/.claude-index{,/**}` a
   `DEFAULT_EXCLUDE_PATTERNS` (`crates/mct-index/src/exclude.rs:11-37`).
2. Estructural: honrar `.gitignore` con `git2` (ya en el árbol de
   dependencias), con un flag `--no-gitignore` para desactivarlo. Un archivo
   ignorado por git es, por definición, no-fuente-del-proyecto.

### A5. `limit` cuenta filas, nunca bytes

`paginate()` (`format.rs:29-34`) corta por número de elementos. Ninguna tool
tiene noción de cuánto contexto está devolviendo. `list_symbols("crates",
limit: 500)` = 50.101 B ≈ 12.500 tokens sin ningún aviso.

**Recomendación**: un presupuesto de bytes compartido en `format.rs`, aplicado
después de `paginate()`, que trunque y lo declare en la nota de truncado ya
existente (`truncation_note`). Un único punto de cambio para todas las tools.

### A6. Líneas de ruido puro

- `(no top-level symbols)` por cada módulo vacío en el overview.
- El padding de alineación de `list_symbols` (`{:<name_width$}`) emite espacios
  que el modelo paga como tokens sin aportar información; dos espacios fijos
  bastan.

---

## 4. Bloque B — Precisión y calidad del MCP

### B1. Las relaciones se resuelven por nombre global — falsos positivos entre lenguajes

Ejemplo reproducible en este mismo repo:

```
find_callers("main") →
  crates/mct-lang-php/tests/fixtures/billing-app/run.php:11 [php]    run  --calls--> main
  crates/mct-lang-python/tests/fixtures/billing-app/main.py:10 [python] main --calls--> main
```

Cero resultados de Rust; dos resultados de fixtures de otro lenguaje. Las
queries (`crates/mct-index/src/queries.rs:140-177`) hacen `WHERE r.to_name = ?1`
sin ninguna condición de archivo, directorio o lenguaje.

Con nombres comunes (`new`, `run`, `get`, `parse`, `main`, `location`) esto
degrada de "impreciso" a "inservible", y el coste en tokens es proporcional al
número de falsos positivos.

**Recomendación** (por orden de coste creciente):
1. **Filtros de ámbito opcionales** en `find_symbol`, `find_calls`,
   `find_callers`, `find_references` e `impact_analysis`: `path` (prefijo) y
   `language`. `list_symbols` ya los tiene; el resto no. Es la mejora de
   precisión más barata que existe y **no toca el schema**, solo añade
   `AND f.relative_path LIKE ?` / `AND f.language = ?`.
2. **Desempate por proximidad** en la resolución: mismo archivo → mismo
   directorio → mismo lenguaje → global. `ROADMAP.md` lo descarta como
   candidato separado y lo absorbe en la decisión #1 (LSP); esa decisión sigue
   pendiente, así que el punto 1 de arriba no debe esperar a ella.

### B2. `to_symbol_id` se escribe una vez por relación y no lo lee nadie

`crates/mct-index/src/indexer.rs:337-343` ejecuta, **por cada relación**,
dentro de la transacción:

```sql
SELECT id FROM symbols WHERE name = ?1 LIMIT 1
```

y guarda el resultado en `relations.to_symbol_id`. Verificado con búsqueda en
todo el árbol: **ninguna query lo lee**. Las cinco queries de `queries.rs`
matchean por `to_name`. Es decir:

- coste de indexado: una query SQL adicional por relación, sin `prepare_cached`;
- beneficio de query: cero;
- y el `LIMIT 1` sin `ORDER BY` elige un símbolo **arbitrario** entre los
  homónimos, así que si algún día se leyera, sería incorrecto.

**Recomendación**: decidir una de las dos, no dejarlo a medias.
- **Usarlo**: resolver con desempate determinista (B1.2) y hacer que
  `find_references`/`find_callers` prefieran `to_symbol_id` cuando no es NULL.
  Esto convierte B1 de heurística en resolución real.
- **Quitarlo**: eliminar la columna y la query por migración. Recupera
  velocidad de indexado inmediata.

Nota: el comentario del propio código ya admite que un reindex posterior no
rellena la columna retroactivamente, así que hoy el valor almacenado depende
del **orden de indexado**. No es determinista entre máquinas.

### B3. Hay un FTS5 montado y jamás consultado

`crates/mct-index/src/schema.rs:47-62` crea `symbols_fts` (tabla virtual FTS5
sobre `symbols.name`) más **tres triggers** (`symbols_ai`, `symbols_ad`,
`symbols_au`). Como `write_parsed_file` hace `DELETE FROM symbols WHERE
file_id = ?` seguido de N `INSERT`, cada reindex de un archivo dispara el ciclo
completo de borrado+inserción en el índice FTS.

Y `find_symbol` usa `WHERE s.name = ?1` — **coincidencia exacta**. No hay
búsqueda por prefijo, difusa ni case-insensitive en ninguna tool.

Consecuencia directa en tokens: el agente que no acierta el nombre exacto cae
en el bucle `list_symbols` → leer → reintentar, que es exactamente el gasto que
el proyecto existe para evitar.

**Recomendación**: aprovechar lo que ya se está pagando. Añadir a `find_symbol`
un modo `fuzzy: true` (o un parámetro `match: "exact" | "prefix" | "fuzzy"`)
que consulte `symbols_fts`. Coste: una query nueva; infraestructura: ya está
construida y mantenida. Si se decide no usarla, hay que **borrar la tabla y los
triggers**: hoy es coste de escritura puro.

### B4. Ninguna tool de relaciones acepta ámbito

Consecuencia de B1, listada aparte porque es la acción concreta: cinco tools
sin `path`/`language`. Para un repo poliglota — el caso de uso que el README
declara como central — es el hueco de precisión más visible.

### B5. Cualquier directorio con un punto se trata como archivo

`crates/mct-index/src/queries.rs:89` y `server.rs:329` comparten esta heurística:

```rust
let is_file = path.rsplit('/').next().unwrap_or(path).contains('.');
```

Por tanto `list_symbols(".claude")` genera `WHERE f.relative_path = '.claude'`
y devuelve `No symbols found under '.claude'` — **con 3.652 símbolos dentro**.
Lo mismo ocurre con `.github`, `.cargo`, `v1.2/`, `my.module/`.

**Recomendación**: decidir por el sistema de archivos, no por la cadena —
`index.root().join(path).is_dir()`, con la heurística actual solo como
fallback para rutas ya borradas. Es un cambio de dos líneas y elimina una clase
entera de "no encontrado" silencioso.

### B6. `looks_like_test_name` es redundante y demasiado amplia

```rust
lower.starts_with("test_") || lower.starts_with("test")
```

La primera condición está contenida en la segunda. Y `starts_with("test")`
captura `testimonial`, `tester`, `testament`. Además no detecta las
convenciones reales de la mayoría de los 16 lenguajes soportados: `#[test]` de
Rust, `*_test.go`, `*Test.java`, `describe`/`it` de JS.

**Recomendación**: complementar con la ruta del archivo (`tests/`, `__tests__/`,
`*_test.*`, `*Test.*`), que es una señal mucho más fiable y ya está en la BD.

### B7. Bugs ya documentados — no reabrir aquí

`ISSUES_PENDING.md` cubre, con tests `#[ignore]`d que sirven de especificación:

1. `get_file_skeleton` / `get_project_overview` ciegos para Go, C#, Bash y
   PowerShell (4 de 16 lenguajes) por usar `parent.is_none()` como definición
   de "top-level".
2. `.lua` se descarta sin ningún diagnóstico.
3. Siete huecos del grafo de notas Obsidian.

El #1 tiene además un efecto de tokens no anotado allí: esos cuatro lenguajes
consumen líneas de módulo en el overview sin aportar ni un símbolo.

---

## 5. Bloque C — SQLite y coste de indexado

### C1. `WalkDir` no poda directorios: 47.000 entradas por reindex

`crates/mct-index/src/indexer.rs:88` usa `WalkDir::new(&root).into_iter()` sin
`filter_entry`. La exclusión se aplica **por archivo, después de descender**.
En este repo eso significa recorrer:

- `target/` → **45.220 archivos**
- `.git/` → **1.725 archivos**

Y antes de comprobar la exclusión se llama a `path.canonicalize()` (línea 97):
una syscall por entrada, ~47.000 por reindex, incluidas todas las de `target/`.

**Corrección a `ROADMAP.md` #10, sospecha 1**: la sospecha decía que cada
archivo no excluido se lee entero para hashearlo *incluso sin cambios*. Es
cierto **solo para archivos con parser registrado** — la comprobación
`registry.for_extension()` (línea 136) ocurre **antes** del `std::fs::read`
(línea 155). El coste real no es la lectura de todo el repo, es el **descenso y
`canonicalize()` sobre las 47.000 entradas de `target/` y `.git/`**.

**Recomendación**: `WalkDir::filter_entry(|e| !excluded_dir(e))` para podar el
directorio completo, y mover `canonicalize()` después del filtro de exclusión.
Corta ~98 % de las syscalls de un reindex en este repo.

### C2. Sin configuración de exclusiones: el punto de extensión no está cableado

`ExcludeSet::new(extra_patterns)` acepta patrones de usuario y su docstring
promete *"lets a project widen it via configuration"*. Pero en todo el árbol
solo se invoca `ExcludeSet::default()`:

- `crates/mct-cli/src/main.rs:85`
- `crates/mct-mcp-server/src/main.rs:44` y `:67`

**No existe ninguna ruta de configuración**. Un usuario con un monorepo no
puede excluir nada sin recompilar.

**Recomendación**: leer `.mct/config.toml` (o una sección `[mct]` en un archivo
ya existente) con `exclude = [...]`. `toml` ya es dependencia de `mct-index`.
Esto y §A4 son la misma solución vista desde dos ángulos.

### C3. N+1 en el BFS multi-hop

`crates/mct-index/src/traversal.rs:21-56` ejecuta una query por **nodo por
nivel**, y cada una pasa por `conn.prepare()` (`queries.rs:180`), no
`prepare_cached()`. Un `find_callers(depth: 3)` sobre un símbolo con fan-in 40
hace decenas de compilaciones de SQL idénticas.

El comentario de cabecera asume el coste conscientemente ("an acceptable trade
for the repo sizes this project targets"), y es razonable. Pero
`prepare_cached()` es un cambio de una palabra que elimina la recompilación sin
tocar la arquitectura.

### C4. N+1 en `get_project_overview`

`server.rs:578-600`: por cada módulo truncado se llama `find_callers` **una vez
por símbolo candidato** para el ranking por fan-in, y después **otra vez por
símbolo conservado** para las relaciones. Con 225 módulos × 8 símbolos son
>1.800 queries, cada una con su `prepare()`.

**Recomendación**: una sola query agregada
(`SELECT to_name, COUNT(*) FROM relations WHERE kind='calls' GROUP BY to_name`)
para todo el ranking de fan-in. Una query en lugar de ~900.

### C5. La paginación es en memoria, no en SQL

`find_*` traen **todas** las filas y `paginate()` corta después. En el caso
medido, `find_references("reindex")` materializa 234 `RelationHit` con sus
`String` para mostrar 50.

Es una decisión defendible (la cabecera `234 reference(s)` necesita el total
real), pero la forma correcta es `COUNT(*)` + `LIMIT/OFFSET`: dos queries
baratas en lugar de una materialización completa.

### C6. Índices de SQLite

Estado actual (`schema.rs`):

| Índice | Evaluación |
|---|---|
| `idx_symbols_name` | correcto, muy usado |
| `idx_symbols_file` | correcto |
| `idx_relations_to_name` | correcto, pero incompleto |
| `idx_relations_from` | correcto |
| `idx_relations_kind` | **de baja selectividad** — `kind` tiene 5 valores; SQLite rara vez lo elegirá y cuesta escrituras en cada insert |

`find_callers` filtra por `r.to_name = ?1 AND r.kind = 'calls'` y
`find_calls` por `caller.name = ?1 AND r.kind = 'calls'`.

**Recomendación**: sustituir `idx_relations_kind` por el compuesto
`(to_name, kind)`, que sirve a ambos predicados y elimina un índice inútil.
Medir con `EXPLAIN QUERY PLAN` antes y después; sin medición esto es una
hipótesis, no un hecho.

### C7. Higiene de la conexión SQLite

`Index::open` fija `journal_mode=WAL` y `foreign_keys=ON`. Nada más. Estado del
disco ahora mismo:

```
index.sqlite3       3.366.912 B
index.sqlite3-wal   5.038.792 B   ← el WAL es mayor que la base de datos
```

El WAL nunca se checkpointea. Falta además:

- `PRAGMA synchronous = NORMAL` — seguro bajo WAL, evita un fsync por commit.
- `PRAGMA wal_autocheckpoint` / un `wal_checkpoint(TRUNCATE)` al cerrar.
- `PRAGMA optimize` al cerrar (recalcula estadísticas para el planner).
- `PRAGMA mmap_size` para lecturas.

Y el directorio **`.claude-index/`** (1 MB de base + 4,3 MB de WAL) es un
residuo del rename `ccm` → `mct`: no está en `.gitignore`, no está en
`DEFAULT_EXCLUDE_PATTERNS` (que solo excluye `**/.mct-index`) y por tanto se
recorre en cada reindex.

### C8. El watcher observa `target/` entero

`background.rs:48` hace `debouncer.watch(&root, RecursiveMode::Recursive)` sobre
el root completo, y el filtro de `ExcludeSet` se aplica **después** de recibir
los eventos. Un `cargo build` genera miles de eventos en `target/` que el
debouncer procesa para acabar descartándolos.

Peor: cuando un evento sí es relevante, se dispara un `reindex(force=false)`
que vuelve a recorrer el árbol **completo** (§C1). Editar un archivo cuesta un
walk de 47.000 entradas.

**Recomendación**: registrar watches por subdirectorio saltándose los excluidos,
o mantener el watch recursivo pero filtrar por prefijo antes de encolar. Y, a
más largo plazo, un reindex incremental que solo toque las rutas notificadas en
vez de re-caminar el árbol.

---

## 6. Bloque D — Obsidian / Markdown

### D1. El vault es invisible para las tools de overview

Medido sobre `docs/` (18 notas):

```
get_project_overview({path:"docs"}) → 1.531 B
  docs/00-system/00-index.md:
    (no top-level symbols)
  docs/00-system/glossary.md:
    (no top-level symbols)
  ... 18 de 18 iguales
```

**El 100 % de la salida es ruido.** Dos causas que se suman:

1. `mct-lang-md` indexa los headings como `SymbolKind::Element`, que **no está
   en `OVERVIEW_KIND_ALLOWLIST`** de `server.rs`.
2. Los headings anidados llevan `parent`, y el filtro es
   `e.parent.is_none()` — la misma raíz que `ISSUES_PENDING` #1.

**Recomendación**: incluir `element` en el allowlist del overview cuando el
lenguaje es `markdown`, y aplicar la corrección de "top-level" de
`ISSUES_PENDING` #1, que arregla los dos casos a la vez.

### D2. No hay símbolo de nota — todo el grafo del vault depende del H1

Esto es `ISSUES_PENDING` #3 (gaps 3.4 a 3.7), y su propio análisis ya identifica
la solución común: **un símbolo por archivo de nota, y resolución de wikilinks
contra el nombre de archivo, no contra el texto del H1**.

Lo que aquí se añade es el ángulo de tokens: sin símbolo de nota,
`get_file_skeleton` y `get_project_overview` no pueden dar un resumen de una
nota, así que la única forma de que el agente vea el contenido de un `.md` es
**leerlo entero** — precisamente el gasto que este proyecto existe para evitar.
En un vault Obsidian de cientos de notas, el ahorro de tokens del MCP es hoy
**cero**.

### D3. Front-matter YAML no se indexa

`ISSUES_PENDING` 3.3: `tags: [daily, review]` en el front-matter no produce
ninguna relación, aunque `#standup` en línea sí. El front-matter es el modo por
defecto de etiquetar en las plantillas de Obsidian, así que un vault estándar
no tiene grafo de tags en absoluto.

### D4. Propuesta específica para Obsidian (post-ISSUES_PENDING #3)

Una vez exista el símbolo de nota, la tool que faltaría — y que sería el
equivalente de `get_file_skeleton` para el vault:

```
get_note_context(note, depth = 1)
  → front-matter (tags, aliases, propiedades)
  → outline de headings con su nivel
  → enlaces salientes (link vs embed, resueltos a ruta)
  → backlinks
  → notas huérfanas conectadas a `depth` saltos
```

Resuelve en una llamada lo que hoy requiere leer N notas completas. **No
implementar antes de cerrar `ISSUES_PENDING` #3**: sin símbolo de nota ni
resolución por ruta, la tool devolvería datos incorrectos.

---

## 7. Plan sugerido

Ordenado por ratio impacto/esfuerzo medido, no por área temática.

### Fase 1 — Ganancias inmediatas (una sesión corta, cero riesgo arquitectónico)

1. Excluir `.claude/`, `.claude-index/` → **−65,9 % de símbolos del índice**,
   −67 % de ruido en resultados de relaciones. *(§A4)*
2. `include_relations` por defecto a `false` → **−73 % en `get_project_overview`**
   (167.864 B → 45.760 B). *(§A2)*
3. Resumir las dependencias en `get_indexing_status` → **−97 % de esa tool**. *(§A3)*
4. `is_dir()` en lugar de `contains('.')` para la heurística archivo/directorio. *(§B5)*
5. `WalkDir::filter_entry` + `canonicalize()` después de excluir. *(§C1)*
6. Borrar `.claude-index/` y añadirlo a `.gitignore`. *(§C7)*

> Verificación: volver a correr las mediciones de §1 y comprobar que
> `get_project_overview({})` baja de 24.000 B y que
> `find_symbol("reindex")` devuelve 4 definiciones en vez de 12.

### Fase 2 — Precisión (una sesión, no toca el schema)

7. `path` y `language` opcionales en `find_symbol`, `find_calls`,
   `find_callers`, `find_references`, `impact_analysis`. *(§B1, §B4)*
8. Presupuesto de bytes compartido en `format.rs`, declarado en el truncado. *(§A5)*
9. `prepare_cached()` + fan-in agregado en una query para el overview. *(§C3, §C4)*
10. Heurística de test basada en la ruta del archivo. *(§B6)*

### Fase 3 — Decisiones que hay que tomar antes de codificar

11. **`to_symbol_id`: usar o borrar.** Hoy es coste sin consumidor y su valor
    depende del orden de indexado. *(§B2)*
12. **FTS5: consultar o borrar.** Hoy son tres triggers mantenidos para una
    tabla que nadie lee; consultarla daría búsqueda por prefijo/difusa casi
    gratis. *(§B3)*
13. **Configuración de exclusiones** (`.mct/config.toml`) — condiciona cómo se
    resuelve §A4 de forma estructural. *(§C2)*
14. **Honrar `.gitignore` con `git2`** — decide si §A4 necesita mantenimiento
    manual perpetuo o se resuelve solo. *(§A4, §C2)*

### Fase 4 — Obsidian (depende de `ISSUES_PENDING` #3)

15. Cerrar `ISSUES_PENDING` #3 (símbolo de nota + resolución por ruta).
16. `element` en el allowlist del overview para markdown. *(§D1)*
17. Front-matter YAML indexado. *(§D3)*
18. Solo entonces: evaluar `get_note_context`. *(§D4)*

---

## 8. Qué NO hacer

- **No añadir tools nuevas antes de la Fase 1.** Cada tool cuesta ~1.400 B de
  `tools/list` en **todas** las sesiones (§A1). `get_note_context` solo se
  justifica después de que el vault produzca datos correctos.
- **No paralelizar el indexado todavía.** `ROADMAP.md` ya lo descarta por falta
  de medición, y este documento lo confirma desde otro ángulo: el cuello de
  botella medido es el descenso a `target/` (§C1), no el parseo. Podar primero
  y volver a medir.
- **No tocar el schema para particionar por lenguaje.** No resuelve ninguno de
  los hallazgos de este documento; `ROADMAP.md` ya lo descartó por lo mismo.
- **No implementar resolución de tipos vía LSP para arreglar §B1.** Los filtros
  de ámbito (Fase 2, punto 7) capturan la mayor parte del beneficio a una
  fracción del coste, y no bloquean la decisión #1 del roadmap si más adelante
  se toma.

---

## 9. Limitaciones de esta investigación

- Las cifras de tokens son `bytes / 4`, no una tokenización real. Los órdenes
  de magnitud son fiables; los porcentajes exactos, aproximados.
- Todas las mediciones son sobre **este** repo (663 archivos, dos worktrees de
  agente). No se ha medido contra un monorepo grande — `ROADMAP.md` #10 sigue
  siendo la tarea pendiente para eso.
- §C6 (índices de SQLite) es la única sección basada en lectura de código sin
  medición. Requiere `EXPLAIN QUERY PLAN` antes de actuar.
- No se ha ejecutado la suite de tests como parte de esta investigación; no se
  ha modificado código de producto.
