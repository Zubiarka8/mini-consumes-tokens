# Rust-first integrity completion and functional coverage

## Scope and provenance

2026-10-27, `feature/dead-code-editor`, base
`b61cc7ce971fdc46a61259af95af7fac823ae352`, with pre-existing local work.
The starting tree had 73 modified tracked files and 32 untracked status entries.
This report covers authorized Batch 1 (D1, D7, D8) and a read-only audit of
remaining Rust and MCP/web functionality. It does not approve later batches.
No schema, dependency, identity, review fingerprint or installed binary changed.
All execution used temporary projects, disposable databases or read-only source
fixtures. Production database contents were not inspected. No browser acceptance,
cross-platform validation, deployment, commit or publication was performed.

## Batch 1 contract

- **D1:** `Index::source_matches_index` compares the indexed Git blob hash with
  the exact bytes already read by the MCP source reader. Each consumer holds
  its SQLite read snapshot across lookup and validation. Search snippets and
  context-pack source/signatures are omitted when unverifiable; related
  signatures use the same guard. File skeletons return an explicit MCP
  invalid-parameters error requesting reindexing for stale/unindexed source.
  Indexed metadata remains available and is not represented as current source.
  Existing per-request file maps avoid repeated source reads. No persistent
  source cache was introduced. `MCT_MAX_FILE_BYTES` and its 16 MiB default remain
  unchanged; the separate 2 MiB review reader is not used for MCP retrieval.
- **D7:** binary classification removes only the same path's `syntax_error`
  issue inside the existing reindex transaction. Unrelated errors remain.
  The previously ignored regression is enabled and checks both forced and
  path-based indexing against a freshly built index.
- **D8:** the concurrency regression first proves `src/a0.rs`, `a0`, L1-L9 are
  visible. During indexing, after publication, after rollback and after recovery,
  it checks the reviewed path/name are absent and `src/c0.rs`, `c0`, L1-L11 remain
  visible. Dashboard records and duplicate-output checks remain intact. Hash
  absence is no longer treated as evidence about dead-code text output.

The source guard verifies a byte buffer against one committed index snapshot;
it does not freeze the working filesystem after reading. It uses the existing
indexed hash contract, not a new content-identity algorithm.

## Rust feature matrix

Statuses describe the tested scope, not universal Rust/compiler equivalence.

| Capability | Status | Direct evidence / limitation |
| --- | --- | --- |
| `.rs` discovery and initial indexing | Verified working | `mct-lang-rust/tests/index_integration.rs`; registry and shared indexer |
| Syntax errors | Verified working | `tests/parse.rs`, corpus malformed fixtures; syntax errors reject the file, not partial AST recovery |
| Partial extraction from malformed Rust | Missing | Parser returns `ParseError::Syntax` when the root has errors; intentional current contract |
| Public/private functions, exact spans | Verified working | `tests/parse.rs`, `tests/corpus.rs`, duplicate visibility fixtures |
| Structs, enums, traits, constants, statics, aliases | Verified working | `types_traits_and_aliases_are_extracted_with_kinds` |
| Inherent/trait impl methods and associated items | Verified working | `methods_hang_off_their_impl_type_including_generic_ones`, implements tests |
| Async/unsafe functions | Partially implemented | Shared `function_item` extraction; no dedicated complete modifier matrix verified here |
| Modules, imports and cross-file resolution | Verified working | `tests/depgraph.rs`, corpus and index integration; bounded lexical evidence |
| Re-export chains and compiler module semantics | Partially implemented | `use` extraction exists; exhaustive alias/glob/re-export resolution not established |
| Calls, references, callers and ambiguity | Partially implemented | Rust parse/depgraph tests and index relation-resolution tests; no compiler type inference |
| Free-function references inside macros | Verified working | `tests/parse.rs` and `tests/dead_code.rs`; references, not proven calls |
| Method references/calls inside macro token trees | Broken | Explicitly executed ignored D3 regression fails: no relations, expected lines 4 and 5 |
| Macro expansion / generated methods | Missing | No compiler expansion; token-tree walking does not prove generated definitions |
| MCP snippets, signatures and skeletons | Verified working | Six new `tests/source_revision.rs` tests; unchanged, stale, removed, binary and reindexed source |
| Incremental invalidation and publication | Verified working | Incremental and generation tests; D7 full/path regression |
| Dead-code candidates | Partially implemented | Shared heuristic, not exhaustive liveness; methods deliberately excluded |
| Same-name dead-code discrimination | Broken | Disposable `used::helper` / `unused::helper` reproduction below |
| Exact/structural duplicate candidates | Verified working | Rust CLI duplicate corpus, shared normalization and MCP duplicate tests |
| Arbitrary near-duplicate similarity | Missing | No Type-3 similarity guarantee; normalized equality and optional repeated blocks are different features |
| Review persistence and invalidation | Verified working | SQLite dashboard decisions, identity/reindex tests and independent stdio integration |
| Rendered dashboard/editor workflow | Not yet verified | Backend and JavaScript VM tests pass; no real browser exercised in this batch |
| MCP initialization under writer contention | Broken | D2 remains outside Batch 1; synchronous open/reindex precedes stdio service |

## Duplicate analysis and MCP/web parity

`mct-lang-rust/src/tokens.rs` supplies callable boundaries and syntactic tokens;
`mct-index/src/duplicates/analysis.rs` groups equal normalized sequences per
language. Broad normalization replaces identifiers/literals; consistent and
exact policies retain stronger distinctions. Equality is not behavioral
equivalence. Visibility is ignored intentionally. Methods and functions are
candidates; tokens outside callable boundaries are not merged into them.
The CLI corpus verifies renamed identifiers, literal differences, unrelated
structures, exact names, method boundaries, stale files, deterministic ordering,
pagination and opt-in partial blocks. MCP tests verify membership changes,
unavailable/ambiguous sources, review invalidation and bounded output.

| Capability | MCP surface | Web surface | Parity assessment |
| --- | --- | --- | --- |
| Symbols and paths/ranges | Symbol/list/context tools | Navigation and rendered symbol views; summary API | Shared indexed facts; rendered usability not verified |
| Dead-code detection | `find_dead_code` | `api/dead`, rendered views | Shared candidate detector and review semantics; inherits same-name limitation |
| Duplicate groups | `find_duplicate_code` | `api/duplicates`, rendered detail | Shared detector and identity/review domain for default whole-callable analysis |
| Matching policies | Explicit MCP arguments | Existing dashboard policy | Not a promise of arbitrary per-request policy parity |
| Reviewed findings | Actionable by default, explicit reviewed duplicate inclusion | Current/history/comment/copy workflows | Valid false-positive/ignored suppress; stale/ambiguous remain actionable |
| Source previews | Verified source, configurable read limit | Verified hash, 2 MiB reader and bounded source pages | Safety contract agrees; limits and response shapes intentionally differ |
| Pagination | Offsets within a generation; response budgets | Snapshot pin/conflict handling | Different contracts; neither implies a shared cross-database snapshot |
| Review writes/comments | Findings read-only | Dashboard SQLite API/store | Presentation responsibilities intentionally differ |
| Per-copy labels/backups | No equivalent finding controls | Dashboard and CLI | Labels do not suppress a group; no missing MCP endpoint inferred |
| Generic graph/context features | Dedicated tools | No matching generic API established | Do not invent web endpoints; product scope needs approval |

The index and dashboard remain separate SQLite databases. Identity and
fingerprint validation prevents unverifiable review suppression; this is not an
atomic cross-database snapshot. Web source checking already uses shared
`review_keys::indexed_text`; MCP now uses shared index revision validation while
preserving its larger configurable limit. No AST/detector logic was duplicated.

## Remaining confirmed issues and minimal backlog

1. **P1 D3, Rust macro methods:** `mct-lang-rust/src/lib.rs` skips token-tree
   identifiers following `.`/`::`. Preserve receiver uncertainty when recording
   evidence; do not resolve a method to a same-named free function. Acceptance:
   the existing ignored regression and ambiguity/shadowing tests pass. Medium
   complexity; separate Batch 2 authorization.
2. **P1 dead-code precision:** create `src/main.rs` with `mod used; mod unused;`
   and `fn main() { used::helper(); }`; both modules define
   `pub fn helper() -> i64 { 41 }`. CLI `find_calls` correctly targets
   `src/used.rs:1`, but `find_dead_code` returns no candidates: the uncalled
   `src/unused.rs:1` is hidden by the name/language reference set in
   `mct-index/src/dead_code.rs`. This is a detector false negative, not a saved
   review being transferred. Evaluate resolved symbol identities while retaining
   conservative handling of ambiguous/unresolved references. Add a labeled Rust
   corpus and verify MCP/web agreement before changing the heuristic. Medium/high
   complexity; separate detector batch authorization.
3. **P1 D2 availability:** design cold/current/locked/generating/recovered readiness
   in `mct-mcp-server/src/main.rs` and background orchestration. Do not report a
   partial generation ready. Real stdio tests required. High complexity; separate
   lifecycle authorization, no installed service changes.
4. **P2 rendered UX acceptance:** temporary project, actual browser, source/review
   conflicts and refresh/pagination. Current VM tests are not rendering evidence.
   No new dependency or Chrome use authorized by this batch. Medium complexity.
5. **P2 Rust completeness/performance:** dedicated re-export/cfg/async/unsafe/macro
   corpus, labeled accuracy and representative Rust benchmarks, including SQL and
   memory where measurable. Profile before proposing indexes/caches. Medium/high
   complexity; no optimization authorized by this report.

No additional production behavior beyond D1 and D7 was changed.

## Validation evidence

The first valid D1 reproduction failed with a context pack labeling
`pub fn unrelated()` as `alpha`, and a related `beta` signature containing
`pub fn different()`. D7 failed with the stale `src/util.fake` diagnostic versus
a fresh index without it. Both are passing after the corrections. Initial new
fixture errors (wrong context argument and assumed ranges) were corrected before
production changes and are not evidence of product defects.

| Executed command | Final result |
| --- | --- |
| `cargo test --locked -p mct-index -p mct-cli -p mct-mcp-server -p mct-lang-rust` | 1,193 passed, 0 failed, 7 ignored, 88 suite summaries including doc-tests |
| `cargo test --locked -p mct-mcp-server --test source_revision` | 6 passed |
| `cargo test --locked -p mct-index --test incremental` | 22 passed; also covered in the final package run |
| `cargo test --locked -p mct-mcp-server --test query_cache` | 22 passed after correcting the previous fresh-source expectation |
| `cargo test --locked -p mct-cli --bin mct-cli web::tests::decisions::review_writes_during_reindex_are_visible_to_mcp_and_survive_failed_publication -- --exact` | 5 repetitions, 1 passed each; finite evidence, not universal race freedom |
| `cargo test --locked -p mct-lang-rust --test parse a_method_call_inside_a_macro_token_tree_leaves_a_relation -- --ignored --exact` | 0 passed, 1 failed: known D3, outside Batch 1 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` | PASS; focused server Clippy also passed after the cache-test change |
| `cargo fmt --all -- --check` / `git diff --check` | PASS |
| `node --test crates/mct-cli/tests/dashboard_spa.cjs crates/mct-cli/tests/dashboard_copy_labels.cjs` | 43 passed, 0 failed; VM tests, not a browser run |

No optional semantic-model download or all-features claim is made. No frontend
assets changed; no browser build or native rendering acceptance is claimed.

The same 10-iteration evaluation suites retained accuracy 1.0, success 1.0 and
estimated response tokens 6,845 (33 general cases) / 1,522 (5 duplicate cases),
with catalog estimate 11,969. General-suite MRR remained 0.916667. Every case's
output is byte-for-byte identical to the preceding audit's saved evaluation.
These small fixture scores are not global Rust precision/recall.

The final evaluation was rerun after this batch's compilation/testing finished,
using `cargo run --locked -p mct-eval -- --iterations 10` and the same command
with `--suite crates/mct-eval/suite-duplicates.json` (external JSON report paths
omitted here). Initial contended measurements are not the final latency sample.

| Debug fixture suite | Previous audit p50 / p95 ms | Final p50 / p95 ms | Previous / final index-build ms |
| --- | --- | --- | --- |
| General, 33 cases | 0.778 / 3.069 | 1.192 / 4.637 | 40.504 / 74.622 |
| Duplicates, 5 cases | 1.147 / 3.716 | 1.748 / 2.709 | 23.717 / 29.167 |

These are small historical-vs-current samples with uncontrolled host load, not
an isolated before/after benchmark. They do not establish a causal performance
regression or improvement. The new guard adds a point lookup on the existing
unique path index and hashes the already-read buffer once per distinct source
file in a request. No new scan of every indexed file or new cache was added.
SQL costs, CPU profiles and peak RSS remain unverified.

Batch 1 completion does not establish full Rust functional acceptance. D3,
same-name liveness precision, readiness and real-browser acceptance remain open.
