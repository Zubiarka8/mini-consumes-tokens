# Design: `get_project_overview` MCP tool (compact hierarchical context)

Date: 2026-09-18
Status: draft, high-level only — pending signature verification against live `ccm-index`/`ccm-mcp-server` source before implementation

## Goal

Add one new MCP tool that answers "what does this crate/directory/project look like" in a single call, returning a compact hierarchical digest (modules → key top-level symbols → most relevant call relations) instead of requiring an agent to chain `list_symbols` + `get_file_skeleton` + `find_calls` per file to build the same picture manually.

## Non-goals

- No change to `LanguageParser`, the SQLite schema, or any existing published MCP tool's signature — this is purely additive. Per `CLAUDE.md`'s cross-cutting rule, none of those changes are in scope here, so no issue needs to be opened first for this piece.
- No new "centrality"/"importance" column or index in SQLite. Any ranking (e.g. by call fan-in) must be computed by composing existing `ccm-index` query functions at call time, not by adding stored state.
- Not a replacement for `find_symbol`/`find_references`/`impact_analysis` — this tool is a coarse first-pass digest; precise lookups still go through the existing tools.
- No caching layer — that's the separate "BFS query cache" backlog item ([[roadmap]] in `docs/05-backlog/roadmap.md`), tracked independently since it *does* touch the schema.

## Proposed interface

**Name**: `get_project_overview`

**Args** (`schemars::JsonSchema`, mirroring the existing `list_symbols`-style arg struct pattern in `server.rs`):
- `path: Option<String>` — file, directory, or crate prefix. Same matching semantics as `list_symbols` (exact file match; prefix match for a directory/crate path with no extension). Omitted = whole project root.
- `language: Option<String>` — same filter semantics as `list_symbols`.
- `max_symbols_per_module: Option<u32>` — cap on how many top-level symbols are surfaced per file/module before truncating (default TBD, likely single-digit).
- `include_relations: Option<bool>` — whether to include the "most relevant calls" section at all (default true); lets an agent ask for a cheaper structure-only digest.

**Output** (rendered via a new `format::overview` function, same pattern as existing `format::*` renderers): a hierarchical, indented text digest — not raw JSON dump of every symbol — grouped by module/file, each entry showing:
1. Module/file path.
2. Its top-level symbols (function/struct/trait/impl-level, no nested items), capped at `max_symbols_per_module`, with a `(+N more)` suffix when truncated.
3. If `include_relations`, each surfaced symbol's top few callers/callees (reusing whatever bounded call-relation query the existing `find_calls`/`find_callers` tools already call into — no new SQL, no new schema).

## Composition from existing `ccm-index` API (to verify exact fn names once MCP tools are reachable)

1. Resolve `path`/`language` into a file set — same resolution `list_symbols` already does.
2. For each file, fetch its symbols filtered to "top-level" (no parent) — reuse whatever `list_symbols` already does for its `kind`/`language` filtering rather than writing a new query.
3. Rank/select which symbols to keep per module when the file has more than `max_symbols_per_module` — MVP ranking: fan-in count via the same call-relation lookup `find_callers` uses, computed per candidate symbol, capped depth 1 (no new BFS depth semantics beyond what already exists).
4. For `include_relations`, run the existing bounded call-relation lookup per surfaced symbol and keep only the top few edges by whatever ordering the existing tools already return (no new sort logic if one already exists; add the minimal cap otherwise).
5. Render everything through one new `format::overview` function — no raw per-symbol JSON in the response; the whole point is a smaller token footprint than chaining the granular tools.

## Open questions (need the live source, or user confirmation, before implementation)

- Exact `ccm-index` function signatures for "list top-level symbols in file X" and "fan-in count for symbol Y" — confirm these already exist in some form (per the user's earlier read of `queries.rs`/`lib.rs`) rather than requiring new query functions.
- Whether "top-level" should be exactly "`parent IS NULL`" or needs a `kind` allowlist (e.g. exclude `SymbolKind::Variable` even if unparented).
- Default value for `max_symbols_per_module` — needs a concrete token-budget target to size against.
- Whether pagination (`offset`, matching the existing tools' convention) is needed for very large crates, or whether the per-module cap makes that unnecessary for v1.

## Related

- [[roadmap]] — the two token-efficiency backlog items (BFS cache, Obsidian session notes) discussed alongside this tool.
- `docs/02-crates/core/ccm-mcp-server.md` — existing tool inventory this adds to.

#design #mcp-tool #backlog
