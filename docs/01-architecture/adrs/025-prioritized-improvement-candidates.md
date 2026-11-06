# ADR-025: Prioritized improvement candidates

## Status
Proposed (2026-11-04). Linked from the [roadmap](../../05-backlog/roadmap.md). Each candidate that touches the SQLite schema or the MCP tool surface needs its own issue before implementation, as [CLAUDE.md](../../../CLAUDE.md) requires.

## Context
- Users cannot yet tell which improvements matter most. The roadmap lists them without an order, and some candidates overlap with features that already exist.
- Each candidate was checked against the code on 2026-11-04. Where a feature already exists, the candidate is narrowed to what is missing.
- Constraints: `rules.md` requires shared repository content in English and forbids private agent state in shared documentation. Schema changes need an issue first.

## Decision
Adopt the following ordered candidates. Order reflects impact and cost, with security first.

| # | Candidate | Verified state | Effort | Risk |
|---|---|---|---|---|
| 1 | Exclude secret files for Ruby, PHP and Swift (`wp-config.php`, `config/database.php`, `config/master.key`, `config/credentials.yml.enc`, `*.xcconfig`, `GoogleService-Info.plist`) | Missing in `crates/mct-index/src/exclude.rs`. Only generic patterns exist (`.env`, `credentials.json`). | Small | High if left open: the index can store credentials. |
| 2 | Show relation resolution state (`resolved`, `ambiguous`, `unresolved`) in every tool that returns relations | States exist in `crates/mct-index/src/queries.rs` (lines 54-72) and are used by `format.rs`. Remaining work: verify every relation tool shows them, and document the meaning. | Small | Low |
| 3 | `mct-cli report`: one architecture report combining most-used symbols, import cycles, duplicates and dead-code candidates | Building blocks exist: `cycles.rs`, `dead_code.rs`, `duplicates.rs`. Most-used symbols can be computed from `relations.to_symbol_id`. Overlaps with the web overview (`crates/mct-cli/src/web/api/overview.rs`). | Small-medium | Medium: duplicate logic if not shared |
| 4 | Installer for instruction files (`AGENTS.md`, `CLAUDE.md`) that adds an idempotent, delimited block pointing to `rules.md` | Missing. `mcp-register` only writes `.mcp.json` (`crates/mct-cli/src/main.rs:575`). | Small-medium | Medium: writes to user-owned files; must update without deleting user content |
| 5 | GraphML export of the module and call graph | Missing. DOT and Mermaid exist in `crates/mct-mcp-server/src/graph_render.rs`. | Small | Low; adds little over DOT and Mermaid. Deferred. |
| 6 | Local query log and token-savings tracking | Partial. `crates/mct-mcp-server/src/cache.rs` caches search results but records no usage. Covers roadmap item 11. | Medium | Medium: must store metadata only, never source text |
| 7 | `NOTE`, `WHY` and `TODO` annotations, queryable from `find_symbol` and `build_context_pack` | Missing. Comments are discarded during tokenization (`crates/mct-core/src/tokens.rs`). Each language parser must capture them, and the schema changes. | Medium-large | High: schema change |
| 8 | Module community detection | Missing. Module graph comes from `imports` relations; the `dependencies` table holds only external manifest entries. Requires a new algorithm with deterministic output. Deferred. | Large | Medium |
| 9 | Exclude generated build output by default (`website/out`, `website/.next`, `crates/mct-cli/web-ui` build files) | Missing. Generated CSS produces ERR-023 and ERR-024 (open), which leave `find_duplicate_code` with `coverage_complete: false`. Dedicated patterns belong in `exclude.rs`. | Small | Low |
| 10 | Report what was left out of duplicate and dead-code results (count and reason per omitted file) | Partial. `coverage_complete`, `skipped` and `index_errors` are already returned; the omitted files are not named. | Small | Low |
| 11 | Suggest the correct parameter name when a tool call uses an unknown one (for example `name` instead of `symbol`) | Missing. ERR-014 recurred on 2026-10-20 with `build_context_pack`. Parameter deserialization in `mct-mcp-server/src/server.rs` gives only the serde message. | Small | Low |
| 12 | Show the indexed root and file count in responses, and name the expected root when a path is not found | Missing. ERR-016 (2026-10-29): a session queried a 5-file index and the path error did not say which root was indexed. `file_tree` error in `format.rs` shows only the path. | Small-medium | Low |
| 13 | Detect concurrent writers to one index and keep build artifacts separate per worktree | Open. ERR-018: `cargo build` failed with artifacts from another worktree while `cargo check` passed. Documentation exists; detection does not. | Medium | Medium |
| 14 | Polling fallback when the native file watcher delivers no events | Open. ERR-009: FSEvents delivered no events on macOS, while kqueue did. `background.rs` uses `notify` only, with no fallback. | Medium | Medium |
| 15 | Separate expected malformed fixtures from real parse failures in indexing status | Open. ERR-003: 16 of 20 diagnostics come from intentional fixtures, which hides real failures. `index_issues` does not classify them. | Small-medium | Low |
| 16 | Accept Tailwind v4 CSS directives (`@source`, `@theme`, nesting, `@import` forms) | Open. CHANGELOG limitations and issues #87, #115, #116; ERR-023 is the `@source` case. May depend on upstream grammar fixes. | Medium-large | Medium |
| 17 | Reduce rejections of valid syntax: Bash `coproc` (#79) and C++ constructs rejected by tree-sitter-cpp (#82) | Open, documented in CHANGELOG. Each case needs a small regression fixture. | Medium per case | Low-medium |
| 18 | `find_tests(symbol?)`: tests that cover a symbol | Missing. Listed as candidate 7 in the roadmap history. Related tests appear only inside `build_context_pack` and `impact_analysis`. Adds a tool, so it needs an issue. | Medium | Medium |
| 19 | Inheritance tree: parents, children and implementations of a class or interface, shown as a tree | Partial. `Extends` and `Implements` relations exist (`crates/mct-core/src/symbol.rs:82`), but no tool walks them as a tree. | Small-medium | Low |
| 20 | Call-flow diagram from one symbol: callers and callees to N hops, as Mermaid or DOT | Partial. Multi-hop walks exist in `find_calls` and `find_callers`; DOT and Mermaid (`graph_render.rs`) render only the module graph. | Medium | Low-medium |
| 21 | Hotspots: large functions that many callers depend on, ranked by size times callers | Partial. `end_line` exists (`list_symbols`) and caller counts are available. No complexity or hotspot metric exists. | Small-medium | Low |
| 22 | Public surface of a module: the symbols it exposes to other modules | Missing. `mct-core` stores no visibility, so each parser must capture it, per language. | Medium-large | Medium |
| 23 | Hidden coupling: files that change together in git history without a direct dependency | Missing. The code never reads git history. Needs a bounded walk over history to keep it fast. | Medium | Medium |
| 24 | Change impact from a diff or branch: affected symbols, callers and tests for the changed files | Missing as an input. `impact_analysis` works per symbol and accepts no git diff. Adds a tool, so it needs an issue. | Medium | Medium |
| 25 | Path between two symbols: the chain of relations connecting A to B | Missing. Walks start from one symbol; there is no path query. Adds a tool, so it needs an issue. | Medium | Medium |
| 26 | Compare two indexes (two branches or roots): added, removed and changed-signature symbols | Partial. `symbol_changes.rs` counts changes per reindex and `--roots` handles roots independently; no comparison is produced. | Medium-large | Medium |
| 27 | Export the full index to JSON or SQLite for external tools and dashboards | Partial. `mct-cli call --json` returns one query at a time; no full export was found in the subcommands reviewed. | Small | Low |
| 28 | Documentation coverage: public functions and types without a doc comment | Missing. Doc comments are used for semantic embeddings (`semantic.rs`) but not stored per symbol. Depends on candidate 22 to know what is public. | Medium-large | Low-medium |
| 29 | Exact name resolution through SCIP ingestion or an LSP | Missing. Relations are matched by name (`WHERE name = ?`, `queries.rs`), and type resolution is AST-only (CHANGELOG known limitations). No SCIP reader exists in `crates/`. Largest precision gap. | Large | High: changes the core symbol model |
| 30 | Faster indexing of large repositories through parallel parsing | Missing. The workspace has no parallel parsing (`rayon` absent). A cold 9,288-file copy took 57 to 136 seconds before the server answered (ERR-022, fixed only for the first response, not the indexing time). | Medium-large | Medium: SQLite writes and deterministic output must stay correct |
| 31 | Semantic search in the prebuilt release binaries | Partial. The `semantic` feature exists, with a `symbol_embeddings` table (`schema.rs`), but release binaries are built without it (`release.yml`). Users must build from source for embedding search. | Small-medium | Medium: binary size and model distribution |
| 32 | Wider distribution: publication on crates.io and package-manager formulas | Partial. One-command installers exist (`install.sh`, `install.ps1`). crates.io publication is blocked by the vendored `tree-sitter-md` patch (`Cargo.toml`, ROADMAP candidate 12). No package-manager formula exists. | Medium | Low |
| 33 | Public, reproducible benchmark against other tools on a public repository | Missing. Benchmarks in `benchmarks/` and the `token_benchmark` example are internal. Results against other tools are not published, and no public repository is used. | Medium | Medium: the method must be fair and publish losing cases too |
| 34 | Relations across roots and monorepos, for `--roots` | Missing. Each root is handled independently (README, "Several projects or worktrees at once"). Cross-root calls and imports are not resolved. | Medium-large | Medium |
| 35 | JSX and TSX components as symbols, with used components, props and events | Missing. Listed as open in the web/3D section of the roadmap and in issue #51; CHANGELOG limitations say framework structure is only partially indexed. | Medium | Medium: grammar coverage |

Gap items 29 to 35 come from the later review. Sequencing for them: 31 and 32 first (shipping, low code risk), then 30 and 33 (baseline and speed), then 35 and 34, and 29 last. Start 29 with a design record, because it changes the core model; implement it after 33 gives a measured baseline.

Recommended sequence: 1, 2, 9, 11 and 15 first (no schema change, small). Then 3, 4, 10 and 12. Then 6, 7, 13 and 14. Then 8, 16, 17 and 18. Candidates 5 and 8 stay out of the next iteration until a benchmark justifies them.

Feature candidates 19 to 28 come after: first 27, 19 and 21 (small, no schema change), then 20, 24 and 25 (new tools, each needs an issue), then 23, 26, 22 and 28 (larger, depend on new data).

## Consequences
- Security exposure from candidate 1 is closed early, without waiting for larger work.
- Candidate 2 may turn out to need little more than documentation, which keeps the first iteration small.
- Candidates 3 and 4 add user-facing output and file writes, so each needs tests that cover idempotency and existing content.
- Candidates 6 and 7 touch persistence or the schema, so each needs an issue and a migration plan first.
- Candidates 5 and 8 are postponed. This avoids building features with no measured benefit.
- Each adopted candidate should get its own ADR if its design involves a trade-off not covered here.

#adr
