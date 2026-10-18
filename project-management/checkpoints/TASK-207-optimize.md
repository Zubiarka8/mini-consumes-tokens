# Checkpoint: TASK-207 optimization of the Codex evaluation work

Task: make the Codex commits `0e10084` (labeled corpus, scorer, CLI tests) and
`ba16cb2` (visibility tests) smaller, faster and clearer without changing
behavior or results. Agent: Claude Code (single-client session). Status: REVIEW.
Branch `feat/207-token-twins`, worktree `mct-207-token-twins`. Base `853f2a6`.

## Commits

- `5a9fdc2` test(duplicates): one `duplicates(project, args)` helper replaces four
  report helpers' bodies and a dozen inline "run, assert success, read stdout" copies.
- `c314dfa` test(duplicates): `Project::indexed(tag, files)` replaces 22 tests'
  write-each-file-then-init setup.
- `930862b` refactor(bench): the scorer parses and matches the primary report once
  (it did both twice); unused `os` import removed.
- `f59172e` perf(bench): TOON `split_row`/`unquote` skip the character scan when a
  row has no quote or a quoted cell has no backslash (the profiled hot spot).
- `fe68523` refactor(cli): renderer drops needless `String` copies; `joined_lines`
  shared by the two line-number tables.
- `fede724` chore(errors): ERR-015 recurrence (installed MCP server older than the
  primary index schema) recorded OPEN.

Changed code: `crates/mct-cli/tests/duplicates.rs` 1778 → 1625 lines (+268/−421),
`benchmarks/score_duplicate_labels.py` +12/−9, `crates/mct-cli/src/duplicates.rs`
+19/−16. `labels.toon`, the corpus, `duplicate_visibility.rs` and `mct-index` are
unchanged.

## Evidence that nothing changed

- Tests: 40 passed before and after (`duplicates` 37, `duplicate_visibility` 3).
- `duplicates` output: release binary at `853f2a6` vs `fe68523` byte-identical on the
  labeled corpus and on this repository across 7 option sets each (policies,
  excerpts, partial blocks, ranking/pagination/trivial-calls, language/path filters).
- Scorer: output byte-identical to the `853f2a6` scorer on the default run, two other
  policies, a failing run, a rejected opt-in flag and four saved reports; parsed TOON
  structures equal on 15 files; `test_score_duplicate_labels` OK. Current scorer
  output equals the committed `token-twins-labels-integrated.toon` except the binary
  path header. `token-twins-labels-baseline.toon` predates the integrated binary and
  is no longer reproducible with it (expected; it is documented as the pre-integration
  baseline).

## Timings (host load average 45-73 from an unrelated benchmark; interleaved runs)

- Both test binaries, 20 rounds: median 1.41 s before / 1.45 s after, min 1.16 / 1.11 s.
  No speedup claimed: the process count (init + duplicates spawns) is unchanged.
- Scorer on the full corpus, 12 rounds: median 0.500 / 0.501 s, min 0.422 / 0.427 s.
  Dominated by the four `mct-cli` processes (~0.24 s). TOON parsing of labels plus
  four reports alone: min 29-37 ms before, 20-24 ms after.

## Full check

`scripts/unix/check.sh` on `fede724`: fmt ok; 1588 passed, 0 failed, 24 ignored;
CI clippy ok; eval no regressions (accuracy 1.000, catalog ~9021 tokens).

## Found, not changed

- Sharing one index across the four partial-block tests (and the four default-project
  tests) would save about three `init` spawns each (~60 ms CPU per spawn) but needs a
  process-wide fixture without `Drop` cleanup or merged tests; not justified.
- `rust_twins_that_differ_only_in_visibility_group` (CLI, broad) overlaps
  `duplicate_visibility.rs` (library, every policy); kept: it is the only end-to-end
  check through the binary and also asserts the inspect hint and the excluded twin.
- The library `Fixture` in `duplicate_visibility.rs` and the CLI `Project` are
  coincidentally similar (different harness and crate API); kept separate.
- The scorer and `duplicate_analysis.py` share no logic (`run` vs `wait4`-based
  `measured`); nothing to merge.
- Running the scorer's three `duplicates` invocations concurrently would cut roughly
  0.08 s; not done because the gain is within the observed noise.
- `benchmarks/token-twins-types-report.md` (stale F1.3 implementation report whose
  tables duplicate `token-twins.md` § F1.3) was removed with the user's approval;
  its one link now points to the copy kept in history at `90a3817`.
- MCP: ERR-015 is open; the fix (`scripts/unix/reinstall.sh --reindex`) touches the
  primary checkout and was not run.

## Next action

Maintainer review of `853f2a6..HEAD`. No push, PR, branch or worktree deletion done.
