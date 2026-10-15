# Checkpoint: TASK-207-B (duplicate analysis)

- Task: implement `analysis::run` behind `find_duplicate_groups` (issue #207, Stages 3, 4, 5, 7).
- Branch: `claude/207-b-duplicate-analysis`; base `bfbf4b3`; latest code commit `4ac8706`.
- Status: completed.

## Done
- `crates/mct-index/src/duplicates/analysis.rs` (+ `analysis/file.rs`, `analysis/fingerprint.rs`): candidates, per-file read/stale/tokenize, normalization, exact grouping, ordering, counters.
- `crates/mct-index/tests/duplicates.rs`: 26 tests on toy-language test doubles (issue cases 1-13; case 14 via `.mctignore`).
- Not done: nothing in scope. The frozen contract in `duplicates.rs` is untouched.

## Changed paths
`crates/mct-index/src/duplicates/analysis.rs`, `crates/mct-index/src/duplicates/analysis/{file,fingerprint}.rs`, `crates/mct-index/tests/duplicates.rs`, this file.

## Checks (via `--manifest-path`)
- `cargo fmt --all --check`: clean.
- `cargo clippy -p mct-index --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`: clean.
- `cargo test -p mct-index`: 293 passed, 0 failed, 2 ignored (82 unit, 26 in `tests/duplicates.rs`, rest existing suites). `Cargo.lock` unchanged.

## Decisions where the spec left a choice
- Candidates are listed per indexed file path (`file_hashes` + `queries::list_symbols(.., is_file = true)`), not `Index::list_symbols_all`, which enumerates the root on disk and would silently lose files deleted or replaced since indexing (and root dot-directories).
- Excluded paths (`Index::exclude.is_excluded`) are dropped silently, including rows left over after rules changed.
- Stale check: git blob hash (`Oid::hash_object`) of the bytes read, compared with `files.content_hash`, exactly as the indexer. A missing stored hash is `StaleLocation`.
- Check order: path inside root (else `ReadError`) -> read (NotFound = `StaleLocation`, other I/O or over-limit = `ReadError`) -> hash (mismatch = `StaleLocation`) -> UTF-8 (`ReadError`, only reachable with a foreign hash) -> tokenize (`Err` = `ParseError`). So a file that changed into non-UTF-8 is stale.
- Per candidate: missing end, start 0, end < start, end > file line count (`str::lines`) = `InvalidRange`; overlap with any `error_lines` range (normalized if inverted) = `ParseError`. File-level failures take precedence over range checks.
- `line_count` counts every line a multi-line token spans. Tokens are taken as `line >= start && end_line <= end` (sorted copy plus binary search). Nested functions include inner tokens in the outer one.
- A candidate with zero tokens counts as below threshold whatever the thresholds.
- Keyword and Punctuation keep their text and are kept distinct from each other; Identifier and Literal collapse. Sequences are interned `Vec<usize>` used as the exact hash-map key (no hash-only equality).
- Group order: `line_count * members` desc, then first member (path, start, end), then language, then token count.
- `skipped.detail` is a fixed message, the I/O error text or `syntax error at lines A-B`; never file contents.
- `detail` of a symlink/`..` escape is `ReadError`, never a query error.

## Limitations
- The size-limit read error (`MCT_MAX_FILE_BYTES`) is not tested: it needs a process-wide env var, racy across parallel tests.
- The `..`/absolute stored-path guard is only reachable with a tampered database; covered indirectly by the symlink-escape test (unix only).
- Whole-file tokenization is held for one file at a time; all candidates' normalized sequences are held until grouping (memory grows with total candidate tokens).
- Generated code: no marker in the repository; only configured exclusion applies.

## Next action
Coordinator integrates with the DUP-A/DUP-C branches; CLI rendering consumes `DuplicateReport`.
