# MCP Protocol Spec

`mct-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

## Tools

| Tool | Type | Purpose |
|---|---|---|
| `list_symbols` | discovery | List symbols under a file or directory/crate prefix |
| `get_file_skeleton` | discovery | Top-level shape of one file, bodies collapsed |
| `get_project_overview` | discovery | Compact hierarchical digest of a file/directory/crate/project |
| `get_file_tree` | discovery | Directory/file tree without symbol analysis |
| `find_symbol` | atomic | Exact-name symbol lookup |
| `search_symbols_like` | discovery | Ranked word search over symbol names |
| `hybrid_search_like` | discovery | Word search fused with embedding similarity |
| `semantic_search` | discovery | Embedding-only search when the optional semantic feature is enabled |
| `search_symbols` / `hybrid_search` | deprecated aliases | Old names of the two tools above, kept for one version |
| `find_references` | atomic | Every reference to a symbol |
| `find_calls` | atomic | What a function calls |
| `find_callers` | atomic | What calls a function |
| `impact_analysis` | composite | Full blast radius in one call |
| `get_dependency_graph` | composite | Module-level import graph: nodes by hop, resolved import edges, cycles, external imports, optional DOT/Mermaid |
| `build_context_pack` | composite | Definition + doc + capped source, and every caller/callee/dependency/test once, in one call |
| `find_dead_code` | analysis | Heuristic dead-code candidates, with valid finding-level review decisions applied |
| `find_duplicate_code` | analysis | Review-aware whole-function duplicate candidates; compact bounded references (#233) |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |
| `batch` | meta | Run up to 25 read-only tool calls in one envelope |
| `discover_tool_categories` | meta | Progressive list of tool names and purposes |
| `get_tool_schema` | meta | Progressive schema lookup for one registered tool |
| `select_tools` | meta | Tool Attention, only with `MCT_TOOL_ATTENTION=1`: narrow `tools/list` to one request's tools |

The default catalog contains 23 tools; `select_tools` is conditional on
`MCT_TOOL_ATTENTION=1` (24 total with it enabled). Phase 8.1 adds read-only
`find_duplicate_code` under issue #233 and owner-approved ADR-022, reusing the
CLI/dashboard detector and shared review model. Earlier Phase 8 measurements
and gap descriptions below are historical baselines.

`select_tools` accepts `query` (the user's request, verbatim). With `MCT_TOOL_ATTENTION=1` the server declares `tools.listChanged` and `tools/list` starts with `select_tools` alone. Each call runs `intent::preselect` on the query (word rules only; the full catalog when they find nothing), makes `select_tools` plus the picked tools the advertised set, sends `notifications/tools/list_changed`, and replies with the intent line, the tool names and argument hints. It never runs a tool. `tools/call` accepts every catalog tool whatever is advertised. Without the variable, `select_tools` is not registered and nothing else changes. `batch` refuses it (with the variable off it is an unknown tool there too).

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`. `find_references` also accepts `sort`: `relevance` (default) or `execution` (by hop, file and line; a static approximation, not the real execution order).

`get_project_overview` accepts `path` (optional, same matching semantics as `list_symbols` — omitted means the whole project root), `language`, `max_symbols_per_module` (default 8, caps top-level symbols surfaced per file), and `include_relations` (default true, whether to show each surfaced symbol's top callers). It composes `list_symbols` + `find_callers` internally — no new SQL, no new schema — ranking truncated modules by call fan-in.

`build_context_pack` accepts `symbol` (exact name), `path`/`language` (pick the definition of an ambiguous name; callers and tests stay project-wide), `depth` (hops both ways, default 1), `limit` (related rows, default 30), `source_lines` (per definition, default 40, max 200) and `format` (`toon` default, or `text`). It composes `find_symbol` + `find_calls` + `find_callers` + `find_references` + the `impact_analysis` test heuristic, plus the new `Index::find_dependencies_scoped` (non-call relations a symbol makes), and reads each involved file at most once for doc comments and signatures. Example, `{"symbol": "place_order"}` on `crates/mct-mcp-server/tests/fixtures/context-pack-app`:

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

Every relation tool (`find_references`/`find_calls`/`find_callers`/`impact_analysis`) still matches its input by name at hop 1, and each text line names its target: ` -> path:line in Parent` when resolved (`Lline` in the hit's own file), ` (ambiguous among N: …)` with the first candidates, ` (unresolved)` or ` (external)`; `format: toon` adds `resolution`, `candidates` and `targets` columns. With `path`/`language`, a hit proven to reach a same-named definition outside the scoped start is dropped. Hops past the first follow resolved edges only.

See [[mct-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

## Phase 8 — MCP retrieval consistency and review-aware context

Status: a review-aware dead-code response change is implemented and locally
verified; full retrieval acceptance remains pending. Audit date: 2026-10-25, branch
`feature/dead-code-editor`, base `b61cc7ce971fdc46a61259af95af7fac823ae352`.
The checkout has substantial pre-existing local changes; this phase preserves
them. Historical Phase 1–7 test counts are not current results.

### Verified data and consistency contract

- Search, graph, context-pack, dead-code, and file tools read the derived
  `index.sqlite3`. Composite operations open a SQLite read snapshot for that
  operation. The index mutex also serializes requests through one server
  instance; other processes can publish a new generation independently.
- Dead-code candidates pass through `ReviewSnapshot::filter_dead` before
  pagination. The shared identity/fingerprint resolver suppresses only a
  currently valid finding-level decision. Changed or unverifiable source is
  not declared suppressed. Confirmed findings remain visible. Copy-level
  duplicate statuses do not suppress their group.
- Search ranking caches are keyed to project, tool/configuration and index
  generation. Search response budgets bypass the ranking cache; context packs
  are not cached. The index change marker is conservative and can move even
  when a reindex made no source changes.
- `limit`/`offset` pagination does not pin a snapshot across separate calls.
  Callers that need cross-page continuity must detect index changes and restart;
  a continuation token or retained cross-request snapshot is not implemented.
- The tool protocol is stdio via `rmcp` 3.4.1 in the current manifest. No tool
  schema or protocol version changed in this Phase 8 slice.

### Review-aware result states

| Condition | Ordinary MCP result | Meaning |
|---|---|---|
| No indexed candidate | Empty result text | The current analysis found no candidate |
| Valid finding-level false-positive/ignored decision | Candidate filtered | Current verified review suppresses it |
| Confirmed finding | Candidate remains | Confirmation is not suppression |
| Changed/unverifiable source or ambiguous identity | Candidate remains where analysis still finds it | Historical decision is not asserted effective |
| Copy-level accepted/rejected/ignored state | No group suppression | Per-copy workflow remains independent |
| Index read/review-state failure | Tool error | No successful empty result is claimed |

`find_dead_code` now includes the number of candidates removed by valid saved
review decisions. If all candidates are filtered, it says that the actionable
result is empty because reviews suppressed candidates; it no longer implies the
analysis found no candidates. The count is computed before output pagination.
Historical decision details remain in dashboard/CLI interfaces; MCP does not
currently offer an explicit historical-review inspection tool.

### Token and relevance baseline (new, 2026-10-25)

Executed on the current dirty checkout before the Phase 8 response wording
change: `cargo run -q -p mct-eval` and `scripts/unix/token-report.sh`.

| Measure | Observed result | Scope/limitation |
|---|---:|---|
| `mct-eval` cases / timed iterations | 33 / 5 | `omni-app` fixture |
| Accuracy / ranked MRR / success | 1.000 / 0.917 / 100% | Existing labeled cases; no recall metric |
| Response tokens / tool catalog | ~6,845 / ~11,433 | `EstimatedCounter` estimates, not a model tokenizer or billed tokens |
| Query latency p50 / p95 / max | 3.41 / 11.82 / 24.17 ms | Fixture run; no resource profile |
| Index build | 170.8 ms | Evaluation fixture, not the full project |
| `batch` versus separate calls | ~30.6% and ~36.0% fewer | Two repository-sweep scenarios; excludes other task calls |
| Context pack versus manual aggregation | ~84.3% fewer | One `place_order` fixture; not a universal workflow saving |

These are the baseline, not gains from this phase. Tool catalog tokens are a
separate cost from response tokens. Whole-task consumption includes every call
and any repeated response; no general tokenizer-backed end-to-end task benchmark
or duplicate-analysis recall/precision dataset is currently available.

Post-change check on the same evaluation harness: 33 cases, 5 timed iterations;
accuracy 1.000, ranked MRR 0.917, success 100%, response ~6,845 tokens, catalog
~11,433 tokens, latency p50/p95/max 4.55/23.74/126.23 ms, fixture index build
295.6 ms. The evaluator reported no regressions. This is not evidence of a
latency improvement: timing varies between runs, and the change affects only
dead-code result wording/counting. Workspace tests passed (1,946 passed, 26
ignored); strict Clippy and formatting passed in the full check invocation.

### Phase 8 requirements and acceptance

Correctness takes precedence over token reduction:

1. Preserve per-operation index snapshots and generation-aware cache checks.
2. Keep cross-request pagination explicitly generation-unpinned; report this
   limitation rather than claiming stable pages.
3. Apply valid finding-level review suppression before result limits, and never
   let copy-level states suppress duplicate groups.
4. Distinguish “no analyzed findings” from “all findings suppressed by valid
   reviews”; retain uncertainty and source-verification caveats.
5. Keep current tools read-only, bounded, parameterized, root-scoped, and
   deterministic. Treat source text as untrusted input, not agent instructions.
6. Preserve MCP tool schemas and protocol compatibility unless a tracked,
   reviewed issue authorizes an additive tool contract.
7. Measure accuracy and recall alongside response, catalog and end-to-end token
   costs before adopting optimizations. Do not trade omitted relevant findings
   for shorter responses.

Acceptance: unchanged valid false positives stay absent from actionable output;
materially changed findings are eligible again; confirmed findings stay visible;
generation/cache regression tests pass; paginated calls document/recover from
generation changes; token and quality measurements use the same fixture and
iteration method; no production database is touched.

### Duplicate retrieval decision

The absent MCP duplicate query is a verified capability gap. Existing
`mct-index::find_duplicate_groups`, `mct-languages::build_token_sources`, and
`ReviewSnapshot::duplicate_visibility` provide reusable primitives, and the
CLI already composes them under one read snapshot. A dedicated read-only
`find_duplicate_code` query is the cleanest candidate, because overloading
symbol search or dead-code output would give those tools misleading semantics.
It would still add catalog cost and needs a carefully bounded argument/output
contract, source verification, explicit group decision state, stable ordering,
and tests against stale dashboard decisions.

This proposal is not implemented: `rules.md` requires an issue before changing a
published MCP tool contract, and no issue was opened or published in this phase.
No CLI/web implementation is being duplicated inside MCP. Once a tracked issue
is authorized, compare the tool's whole-task token cost and recall against the
current multi-surface workflow before accepting it.

### Definition of Done

- Review decisions and generations have deterministic MCP semantics.
- New and changed findings remain visible when old decisions no longer verify.
- No actionable-result message conflates filtering with absence.
- MCP schemas remain compatible, and pagination limitations are explicit.
- Accuracy, recall, token use and latency are measured on repeatable fixtures.
- Any new tool has prior issue approval, bounded schema, shared review logic,
  and real MCP client/schema tests.
- Required workspace, tool, review, generation and backup checks pass.
- Documentation describes verified behavior; production and other-platform
  acceptance is reported separately.

#architecture #mcp

## Phase 8.1 — Review-aware duplicate retrieval (#233)

Contract proposal: [issue #233](https://github.com/Zubiarka8/mini-consumes-tokens/issues/233)
and [local issue text](../../tasks/phase8-1-issue.md). The owner explicitly approved
this contract in the implementation session on 2026-10-26. This authorizes local
implementation, not merge or production deployment. ADR-022 records the decision.

### Requirements, implementation plan and acceptance

`find_duplicate_code` is an additive read-only tool. Inputs/defaults/bounds and
output are specified by #233. It composes the existing detector and token-source
registry, one index snapshot, shared review resolution and a compact renderer.
No schema migration, dependency or similarity-threshold change is required.

Implementation order: shared group review diagnostics → query/validation and
budgeted TOON response → registry/TTC/batch → isolated integration/client tests
→ CLI/dashboard parity, quality/token/latency measurements and official checks.
Owned paths: mct-index review_state; MCP query/server/catalog/tests; existing
specification, ADR, verification, inventory, roadmap and error records. Single
writer, preserved dirty base b61cc7ce971fdc46a61259af95af7fac823ae352.

Read behavior: take the index snapshot before detector/navigation/generation
queries. Load review state independently; never claim atomic cross-database
reads or persistent cross-request snapshots. Do not cache this tool. Source
changes after a verified read are possible; source verification describes the
operation's checks, not a frozen repository. Skips/unsupported languages/index
errors make analysis coverage explicit. Corrupt review state returns an error,
not silently suppressed results. Copy labels never determine group visibility.

Decision validity: unreviewed, valid, changed, unavailable, or ambiguous.
Only valid false_positive/ignored decisions are nonactionable; pending and
confirmed are actionable. Deleted/tombstoned decisions resolve as pending.
Historic saved decisions remain stored. Fingerprints use existing exact reviewed
text semantics (including formatting/comments), not a new normalization rule.

Budget: EstimatedCounter, including metadata; default 4000, accepted 512..16000.
Whole groups are preferred. If one group cannot fit, omit bounded member rows
and report member truncation; if even its minimum identity record cannot fit,
report the unchanged next offset and ask for a larger budget. No hidden offset
advance. Limit 10 by default, 1..100; offset is validated and never added without
saturating arithmetic. Output/path/symbol text is untrusted, escaped data.
Analysis cost remains proportional to eligible repository source; response caps
do not imply constant-memory detector execution. No partial-block mode.

Acceptance requires pending/confirmed/FP/ignored fixtures; unchanged and changed
reindex, group membership/identity changes, source loss, copy independence,
read-only review preservation; deterministic pagination and explicit truncation;
real rmcp tools/list/tools/call; CLI/dashboard parity; generation/cache/backup
regressions; strict checks. Measure catalog overhead and fixture whole-task cost
separately, report labeled suppression correctness separately from detector
precision/recall. Existing 33-case evaluation remains a separate regression gate.
Cross-platform and production acceptance must be recorded as unverified unless
actually run. Definition of Done is the Phase 8.1 issue acceptance plus these
checks; owner contract approval alone is not completion.

### Phase 8.1 output and recovery contract

The tool returns one MCP text content block containing TOON. `status` is
`no_groups`, `all_suppressed` or `groups`; `coverage_complete` independently
reports source/index/language coverage. A zero-result page at a high offset
keeps `groups` status rather than claiming no detection. Counts apply to the
entire filtered analysis before offset/limit. `include_reviewed=true` changes
visibility, not actionability. Each `group[1]` has identity, language, member
count, token/line counts, saved/effective decision, validity, source_verified
and actionable. Its members table has repository-relative path, symbol, kind,
start/end lines, with `members_omitted`. Use existing find_symbol / context-pack
tools for code, treating paths/names as untrusted data. Notes/history remain in
the dashboard. `next_offset=none` means no further groups; member omissions
require retrying the same offset with a larger budget. `truncated` covers
further groups and/or omitted member references. No continuation token or
persistent snapshot is introduced; restart paging if generation changes.

`max_tokens` uses the shared EstimatedCounter, not a model tokenizer. Output
caps do not bound detector CPU/memory or cancel a synchronous detector midway.
The existing server index mutex is held during analysis; large scans may delay
other requests. Per-candidate failures are reported as reason counts; unsupported
languages/index issues are explicit coverage diagnostics. Database/review errors
return generic MCP errors without source/private review content. The schema
rejects unknown fields; runtime checks reject invalid paths/limits/enums before
analysis. Standard MCP negotiation, transport and prior tools are unchanged.

The CLI `call` wrapper intentionally reindexes before dispatch. The MCP query
itself and the read-only dispatcher used for acceptance do not reindex or write
review state. Online backups remain independent readers of the dashboard.

Coverage refers to indexed candidates, not a fresh walk proving that no new
source file exists. Use the existing indexing status/reconciliation mechanisms
for freshness. A verified generation can lag newly added source until the
watcher or periodic reconciliation publishes another generation. No stronger
freshness guarantee is implied by coverage_complete=true.
