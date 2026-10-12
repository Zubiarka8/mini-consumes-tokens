# TASK-CSS-NESTING-115 checkpoint

Task / previous agent / status: TASK-CSS-NESTING-115 (issue #115, CSS Nesting selector resolution) / none (old `codex/css-nesting-115` held only two uncommitted tests, reused) / completed with provisional decisions, awaiting coordinator review.

Branch / worktree name / base commit / latest commit: `claude/css-nesting-115` / `mini-consumes-tokens-claude-css-nesting-115` / `a3e10b0f450a93107061152feb862a1d242820a1` / implementation `1fa4b3f`, docs `910ceca`, then this checkpoint commit (resolve with `git -C <worktree> rev-parse HEAD`). Not pushed; no PR.

Objective and acceptance criteria: index a nested CSS rule under its selector resolved against the parent rule (explicit `&`, implicit nesting, multiple levels, cartesian comma lists, context through at-rule blocks), from AST `nesting_selector` nodes only; nested ranges unchanged; bounded expansion; no panic or unbounded allocation; scope `mct-lang-css` only.

Completed / in progress / not started:
- Completed: `crates/mct-lang-css/src/lib.rs` (`Parents` context, `SelectorParts`, `Walker::index_rule_set`, `Walker::resolve_nested`, `selector_parts`, `collect_nesting_selectors`, `keeps_literal_name`, atom filtering); 16 tests in `tests/parse.rs`, 4 limit unit tests in `src/lib.rs` (`nesting_limit_tests`); Tailwind v4 known-limit assertions updated; corpus snapshot re-blessed; ERR-004 and corpus progress updated.
- Not started: DC1/DC2 product decisions; merge; live index verification.

Changed paths and uncommitted changes: `crates/mct-lang-css/src/lib.rs`, `crates/mct-lang-css/src/libraries/tailwind/mod.rs`, `crates/mct-lang-css/tests/parse.rs`, `crates/mct-lang-css/tests/corpus/expected.snap`, `internal/corpus-progress.md`, `errors/REGISTRY.md`, `errors/HISTORY.md`, this file. No uncommitted changes.

Checks executed, exact results, and checks still pending:
- Red before implementation: `cargo test -p mct-lang-css --test parse`: 21 passed, 14 failed (13 behavioural; `self_doubling_nesting_stays_bounded` failed because its first input `& & {` is rejected by the grammar, fixed to `& + &`, then passes as a regression guard).
- Red for the cap: with the cap check disabled, `expansion_over_the_limit_degrades_to_the_literal_selector` failed (3 passed, 1 failed).
- `scripts/unix/corpus-report.sh css --bless --update-progress`: 18 passed, 0 failed; 933 symbols / 17 relations.
- `scripts/unix/check.sh -p mct-lang-css`: fmt ok; test 63 passed, 0 failed, 1 ignored (`corpus_report`, a report by design); clippy (CI flags) no warnings.
- `scripts/unix/check.sh`: fmt ok; test 1244 passed, 0 failed, 24 ignored; clippy no warnings; eval no regressions (accuracy 1.000).
- `cargo +nightly fuzz run parse-css` with nesting seeds, 90 s: 702,571 runs, no crash.
- Pending: coordinator review; Linux/Windows CI.

Known problems and relevant decisions:
- DC1 (open; provisional `KEEP_LITERAL_NESTED_SELECTORS = false` in `src/lib.rs`): a resolved selector written with `&` drops its literal name and `&`-containing atoms. Options: keep both (`true`); keep literal only when resolution fails (current fallback already does this). A nested selector without `&` (`.dropdown-menu` inside `[data-bs-theme]`) keeps its written name, so `bootstrap_shop_customizes_a_versioned_external_bootstrap` passes unmodified; this is an interpretation of DC1's "literal `&…` symbol" wording, switchable in `keeps_literal_name`. Affected tests: `nested_rule_literal_ampersand_selector_is_not_indexed`, the `&:hover`/`& > .hb-card__title` lines of `tailwind_v4_theme_utilities_variants_and_nesting`, `assert_resolved` in `nesting_limit_tests`, the snapshot.
- DC2 (open; provisional `MAX_RESOLVED_SELECTORS_PER_RULE = 256`, plus `RESOLVED_BUDGET_BASE` 1 MiB and `RESOLVED_BUDGET_PER_SOURCE_BYTE` 8 per-file budget charging name length + `size_of::<SymbolRecord>()`): over either bound the rule and its descendants are indexed as written; never an error. Options: truncate to the first N; error (drops the file); another N. Affected tests: the three `expansion_*` and `the_per_file_budget_stops_resolution_without_failing_the_file` unit tests.
- Simplification: several `&` in one selector all take the same parent (`.a, .b { & + & {} }` gives `.a + .a`, `.b + .b`, not the mixed pairs).
- `@scope` resets context to "no parent" (its `&` is the scoping root). `@container (...)` inside a rule is rejected by tree-sitter-css 0.25 (#116); tested as a named `@container` inside a rule and `@container (...)` around one.

Dependencies and outstanding worker/process/session references: none running.

Files or symbols to inspect first: `crates/mct-lang-css/src/lib.rs` (`KEEP_LITERAL_NESTED_SELECTORS`, `MAX_RESOLVED_SELECTORS_PER_RULE`, `Walker::index_rule_set`, `Walker::resolve_nested`), the snapshot diff in `1fa4b3f`.

Exact next action and resume condition: coordinator reviews `a3e10b0..claude/css-nesting-115`, decides DC1/DC2 with the user, then pushes/opens a PR (worker not authorized to). Changing a decision is one constant plus its listed tests.
