# Checkpoint: TASK-207-C

Task: `mct-cli duplicates` subcommand, TOON report, ADR draft (issue #207, Stage 6 and a draft of Stage 9).
Branch: `claude/207-c-cli-report`. Base: `bfbf4b3eb45bd97031f1818b2a9474335c2b01f2`.
Latest commit: see `git log` on the branch (feat(cli), test(cli), docs(adr), then this checkpoint).

## Done
- `Command::Duplicates { min_lines, min_tokens }` in `crates/mct-cli/src/main.rs`, with doc comment and `after_help` examples. Defaults come from `DEFAULT_MIN_LINES` / `DEFAULT_MIN_TOKENS`; both flags use a clap `value_parser` that rejects 0 and non-integers (same style as `--reindex-delay`).
- `crates/mct-cli/src/duplicates.rs`: handler `run`, local TOON renderer `render_toon`, `parse_threshold`, 10 unit tests on hand-built reports.
- `crates/mct-cli/tests/duplicates.rs`: 9 end-to-end tests running the binary, valid on the placeholder analysis (zero groups, complete).
- `docs/01-architecture/adrs/007-duplicate-function-report.md`: ADR draft, Status "Proposed (spike, issue #207)", with an "Evaluation (Stage 8)" section of `TODO(coordinator)` placeholders. No numbers invented.

## Not done
- Grouping end-to-end tests (need the real analysis and token sources from the other workers; the coordinator adds them).
- Stage 8 measurements and manual review (ADR placeholders).

## Changed paths
- `crates/mct-cli/src/main.rs` (`mod duplicates;`, the variant, dispatch, unreachable arm)
- `crates/mct-cli/src/duplicates.rs` (new)
- `crates/mct-cli/tests/duplicates.rs` (new)
- `docs/01-architecture/adrs/007-duplicate-function-report.md` (new)
- `project-management/checkpoints/TASK-207-C.md` (new)
No Cargo.toml or Cargo.lock change.

## Checks (run with `--manifest-path` on the worktree)
- `cargo fmt --all --check`: clean.
- `cargo clippy -p mct-cli --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`: no warnings.
- `cargo test -p mct-cli`: 56 passed, 0 failed (unit 36 incl. 10 new in `duplicates::tests`; `tests/call.rs` 11; `tests/duplicates.rs` 9).

## Behavior
- `mct-cli --root <project> duplicates [--min-lines N] [--min-tokens N]`. One root only (the existing "exactly one --root" error otherwise).
- Mirrors `status`/`dead-code` (read-only, no reindex), with one deliberate difference: it does not call `Index::open` on a project without `.mct-index/index.sqlite3`, because `Index::open` creates the database and an empty index would give a misleading "zero groups, complete" report. Errors (non-zero exit, nothing on stdout):
  - root not a directory: `project root <p> is not a directory`
  - no database: `no index found at <p>; run `mct-cli --root <root> init` first`
  - unopenable/corrupt database: `cannot open the index at <p>` (with the cause)
  - query failure: `cannot query the index for duplicate candidates`
- Analysis never runs from `reindex`.

## TOON layout
Own renderer (`render_toon`), not `mct_mcp_server::toon::encode_table`. That helper is public but only quotes comma, quote, backslash and newline, and it has no nested blocks. Source excerpts need quoting for leading indentation, colons and brackets. Strings are quoted when empty, with leading/trailing whitespace, a comma, colon, quote, backslash, bracket or brace, a control character, a leading hyphen, or when they read as a number, `true`, `false` or `null`. Escapes: `\\`, `\"`, `\n`, `\r`, `\t`; other control characters become U+FFFD. Output follows the report's order; no hash iteration.

Zero groups, complete:

```
notice: Groups below are CANDIDATE duplicates for human review. Identical normalized structure does not prove that the functions are redundant or behave the same.
status: complete
options:
  min_lines: 6
  min_tokens: 40
supported_languages[0]:
unsupported_languages[0]:
generated_code: generated code is excluded only by the project's configured exclusion rules; no generated-code marker is detected
candidates_analyzed: 0
below_threshold: 0
total_groups: 0
total_skipped: 0
skipped_by_reason:
  read-error: 0
  parse-error: 0
  stale-location: 0
  invalid-range: 0
groups[0]:
skipped[0]{path,range,symbol,language,reason,detail}:
```

With a group (tail of the report; the excerpt is one quoted cell, lines joined by `\n`):

```
groups[1]:
  - index: 1
    language: rust
    member_count: 2
    line_count: 8
    token_count: 52
    evidence: "members share an identical normalized token sequence (identifiers -> $ID, literals -> $LIT); inspect src/a.rs:10-20, src/b.rs:3-13"
    members[2]{path,range,symbol,kind,language,lines,tokens,excerpt}:
      src/a.rs,10-20,load,function,rust,8,52,"    fn load(name: &str, flag: bool) -> Result<u8, String> {\n        let x = \"a,b\";\n    }"
      src/b.rs,3-13,read,function,rust,8,52,"fn read() {"
skipped[0]{path,range,symbol,language,reason,detail}:
```

A partial report adds `status: partial` and a `partial_reason` line; skipped rows are `path,start-end,symbol,language,reason,detail`, with `start-?` when the end line is unknown.

## Decisions
- `evidence` is one field per group listing every `path:start-end` range; `members` carries the same ranges as `start-end` per row.
- Skip reasons are listed from a fixed array (read-error, parse-error, stale-location, invalid-range); a test pins the order.
- `ExcludeSet` is built from `read_ignore_file` like the other commands, since `Index::open` requires one.

## Limitations
- Not exercised against real groupings or real token sources (placeholders in this base).
- The excerpt cell can be long when lines are near the 120-character cap (3 lines at most).
- The ADR describes the design as specified for the other workers' tasks; the coordinator must check it against what they actually built (names, which languages ship) before moving it from Proposed.

## Next action
Coordinator: integrate with TASK-207-A/B, add grouping end-to-end tests, run Stage 8, fill the ADR's `TODO(coordinator)` items, then update docs (README command list) outside this task's scope.
