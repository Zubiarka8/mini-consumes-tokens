# MCP Protocol Spec

`mct-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

## Tools

| Tool | Type | Purpose |
|---|---|---|
| `list_symbols` | discovery | List symbols under a file or directory/crate prefix |
| `find_symbol` | atomic | Exact-name symbol lookup |
| `find_references` | atomic | Every reference to a symbol |
| `find_calls` | atomic | What a function calls |
| `find_callers` | atomic | What calls a function |
| `impact_analysis` | composite | Full blast radius in one call |
| `build_context_pack` | composite | Definition + doc + capped source, and every caller/callee/dependency/test once, in one call |
| `get_file_skeleton` | discovery | Top-level shape of one file, bodies collapsed |
| `get_project_overview` | discovery | Compact hierarchical digest of a file/directory/crate/project |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`.

`get_project_overview` accepts `path` (optional, same matching semantics as `list_symbols` — omitted means the whole project root), `language`, `max_symbols_per_module` (default 8, caps top-level symbols surfaced per file), and `include_relations` (default true, whether to show each surfaced symbol's top callers). It composes `list_symbols` + `find_callers` internally — no new SQL, no new schema — ranking truncated modules by call fan-in.

`build_context_pack` accepts `symbol` (exact name), `path`/`language` (pick the definition of an ambiguous name; callers and tests stay project-wide), `depth` (hops both ways, default 1), `limit` (related rows, default 30), `source_lines` (per definition, default 40, max 200) and `format` (`text`/`toon`). It composes `find_symbol` + `find_calls` + `find_callers` + `find_references` + the `impact_analysis` test heuristic, plus the new `Index::find_dependencies_scoped` (non-call relations a symbol makes), and reads each involved file at most once for doc comments and signatures. Example, `{"symbol": "place_order"}` on `crates/mct-mcp-server/tests/fixtures/context-pack-app`:

```text
Context pack for `place_order` (depth 1): 1 definition(s), 8 related symbol(s), 3 unresolved name(s)

src/orders.rs:L35-L49 [rust] function place_order
31| /// Places `order`: checks it is well formed, reserves stock for every line,
...
35| pub fn place_order(
...
49| }

Related symbols, each listed once:
  callee validate_order  src/orders.rs:L52-L62 function  | pub fn validate_order(order: &Order) -> Result<(), OrderError> {
  callee apply_discount  src/pricing.rs:L32-L35 function  | pub fn apply_discount(cents: u64, coupon: Option<&str>) -> u64 {
  callee insert  src/store.rs:L27-L31 method  | pub fn insert(&mut self, order: StoredOrder) -> u64 {
  caller place_orders  src/orders.rs:L82-L91 function  | pub fn place_orders(
  caller,test test_place_order_stores_the_discounted_total  tests/orders.rs:L18-L25 function  | fn test_place_order_stores_the_discounted_total() {
  ...

Referenced at file level (use/import) by: tests/orders.rs
Not resolved to an indexed definition (std/third-party/unknown): Ok, as_deref, len
```

Relations are walked from the packed definitions' exact symbol rows. A callee or dependency is a related symbol only when its target is resolved to one definition ([[002-qualified-relation-resolution]]); an ambiguous one is listed under a separate `Uncertain relations` section with its first candidates (`callee run  ambiguous among 2: …`), never picked, and never followed by `depth`; a caller whose call is ambiguous is listed there too. Unresolved and external names go to the footer.

Every relation tool (`find_references`/`find_calls`/`find_callers`/`impact_analysis`) still matches its input by name at hop 1, but each text line ends in ` (ambiguous: N)`, ` (unresolved)` or ` (external)` when its target isn't resolved (a resolved line is unchanged), and `format: toon` adds `resolution` and `candidates` columns. Hops past the first follow resolved edges only.

See [[mct-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

#architecture #mcp
