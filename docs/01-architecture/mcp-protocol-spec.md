# MCP Protocol Spec

`mct-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

## Tools

| Tool | Type | Purpose |
|---|---|---|
| `list_symbols` | discovery | List symbols under a file or directory/crate prefix |
| `find_symbol` | atomic | Exact-name symbol lookup |
| `search_symbols_like` | discovery | Ranked word search over symbol names |
| `hybrid_search_like` | discovery | Word search fused with embedding similarity |
| `semantic_search` | lookup | Embedding-only similarity search |
| `search_symbols` / `hybrid_search` | deprecated aliases | Old names of the two tools above, kept for one version |
| `find_references` | atomic | Every reference to a symbol |
| `find_calls` | atomic | What a function calls |
| `find_callers` | atomic | What calls a function |
| `impact_analysis` | composite | Full blast radius in one call |
| `find_dead_code` | analysis | Review-aware candidates with zero indexed references; TOON or text ([ADR-023](adrs/023-mcp-dead-code-reviews.md)) |
| `find_duplicate_code` | analysis | Review-aware whole-function duplicate candidates; compact bounded references ([ADR-022](adrs/022-mcp-duplicate-findings.md)) |
| `get_dependency_graph` | composite | Module-level import graph: nodes by hop, resolved import edges, cycles, external imports, optional DOT/Mermaid |
| `build_context_pack` | composite | Definition + doc + capped source, and every caller/callee/dependency/test once, in one call |
| `get_file_skeleton` | discovery | Top-level shape of one file, bodies collapsed |
| `get_project_overview` | discovery | Compact hierarchical digest of a file/directory/crate/project |
| `get_file_tree` | discovery | File and directory tree |
| `find_files` | discovery | Indexed files by path, name, category or format, with size; no contents ([ADR-052](adrs/052-path-catalog.md)) |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |
| `discover_tool_categories` | meta | List tool names and purposes by category |
| `get_tool_schema` | meta | Retrieve one tool schema |
| `batch` | meta | Run several read-only queries |
| `select_tools` | meta | Tool Attention, only with `MCT_TOOL_ATTENTION=1`: narrow `tools/list` to one request's tools |

`select_tools` accepts `query` (the user's request, verbatim). With `MCT_TOOL_ATTENTION=1` the server declares `tools.listChanged` and `tools/list` starts with `select_tools` alone. Each call runs `intent::preselect` on the query (word rules only; the full catalog when they find nothing), makes `select_tools` plus the picked tools the advertised set, sends `notifications/tools/list_changed`, and replies with the intent line, the tool names and argument hints. It never runs a tool. `tools/call` accepts every catalog tool whatever is advertised. Without the variable, `select_tools` is not registered and nothing else changes. `batch` refuses it (with the variable off it is an unknown tool there too).

`find_files` accepts `query` (words that must all occur in the path, case-insensitive), `path` (that file or directory and everything under it), `category` (one of the 20 in `mct_index::catalog::Category`), `format` (a format id or extension alias, normalized: `jpg` = `jpeg`, `tgz` = `tar.gz`, `ts` = `mpeg-ts`; or a parser language id for a source file), `limit` (1..=200, default 50), `offset` and `output_format` (`toon` default, or `text`; named so because `format` is the file format). Rows come in stored path order. The page states `total`, `offset`, `returned` and `next_offset` (`none` on the last page), and adds a `note` column only when a returned row's classification is doubtful. An unknown category or format, an out-of-range limit or a path outside the root is `invalid_params` naming the valid values. An index that was never filled says so. `mct-cli files` prints the same page. Example, `{"category": "document", "format": "pdf", "limit": 20}`:

```text
filters: category=document format=pdf
total: 1
offset: 0
returned: 1
next_offset: none
files[1]{path,category,format,bytes}:
  docs/manual.pdf,document,pdf,9
```

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`. `find_references` also accepts `sort`: `relevance` (default) or `execution` (by hop, file and line; a static approximation, not the real execution order).

`get_project_overview` accepts `path` (optional, same matching semantics as `list_symbols` — omitted means the whole project root), `language`, `max_symbols_per_module` (default 8, caps top-level symbols surfaced per file), and `include_relations` (default false, whether to show each surfaced symbol's top callers). It composes `list_symbols` + `find_callers` internally — no new SQL, no new schema — ranking truncated modules by call fan-in.

`build_context_pack` accepts `symbol` (exact name), `path`/`language` (pick the definition of an ambiguous name; callers and tests stay project-wide), `depth` (hops both ways, default 1), `limit` (related rows, default 30), `source_lines` (per definition, default 40, max 200), `source_offset` (lines to skip before the source, to page past the cap, default 0) and `format` (`toon` default, or `text`). It composes `find_symbol` + `find_calls` + `find_callers` + `find_references` + the `impact_analysis` test heuristic, plus the new `Index::find_dependencies_scoped` (non-call relations a symbol makes), and reads each involved file at most once for doc comments and signatures. Example, `{"symbol": "place_order"}` on `crates/mct-mcp-server/tests/fixtures/context-pack-app`:

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

`get_dependency_graph` accepts `module` (required; repo-relative file or directory), `depth` (hops, default 2, clamped to the project's maximum query depth), `limit` (standard), `format` (`toon` default, or `text`) and `export` (optional `dot` | `mermaid`). It is the module-level counterpart of `impact_analysis`: the unit is a file, and an edge `importer -> imported` exists only for a RESOLVED `Imports` relation. The answer carries the root, the nodes with their hop depth, the directed edges, the circular dependencies (one line per cycle, `none` otherwise), the external imports (unresolved or third-party, never edges), a `truncated` flag and, when `export` is set, the rendered graph. Cycles come from an iterative strongly-connected-component pass. Decision record: [ADR-006](adrs/006-dependency-graph-tool.md). Example, `{"module": "src/a.ts", "export": "mermaid"}` on a project where `a.ts` and `b.ts` import each other and `a.ts` also imports `lodash`:

```text
root: src/a.ts (depth 2)
nodes[2]{path,hop}:
  src/a.ts,0
  src/b.ts,1
edges[2]{from,to}:
  src/a.ts,src/b.ts
  src/b.ts,src/a.ts
circular[1]: src/a.ts -> src/b.ts -> src/a.ts
external[1]: lodash
truncated: false
graph LR
  n0["src/a.ts"] --> n1["src/b.ts"]
  n1 --> n0
```

Every relation tool (`find_references`/`find_calls`/`find_callers`/`impact_analysis`) still matches its input by name at hop 1, and each text line names its target: ` -> path:line in Parent` when resolved (`Lline` in the hit's own file), ` (ambiguous among N: …)` with the first candidates, ` (unresolved)` or ` (external)`; a resolved target that only a unique name left, with no evidence from the parser, ends in ` (by name)`. `format: toon` adds `resolution`, `evidence`, `candidates` and `targets` columns. `evidence` says why a resolved hit is resolved (issue #320): `declared` (the parser named the target's file, type or module) or `name` (a unique name alone: an inference). It is empty on other rows, whose `resolution` already says why. `RelationHit::evidence` in `mct-index` also tells an ambiguous receiver of unproven type (`receiver`) from several candidates (`candidates`), and marks `syntax` (external) and `none` (unresolved). `resolution` keeps its values. With `path`/`language`, a hit proven to reach a same-named definition outside the scoped start is dropped. Hops past the first follow resolved edges only.

See [[mct-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

#architecture #mcp

### Review-aware MCP dead-code findings

`find_dead_code` reads the shared TOON review decisions. Only a matching
`false-positive` or `dismissed` decision hides a candidate; filtering occurs
before `limit` (default 50) and `offset`. Optional `include_reviewed: true`
shows hidden rows with `actionable: false`. Both formats report `status`
(`no_candidates`, `all_suppressed`, `candidates`), `detected`, `actionable`,
`suppressed`, `review_store` (`ok` or `degraded`), `returned`, `truncated` and
`next_offset`. Unreadable stores hide nothing. Rows expose identity and review
label, never private notes or history. `max_tokens` is an estimated output
budget, 512..16000 (default 4000); it bounds the response, not analysis work.
Restart pagination after a reindex or a review change. Existing `path`,
`language`, `limit`, `offset` and `format` parameters retain their defaults.

Symbols that declare a join point are excluded from the candidate counts.
When declarations exist in scope, both formats report
`declared_entry_points_total` and show up to 20 declaration reasons, subject
to the output budget. Candidate pagination applies only to candidates.
See [ADR-038](adrs/038-declared-join-points.md).

A literal HTTP endpoint that JS/TS code uses (`fetch`, `axios`) and that an
axum, Actix Web or Express route declares is also a `calls` relation from the
user to the handler. It appears in `find_callers`, `find_calls`,
`find_references` and `impact_analysis` like any other `calls` hit, with no
new parameter or field. Several declarers of one endpoint give `ambiguous`
hits and no target.

### Source revision verification

MCP source consumers compare the bytes read with the indexed Git blob hash
before using indexed line ranges. Changed, deleted, binary or unindexed files
supply no search snippets or context-pack signatures/source. Metadata remains
queryable. `get_file_skeleton` rejects stale revisions with a reindex instruction.
Restoring the indexed bytes or reindexing restores source retrieval; cached
rankings do not bypass the byte check. Reads retain existing size/path limits.

### Live index generations

A running MCP server observes externally published reindexes without restart.
Both finding handlers resolve candidates and indexed review fingerprints within
one read snapshot. Generation changes invalidate incompatible cached rankings;
pagination is not pinned across requests, so restart paging after a reindex.
