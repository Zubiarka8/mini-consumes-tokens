# Optimization campaign: record

Recoverable record of the maintainability, reliability and efficiency campaign on branch `refactor/optimization`. It is the source of truth for the campaign's state; the local review store under `.mct-index/` and chat history are not.

- Started: 2026-11-03. Base: HEAD `15f8f74`.
- Scope: the whole repository, worked in blocks (B0–B14 below). Code changes only for tasks the maintainer approves.
- Not done by the campaign without an explicit request: commits, pushes, PRs, issues, reinstalling shared binaries, changing `LanguageParser`, the SQLite schema or published MCP tool signatures.

## Conventions

- **Campaign IDs** (`OPT-<area><n>`) identify findings. MCT finding hashes are hints only: they can change, or stay the same, when code changes (observed: the D01 group kept its hash after its members changed).
- **Statuses:** Pending · In analysis · Confirmed · Keep · Inconclusive · Approved · Implemented · Verified · Blocked.
- **Evidence labels:** `[MCP]` connected MCP server (installed binary, build commit unverified) · `[CLI-before]`/`[CLI-after]` `mct-cli` built from this branch before/after a change, in an isolated target directory, run on a disposable copy of HEAD · `[GIT]` · `[FS]` file system · `[GREP-auth]` Grep/Read authorized case by case (see below) · `[I]` inferred · `[NV]` not verified.
- **Snapshots:** the initial totals (193 duplicate groups, 336 dead-code candidates at `15f8f74`) are a reference, not targets. Each block re-runs detection and reconciles additions, removals and uncertain matches by path and symbol.
- **Fields per opportunity:** ID; area/type; paths/symbols/lines; problem and evidence; affected users/contracts/behavior; expected benefit; risk/scope; skill and how it is applied; existing tests; pending tests/measurements; status; recommendation/next action.

## Method and coverage

- Exploration uses the project's MCP tools and `mct-cli`, per `rules.md` (dogfooding).
- **Grep/Read exception, authorized for one list only:** to verify dead-code candidates that MCT cannot resolve, namely six zero-reference functions of `crates/mct-mcp-server/src/format.rs` and a sample of ten Rust types. Each later need is asked for separately.
- `find_duplicate_code` covers Rust, Python and Lua (plus the other languages its output does not list as unsupported). It does **not** cover bash, cpp, csharp, go, java, js/ts, kotlin, php or powershell, so the web UI JavaScript and `website/` have no duplicate analysis yet. `coverage_complete: false`; 9 members skipped (`invalid-range`).
- Duplicate groups 1–65 (of 193) were **listed** with a preliminary pattern classification. Only the D01 and D02 members, and the D05 helpers, were inspected in full.
- Dead-code candidates: all 336 listed (237 `crates/`, 77 `benchmarks/`, 22 `website/`); verified only by the sample above.
- Known MCT limits met: ERR-025 (calls inside macro arguments, documented), ERR-026 (type-position usages, classification pending), ERR-024 (CLI coordination timeout). See [errors/REGISTRY.md](../../errors/REGISTRY.md).

## Repository map (preliminary)

| Area | Location | Class |
|---|---|---|
| Model and parser trait | `crates/mct-core` | Production |
| Index, schema, queries, duplicates, reviews, manifests | `crates/mct-index` | Production |
| Language parsers | `crates/mct-lang-*` (17), `crates/mct-tree-sitter` | Production; `tests/corpus/**` fixtures (`malformed/` intentional) |
| Language registry | `crates/mct-languages` | Production |
| MCP server | `crates/mct-mcp-server` | Production; `examples/` development support |
| CLI and local web dashboard | `crates/mct-cli` (`src/web`, `web-ui/`) | Production; `src/web/review-dist/` generated, tracked |
| Obsidian atlas | `crates/mct-obsidian` | Production `[NV]` |
| Evaluation and corpus harness | `crates/mct-eval`, `crates/mct-corpus` (`publish = false`) | Development support |
| Benchmarks | `benchmarks/` | Fixtures and evaluation data |
| Scripts and CI | `scripts/unix`, `scripts/windows`, `install.*`, `.github/workflows` | Automation |
| Documentation and records | `docs/`, `errors/`, `internal/`, root `*.md` | Documentation |
| Public site | `website/` (Next.js) | Production (site) |
| Vendored code | `vendor/` | Present on disk, absent from `get_file_tree` `[I]` excluded |

## Block plan

| Block | Area | Status |
|---|---|---|
| B0 | Preparation (instructions, tool help, map, exclusions) | Partly done; see "Open items" |
| B1 | Approved tasks D01, D02, D05a/b/c | Done; see below |
| B2 | Duplication (full pass, modes, reconciliation) | Pending |
| B3 | Dead code (§7.4 checklist per candidate) | Pending |
| B4 | Architecture | Pending |
| B5 | Index and search limits (ERR-024/025/026, ERR-023) | Pending |
| B6 | MCP contracts | Pending |
| B7 | CLI and scripts (Unix/Windows parity) | Pending |
| B8 | Web dashboard (read-only on the live instance; writes on a disposable project) | Pending |
| B9 | Tests | Pending |
| B10 | Performance (existing benchmarks first) | Pending |
| B11 | Dependencies | Pending |
| B12 | Security and errors | Pending |
| B13 | Documentation | Pending |
| B14 | CI | Pending |

## Block 1 results

### OPT-D01 — `find_calls` / `find_callers` handlers

- **Area/type:** duplication, MCP server.
- **Location:** `crates/mct-mcp-server/src/server.rs` `find_calls`, `find_callers` (HEAD lines 2311–2385).
- **Problem/evidence:** `[MCP]` the two handlers ran the same pipeline (validate name, defaults, output format, scope, snapshot, `relation_page`, render), differing only in `RelationDirection` and the result label. `FindCallsArgs` and `FindCallersArgs` have identical fields; their doc comments differ and are the published input schemas.
- **Options compared:** (a) keep both; (b) private struct plus two `From` impls (more code than it removes); (c) private method taking seven fields plus direction and label (trips `clippy::too_many_arguments`); (d) a private `RelationRequest` built with field shorthand plus `MctServer::relation_tool(request, direction, label)`. Chosen: (d), because the pipeline then exists once, so the two tools cannot drift in defaults, validation order or errors. Line count is about even; that was not the criterion.
- **Contracts:** unchanged. Public `#[tool]` methods, their names, both `*Args` structs and labels kept; `batch` still dispatches by tool name.
- **Skill:** `reduce-duplication` and `code-simplification` (project-local; SKILL.md read and its steps applied).
- **Verification:** `scripts/unix/check.sh -p mct-mcp-server`: fmt ok, 434 tests passed, 0 failed, 1 ignored, CI clippy clean. `[CLI-before]` vs `[CLI-after]` on a disposable copy of HEAD: 16 calls (8 per tool: default, text format, depth/limit/offset, path+language scope, unknown symbol, empty name, invalid format, unknown language) have identical stdout, stderr and exit status. Full `check.sh` (with `mct-eval`): see "Block 1 verification".
- **Detector after the change:** the group (same hash) still lists both handlers, now as the 28-line argument-forwarding wrappers. Not evidence either way.
- **Status:** Verified (pending the full check below).

### OPT-D02 — `eval_tokens` in `catalog_report`

- **Location:** `crates/mct-mcp-server/examples/catalog_report.rs` (`eval_tokens`, HEAD line 53).
- **Problem/evidence:** `[MCP]` a copy of `approx_tokens` that commit `753dde3` left behind.
- **Change:** include `tests/common/tokens.rs` with `#[path]` (same pattern as `format_benchmark.rs`); consumer list in that file's doc updated.
- **Verification:** example output checked deterministic (two runs identical), then compared before/after: byte-identical (55 lines). Examples build; fmt and CI clippy clean.
- **Not changed:** `mct-eval::approx_tokens` (dependency direction).
- **Status:** Verified.

### OPT-D05a — `uuid_like` in `mct-index` tests

- **Location:** nine copies in `crates/mct-index/tests/{exclude_pruning,file_tree,hybrid_search,manifests,reindex,scoped_queries,symbol_match,symbol_search,traversal}.rs`.
- **Evidence:** `exact` filter selected them; full comparison: byte-identical 10-line bodies; one call per file, each with its own directory prefix (kept). Each test file is its own binary, so a `#[path]` include keeps one counter per binary, as before.
- **Change:** `crates/mct-index/tests/common/unique.rs`, included with `#[path]`.
- **Verification:** `scripts/unix/check.sh -p mct-index`: fmt ok, 494 passed, 0 failed, 2 ignored, CI clippy clean.
- **Status:** Verified.

### OPT-D05b — `tempdir` in `mct-index` tests

- **Location:** `crates/mct-index/tests/{exclude_reload,incremental,read_failures,symbol_changes}.rs`.
- **Evidence:** the four copies differ only in the directory prefix, which identifies leftovers per test file; 36 call sites use `tempdir()` without arguments.
- **Decision:** Keep. Sharing it needs a prefix parameter plus a per-file wrapper to leave the call sites alone, for no net reduction (about 42 lines vs 44) and no gain in clarity.
- **Related, not merged:** the nine `uuid_like`-based `tempdir` helpers name directories by time and do not pre-clean: a different behavior.
- **Status:** Keep.

### OPT-D05c — `content_of` in `mct-mcp-server` tests

- **Location:** eight copies in `crates/mct-mcp-server/tests/{hybrid_search,luarocks_tools,omni_tools,portfolio_3d,progressive_discovery,search_symbols,tools,toon_output}.rs`.
- **Evidence:** byte-identical bodies returning the first text block, else `""`. Other helpers behave differently with multi-block results and were left alone: `contracts.rs::text` (all blocks, no separator), `context_pack.rs`/`exclusion_privacy.rs::content_of` (all blocks joined by `\n`), and a further `batch.rs`/`toon_default.rs::content_of` pair found after the change (not compared yet).
- **Change:** `crates/mct-mcp-server/tests/common/content.rs`, included with `#[path]` (avoids compiling `common/mod.rs` fixtures).
- **Verification:** `scripts/unix/check.sh -p mct-mcp-server`: fmt ok, 434 passed, 0 failed, 1 ignored (same count as before the change), CI clippy clean.
- **Status:** Verified.

### Block 1 verification

- Full `scripts/unix/check.sh`: fmt ok; tests 1988 passed, 0 failed, 31 ignored; CI clippy clean; `mct-eval` no regressions (accuracy 1.000, catalog ~12033 tokens). It also printed "improvements found"; the baseline was not rewritten (`--write-baseline` not run).
- Working tree before vs after `check.sh` (`git status --porcelain=v1 -uall` plus `git diff`): the only difference is this record, written while the check ran. `check.sh` modified no file. The untracked pre-existing transcript keeps the same SHA-256.
- After the check, one doc-comment wording fix in `tests/common/tokens.rs`; re-checked with `cargo fmt --all --check` and `cargo clippy -p mct-mcp-server --all-targets`.
- Re-detection: index refreshed by the background watcher (timestamp after the last edit; 975 files = 973 + 2 new). Duplicate groups: 192 (snapshot 193). `exact` groups for `uuid_like` (mct-index tests) and the eight-member `content_of` group are gone; `catalog_report.rs` has no group. Not reconciled group by group yet (B2).

## Block 2 — duplication: reconciliation and first analysis (in progress)

**Method.** `[CLI-before]` = `mct-cli` built from HEAD `15f8f74`, run with `call --tool find_duplicate_code` / `find_dead_code` (pages of 100, `max_tokens` 16000, following `next_offset`) over two disposable copies with their own indexes: a `git archive` of HEAD and a copy of the current working tree (tracked + untracked, transcript excluded). The same binary was used for both, so the detector is identical on both sides; the diff of this campaign touches no indexer or parser code. Groups are matched by id and, when an id changed, by overlapping `(path, symbol)` members. `[MCP]` (installed server) gave the same totals for the current tree (192 broad groups, 337 dead-code candidates).

**Reconciliation, `broad` (snapshot 193 at HEAD, 192 now).**

| Change | Group (HEAD id → now) | Explanation | Certainty |
|---|---|---|---|
| Removed | `bce8ee5d` 9× `uuid_like` | OPT-D05a | Certain: all nine members are the files edited |
| Removed + reappeared smaller | `d2845491` 9× `content_of`/`text` → `104ade14` `content.rs::content_of` + `contracts.rs::text` | OPT-D05c removed eight copies; the ninth member (`contracts.rs::text`) now pairs with the new shared helper | Certain by members |
| Removed + reappeared smaller | `466e2f99` 3× `approx_tokens`/`eval_tokens` → `0d4874bb` `mct-eval/src/lib.rs::approx_tokens` + `tests/common/tokens.rs::approx_tokens` | OPT-D02 removed `eval_tokens`; the remaining pair is the one commit `753dde3` kept on purpose (dependency direction) | Certain by members; reason for the pair taken from `753dde3`, `[GIT]` |
| Same id, shorter | `7def4708` `find_calls`/`find_callers` 36 → 28 lines | OPT-D01: the handlers still match as argument-forwarding wrappers | Certain |
| Other 189 groups | same id, same members, same line count | — | Certain |

Net: 193 − 3 + 2 = 192. No group was added by an unrelated edit. **Uncertain correspondences: none** for `broad`. `consistent`: 190 → 188 (the same three removals; `34f28a53` instead of `d2845491`, and `0d4874bb` appears as the kept pair). `exact`: 45 → 43 (`bce8ee5d`, `34f28a53` removed; nothing new). A group disappearing is not evidence of anything by itself; each removal above is tied to a reviewed edit.

**Dead code (snapshot 336 at HEAD, 337 now).** By `(path, name, kind)`: one new candidate, `RelationRequest` (struct, `server.rs`) from OPT-D01; nothing removed. `[I]` it is a false positive of the same kind as ERR-026: the struct is used as a struct literal (twice), a destructuring pattern and a parameter type, none of which MCT indexes as a reference. The positions are known from the code written for D01, not from a probe of the detector; it is added to ERR-026 as an eleventh example. (The keyed sets hold 327 vs 328 entries because several candidates share a path, name and kind.)

**Current population (`broad`, 192 groups).** Rust 173, Python 18, Lua 1 (the unsupported languages cannot appear). By file scope, from member paths only: 122 groups have every member in tests, examples or benchmarks; 47 span several crates; 23 sit inside one crate. `exact` (43): 27 / 13 / 3. This is a scope split, not a behavioral classification.

**Preliminary classification (pattern and member paths; bodies not read).** Used only to order the work, none is definitive.

| ID | Group(s) | Observed | Preliminary class | Next |
|---|---|---|---|---|
| OPT-D06 | `Walker` helpers: `push_symbol` ×15 (L19), `push_relation` ×14 (L17), `module_location` ×8 (L8), `new` ×6, `visit_children` ×5 | One method per parser crate with identical normalized shape | Same rule repeated across crates; a shared home would sit next to the `LanguageParser` boundary (protected) | Design note only, B4 |
| OPT-D07 | `open_indexed` ×10/×13/×5, `parse` ×14, `syntax_error_is_reported_not_panicked` ×13 in `mct-lang-*/tests` | Per-crate test helpers | Possibly deliberate isolation (each crate's test target is separate) `[I]` | Check CONTRIBUTING / rules.md before proposing |
| OPT-D03 | `mcp_chars` (L50, 271 tokens), `run_language`, `walkdir_files` between `mct-cli/examples/token_benchmark.rs` and `mct-lang-lua/examples/token_benchmark_lua.rs` | Largest exact-ish pair by size | Same rule or a fork of one example `[I]`; reason `[NV]` | Needs body comparison |
| OPT-D09 | `build_server`/`server_at` ×10 and the pairs listed in the inventory in `mct-mcp-server/tests` | Test setup | Same rule in test code | Needs body comparison |
| OPT-D08 | `queries.rs::find_symbol_scoped` (L239–259) vs `find_symbol_fts` (L349–371) | `[MCP]` `build_context_pack`: the two bodies differ only in the `FROM`/`WHERE` text (`symbols s … WHERE s.name = ?1` vs `symbols_fts … WHERE symbols_fts MATCH ?1`) and the bound value; the last eight lines (scope, `ORDER BY`, prepare, `query_map`, collect) are identical, as are the callees `push_scope` and `symbol_hit` | Same rule with a deliberately different head (search vs exact match) | Candidate: one private helper taking the SQL head and first bound value. Benefit is small (about 8 lines ×2); public signatures untouched. **Not approved, not implemented.** |
| OPT-D04 | calls/callers/references triples in `queries.rs` and `lib.rs` | Public `mct-index` API involved | Parallel by design until bodies are compared `[I]` | Needs body comparison |

**Dimensions of §6.6 covered for D08 only** (callers, tests, behavior of the two heads, public surface). The other candidates have only the detector evidence above.

## Block 2 (continued) and Block 3 — decisions after authorization

Authorization received: Grep and direct reads where MCT is not enough; implement bounded, behavior-preserving, verifiable improvements (D08 and others confirmed during the campaign); do not update the `mct-eval` baseline; do not stop other people's processes, reinstall shared binaries, change protected contracts, commit, push or open PRs.

| ID | Evidence read | Decision | Reason |
|---|---|---|---|
| OPT-D08 | `queries.rs`: `find_symbol_scoped`, `find_symbol_fts` **and** `find_symbol_like` ended with the same eight lines (scope, `ORDER BY`, prepare, `query_map`, collect) | **Implemented** | New private `symbol_hits_in_scope(conn, head, first, scope)`; the three functions keep their SQL head and bound value. Public signatures unchanged; `queries.rs` 37 insertions, 36 deletions (about even in lines, one place for the tail instead of three). `cargo test -p mct-index` 0 failures (`[CLI]` lib 198 tests + integration), CI clippy on `mct-index --all-targets` clean |
| OPT-D09 | `build_server_at` body (registry, in-memory index, reindex, `MctServer::new`) read in 9 test files; identical text | **Implemented** | New `crates/mct-mcp-server/tests/common/server.rs` (`build_server_at`), included with `#[path]` like `tokens.rs`/`content.rs`; 4 files keep their old local name through `use … as`. Variants that differ (`server_with`, embedder, fixture-root wrappers, `catalog.rs`) left alone. Removed 5 now-unused `mct_index` imports. `cargo test -p mct-mcp-server` 434 passed, 0 failed; CI clippy on tests and examples clean |
| OPT-D04 | `queries.rs`: `find_calls_scoped`/`find_callers_scoped`/`find_references_scoped`/`find_dependencies_scoped` | **Kept** | They already share `query_relations`; each is a thin wrapper that only supplies a SQL condition. Nothing left to extract; the public API stays |
| OPT-D03 | `token_benchmark.rs` vs `token_benchmark_lua.rs` | **Kept** | The Lua example documents the copy as deliberate: Lua is not a `mct-cli` dependency of that example. Side finding: that header says `mct-lang-lua` is "not wired into" the production registry, while the crate's own `Cargo.toml` description says it is registered in production. One of the two is stale (L-item, not changed: the example's text and the registry state were not both checked) |
| OPT-D06 | `push_symbol` read in `mct-lang-go/java/csharp/kotlin` | **Not implemented, proposal** | Byte-identical 18-line bodies, and 15 crates share them. A shared home means a new type or trait method visible to every `mct-lang-*` crate, next to the protected `LanguageParser` boundary, and a 15-crate edit. Beyond the "bounded" limit; recorded as a design proposal for the user (B4) |
| OPT-D07 | not read | Open | Per-crate test helpers; not compared in this pass |

**Dimensions of §6.6** were compared for D08 (callers, tests, behavior of the three heads, public surface) and D09 (setup, errors, cleanup: none, uniqueness: not applicable). D03/D04 by reading the code; D06 on four of fifteen crates.

## Block 3 — dead code (Rust verified by name occurrence; JS and others sampled)

Method: for every Rust candidate (120 of 337: `struct`, `enum`, `type_alias`, `function` under `crates/**/*.rs`) `grep -w` for the name over all of `crates/` excluding the definition line. `[Grep]` Result: **119 have other occurrences, 1 has none** (`find_dead_code_toon`). Name occurrence is not proof of use (same names in other scopes), but together with ERR-025/ERR-026 it fits the known detector limits. Private items cannot be dead without a `dead_code` warning, and CI clippy with `-D warnings` is clean.

- `find_dead_code_toon` (`mct-mcp-server/src/format.rs:1546`): no use in the workspace; the server renders TOON for `find_dead_code` another way and its test (`toon_output.rs`) goes through `MctServer::find_dead_code`. **Kept**: `pub fn` in `pub mod format` of a library crate without `publish = false`, so removal changes a public Rust API; it also appears in `docs/design/2026-10-22-shared-review-decisions-design.md`. Proposal for the user (OPT-M01).
- `filterPanel` (`mct-cli/src/web/app/components.js`, exported, 38 lines): no reference anywhere in the repository except its definition (`grep -rI` excluding `target`, `.git`, `node_modules`, `.mct-index`); the file is served as a static asset by name (`assets.rs`). **Removed** (OPT-H01). `node --check` passes; UI not exercised in a browser (browser use was not authorized), so a dynamic lookup by string is excluded only by the grep.
- 15 other `web/app` JS candidates have 1 to 17 other name occurrences each; not removed.
- `review-dist/review.js` (89), `benchmarks/token-twins-labels` (72 Python), `website/` (22): generated bundle, labelled fixtures and framework entry points; **Kept**, not re-examined one by one.

## Other findings from the scripts, CI and dependencies pass

- OPT-G01 **Fixed**: `scripts/README.md` said `branches-status`, `publish-branch` and `release` are the Unix-only scripts. `ls` shows seven Unix scripts without a `.ps1` twin: those three plus `preview-branches`, `reindex-delay-smoke`, `review-compat` and `sync-skills`. The sentence now lists all seven (documentation only; the missing ports are not proposed here).
- CI (`.github/workflows/ci.yml`): the `cargo audit || true` step is deliberate and commented; the second step gates on high/critical severity. Actions are pinned by major tag (`@v4`, `@v2`), not by commit; a hardening option, not changed.
- `cargo-audit` exists locally, but running it fetches the advisory database from the network. **Not run.** J (dependencies) and K (security) are therefore not assessed beyond reading the workflow.

## Inventory (not yet worked)

| ID | Location | Evidence | Status | Next action |
|---|---|---|---|---|
| OPT-D03 | `crates/mct-cli/examples/token_benchmark.rs` vs `crates/mct-lang-lua/examples/token_benchmark_lua.rs` | `[MCP]` four shared groups (`mcp_chars`, `run_language`, `grep_then_read_chars`, `Query`); reason for the copy `[NV]` | Pending | B2: compare and find the reason |
| OPT-D04 | `crates/mct-index/src/queries.rs` (`find_*_scoped`, `find_*_page`), `lib.rs` (`find_*_bfs_scoped`) | `[MCP]` calls/callers/references triples; public `mct-index` API | Pending | B2 |
| OPT-D06 | `Walker` helpers across `mct-lang-*` (`push_symbol` ×15, `push_relation` ×14, `new` ×6, `visit_children` ×5, `module_location` ×8, `module_name_for` ×5) | `[MCP]` groups listed, bodies not inspected; target crate and design `[NV]` | Pending | B2/B4: design note only |
| OPT-D07 | per-language test helpers (`parse`, `open_indexed`, `syntax_error_*`) | `[I]` possibly deliberate per-crate isolation | Pending | B2: check CONTRIBUTING |
| OPT-D08 | `queries.rs` `find_symbol_scoped` vs `find_symbol_fts` | `[MCP]` listed, not compared | Pending | B2 |
| OPT-D09 | other test helper pairs found by `exact` (`build_server_at`, `server_with`, `bag_of_words`, `reindex`, `text`, `request`, `server`) in `mct-mcp-server/tests` | `[MCP]` exact-text candidates, not compared | Pending | B2/B9 |
| OPT-M01 | `crates/mct-mcp-server/src/format.rs` `find_dead_code_toon` (`pub`) | `[MCP]` 0 refs; `[GREP-auth]` no occurrence in `crates/**/*.rs`; external consumers, features, other directories `[NV]` | Inconclusive | B3 |
| OPT-M02 | five `format.rs` functions and ten Rust types | `[GREP-auth]` used (ERR-025, ERR-026) | Keep | — |
| OPT-M03 | `review-dist/review.js` (~90), `benchmarks/token-twins-labels` (77), Next.js conventions in `website/` (22) | `[I]` generated, fixtures, framework entry points | Keep | B3: confirm |
| OPT-M04 | web UI JS closures, `web-ui` TS interfaces | `[I]` callbacks and type-only use | Inconclusive | B3 |
| OPT-L01 | `docs/01-architecture/dashboard-review-acceptance.md:175` | `[MCP]` says MCP tools ignore review decisions; tools now expose `include_reviewed` | Pending | B13: read context |
| OPT-L02 | `rules.md` Architecture tool list | `[FS]` omits `find_duplicate_code`, `discover_tool_categories`, `get_tool_schema`, `select_tools` | Confirmed | B13 |
| OPT-L03 | `rules.md` Architecture crate list | `[MCP]` no `mct-obsidian` | Confirmed | B13 |

## Open items from B0

- Review store: `CANONICAL_FILE = "review-decisions.toon"` (`crates/mct-index/src/reviews/store.rs`). No such file under `.mct-index/` in this worktree and no decision observed; the store directory and the meaning of `review_store: ok` / `suppressed` are still to be confirmed.
- Excluded files: `mct-cli status` failed (ERR-024); no `.mctignore`; `vendor/` not in the tree. To confirm.
- Skills: project-local skill directories match the session's skill list; whether each appears in the `/` menu as the local copy is unverified. `safe-refactor` and `investigate-first` are not project-local.
- `scripts/unix/check.sh` formatting step runs `cargo fmt --all --check` (check only, `[MCP]` source of `step_fmt`).

## Pre-existing work

- An untracked session transcript at the repository root: preserved, never staged, not read.
- Commits `753dde3` (shared `approx_tokens`) and `15f8f74` (error records) were already on the remote branch at the start.

## B5 and B9 results (probes and ignored tests)

- **B5, T01 (ERR-025) delimited** with a disposable crate (eight callees, one call position each; `find_callers` through `mct-cli` built from HEAD): calls as plain statements and in `let` initializers are indexed; calls inside any macro invocation (`format!`, `vec![]`, `assert_eq!`, `println!`, a user `macro_rules!`) are not. The scope is wider than the five original sites. Same file, no `use`; cross-file with imports not probed (`symbol-coverage.md` line 731 says imports count as uses).
- **B5, T02 (ERR-026) delimited** the same way: no reference for a type used as parameter, return type, field type, generic argument, struct literal, `let` annotation, `impl` or trait `impl` target; one reference for a path expression. All positions, not a subset. Same-file caveat as above. Status left OPEN: documenting it as a known limit or fixing the relation model is the user's call (protected area).
- **B9, ignored tests:** `grep -rn '#\[ignore'` shows 18 with a reason string. Four record known bugs that match findings of this campaign: Rust macro-token calls (`mct-lang-rust/tests/parse.rs:235`, same defect as ERR-025), Java `this(…)`/`super(…)`/method references (`mct-lang-java/tests/corpus.rs:233`), Kotlin companion objects and `typealias` (`mct-lang-kotlin/tests/corpus.rs:217`) and bash `coproc` (#79, `mct-lang-bash/tests/corpus.rs:235`). Three say "not filed yet"; filing issues needs your authorization. The rest need an external binary, the luarocks CLI or the embedding model download. I did not count the 31 reported by `check.sh` against these 18 (the other 13 come from `#[ignore]` without a reason or doc-comment mentions; not itemized).
- **B13:** rules.md's crate list has no `mct-obsidian` entry (`grep` finds no mention) and its MCP tool list omits several tools (OPT-L02/L03; crate directories listed with `ls crates`). Not edited: rules.md governs the work and changing it is your decision. OPT-L01 closed: the document carries a "Later implementation note" that supersedes the line.
- **D07 closed as Kept:** the per-crate `open_indexed` helpers differ in parser type, fixture and assertions (two Go variants read), so sharing them would need parameters that cost more clarity than they save; `parse` and `syntax_error_*` follow the same shape.

- **B4, dependency direction (`[Cargo.toml]` `[dependencies]` of each crate, read with `awk`/`grep`):** `mct-core` depends on no workspace crate; `mct-index` and `mct-tree-sitter` only on `mct-core`; `mct-languages` on `mct-core` and the 17 `mct-lang-*`; no `mct-lang-*` has `mct-index` as a normal dependency (it appears in the 17 `[dev-dependencies]`); `mct-obsidian` on `mct-index`; `mct-mcp-server` on core, index, languages and obsidian; `mct-cli` adds `mct-mcp-server`; `mct-eval` on index and mcp-server; `mct-corpus` on core and index. No cycle seen and the layering matches rules.md's description. `[NV]` features, build scripts and `cargo tree` were not examined. Consequence for D06: a shared `Walker` base would have to live in `mct-core` or `mct-tree-sitter`, the only crates every parser already depends on.

- **B6, catalog vs docs (`[MCP]` `discover_tool_categories`, CHANGELOG):** the live catalog lists 26 tools in five groups; rules.md lists 18 (the L02 omissions are `find_duplicate_code`, `discover_tool_categories`, `get_tool_schema`, plus the two deprecated aliases and the optional `select_tools`). The deprecated aliases `search_symbols`/`hybrid_search` say "removed next version"; CHANGELOG 0.3.0 says they stay "for one version", and the workspace is at 0.3.0, so the two statements agree. CHANGELOG puts the alias cost at about 1,900 catalog tokens; removing them is a published-signature change and stays the user's decision. No tool signature, default or error text was compared field by field, so B6 is only partially covered.
- **B8, web (source read, no browser, no requests to running instances this turn):** `crates/mct-cli/src/web/http.rs` accepts only `GET`/`HEAD`/`POST`, rejects any `Host` other than `127.0.0.1:<port>` or `localhost:<port>` with 421, checks `Origin` and `Sec-Fetch-Site` and sends a strict CSP (`default-src 'none'`, `frame-ancestors 'none'`), `X-Frame-Options: DENY`, `nosniff`, `no-store`; a unit test (`host_origin_and_body_rules_are_enforced_without_a_url_key`) covers host and origin cases. The API has 15 read routes plus review and `.mctignore` writes. Persistence, concurrency and `needs-reevaluation` behavior (disposable instance) were not exercised.
- **B10, performance:** inventory only: examples `budget_benchmark`, `cache_benchmark`, `format_benchmark`, `incremental_benchmark`, `token_benchmark(_lua)`; `benchmarks/*.md` and `docs/03-performance`. No hypothesis from this campaign needs a measurement: D08/D09/D01 keep the same SQL text, the same `prepare_cached` use and the same call sequence, and no performance benefit is claimed for them. Nothing was measured.

## Next step

State: B1 verified; D08, D09 and OPT-H01/G01 implemented and tested individually (see above). Full `scripts/unix/check.sh` on the whole tree after these changes: fmt ok; 1988 passed, 0 failed, 31 ignored; CI clippy no warnings; mct-eval no regressions (accuracy 1.000, catalog ~12033 tokens), improvements reported and baseline not rewritten. Git status and full diff were identical before and after the run. Net effect of the whole campaign so far: tracked code and tests +215/-369 lines, new shared test helpers +57, about 97 lines fewer.

Proposals needing the user's decision (not implemented, outside the stated limits):

1. OPT-D06: a shared `Walker` base for `push_symbol`/`push_relation` across 15 `mct-lang-*` crates (touches the protected parser boundary).
2. OPT-M01: remove `format::find_dead_code_toon` (public API of a library crate).
3. `mct-eval` baseline: the full check reported improvements; **not rewritten** per instruction. Differences to review are in "Block 1 verification".

Not done or partial: B6 (field-by-field contract comparison), B8 (disposable-instance behavior), B10 (no measurements), B11/B12 (needs advisory fetch), `mct-cli status` retry (ERR-024), definitive classification of the remaining ~150 duplicate groups.

## Phase 2 on `mct-lang-*` (branch `refactor/lang-shared-ts-helpers`)

- **Done:** `module_location`, `find_child` and `find_named_child` moved to `mct-tree-sitter` (13 crates, about 107 lines fewer). `find_child` had two semantics (all children vs named children), kept as two helpers. `mct-lang-kotlin::module_location(source)` is a different function and stays.
- **Kept, documented in `docs/02-crates/parsers/overview.md`:** `push_symbol`/`push_relation` (a forwarding wrapper saves about 4 lines per crate), `unquote`, `visit_children`, `new`, per-crate test helpers.
- **Docs:** `rules.md` architecture block now lists `mct-tree-sitter`, `mct-obsidian`, `mct-corpus`, the full tool set and the CLI subcommands (L02, L03); ADR-015 marked partly superseded; ADR number gaps noted in the index.
- **Check:** `scripts/unix/check.sh`: fmt ok, 1990 passed, 0 failed, 31 ignored, clippy none, eval no regressions. Test `get_project_overview_truncates_a_real_crates_module_and_reports_the_overflow` counted `mct-lang-go/src/lib.rs` symbols and was updated from 10 to 9.
