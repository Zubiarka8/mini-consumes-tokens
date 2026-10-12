# TASK-CSS-NESTING-115 checkpoint

## Final recovery review (2026-10-12)

Branch/worktree: `codex/review-css-115-final` / `mct-review-css-115-final`.
Review base: `a3e10b0f450a93107061152feb862a1d242820a1`; recovery starts at `e3f7ca9`.
Status: focused review and fixes complete; pending coordinator review and combined workspace verification.

Two additional correctness findings were fixed against the [CSS Nesting specification](https://drafts.csswg.org/css-nesting-1/#nest-selector):

- A leading combinator implies an initial parent even when explicit `&` occurs later. `.a, .b { + .next + & {} }` now produces all four combinations. This implicit slot participates in the 256-name cap and byte budget.
- Complex parents require grouping when replacing `&`: `.a .b { & + & {} }` resolves to `:is(.a .b) + :is(.a .b)`, rather than a different descendant/sibling chain. Each parent still gets a separate expansion, never a grouped parent list. Simple-parent names such as `.a:hover` are preserved. Complexity is determined from AST combinators outside function arguments; strings and escaped ampersands remain unaffected.

Regression checks: the leading-combinator test failed before the fix (zero of the four expected names). Added focused complex-parent/function-argument and implicit-slot limit coverage. Existing deep-nesting and Tailwind expectations now use individual complex-parent grouping. The regenerated corpus changes exactly one symbol spelling: `.hb-nav a:hover` to `:is(.hb-nav a):hover`.

Independent checks on the recovered result:

- `cargo test --offline -p mct-lang-css`: 71 passed, 0 failed, 1 ignored (11 unit, 14 corpus, 6 integration, 40 parse; corpus report intentionally ignored).
- `cargo clippy --offline -p mct-lang-css --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`: exit 0.
- `cargo fmt --all --check` and `git diff --check`: exit 0.

The website CSS limitation now documents actual selector indexing, bounded nesting expansion, and the grammar limitations. Shared coordination records were not edited. User-requested local Dogfooding hook/script changes are excluded from the feature commit. No push or PR.

Remaining limitations: DOM matching and specificity are not modeled; upstream grammar limitations (#116) remain. Combined workspace/evaluation and platform CI verification belong to the coordinator. The earlier checkpoint below records predecessor work; the grouped selector spellings described here supersede its plain complex-parent examples.

Exact next action: coordinator reviews the recovery commit (resolve `HEAD`) and integrates it in the isolated combined tree before running required workspace checks.

Task / previous agent / status: TASK-CSS-NESTING-115 (issue #115, CSS Nesting selector resolution) / earlier session on this branch (`1fa4b3f`, `910ceca`, `992892c`) / completed with the user's decisions applied, awaiting coordinator review.

Branch / worktree name / base commit / latest commit: `claude/css-nesting-115` / `mini-consumes-tokens-claude-css-nesting-115` / `a3e10b0f450a93107061152feb862a1d242820a1` / implementation `1fa4b3f` + `3cbb88c`, docs `910ceca` + `1ab2ea4`, then this checkpoint commit (resolve with `git -C <worktree> rev-parse HEAD`). Not pushed; no PR.

Objective and acceptance criteria: index a nested CSS rule under its selector resolved against the parent rule (explicit `&`, implicit nesting, multiple levels, comma lists, repeated `&` as the full product, context through `@media`/`@supports`/`@layer`/`@container`), from AST `nesting_selector` nodes only (strings and `\&` untouched); no literal `&…` name next to a resolved one; at most 256 resolved names per rule, a rule over it kept unexpanded with a visible marker; no panic or unbounded allocation; scope `mct-lang-css` only.

User decisions (binding, applied): no literal `&:hover` beside `.a:hover`; limit 256 names per rule; over it, no abort and no partial expansion, the selector is indexed as written plus `/* mct: not expanded, over 256 selectors */` (constant `UNEXPANDED_MARKER`, `pub` in `mct-lang-css`), its nested rules plus `/* mct: not expanded, parent not expanded */`; a selector without `&` also keeps its plain name; repeated `&` gives every combination.

Completed / in progress / not started:
- Completed: `crates/mct-lang-css/src/lib.rs` (`Parents::Unexpanded`, `Inner`, `Unexpanded` reasons and `mark`, `SelectorParts::{expansion_count, resolved_bytes, resolve}` odometer product, `Walker::{index_rule_set, resolve_nested}`; `KEEP_LITERAL_NESTED_SELECTORS`/`keeps_literal_name` removed); 3 new tests in `tests/parse.rs`; `nesting_limit_tests` now 6 tests (exactly 256 resolves for 1x256, 2x128 and 2 parents x 8 `&`; 257 stays unexpanded and marked; plain-name child; budget); Tailwind comment updated; ERR-004, HISTORY, corpus progress updated.
- Not started: merge; live index verification (forced reindex).

Changed paths and uncommitted changes: `crates/mct-lang-css/src/lib.rs`, `crates/mct-lang-css/src/libraries/tailwind/mod.rs`, `crates/mct-lang-css/tests/parse.rs`, `crates/mct-lang-css/tests/corpus/expected.snap` (only in `1fa4b3f`; unchanged by `3cbb88c`), `internal/corpus-progress.md`, `errors/REGISTRY.md`, `errors/HISTORY.md`, this file. No uncommitted changes.

Checks executed, exact results, and checks still pending:
- Red before `3cbb88c`: `cargo test -p mct-lang-css --test parse`: 35 passed, 3 failed (the three new repeated-`&` tests).
- Mutation: limit check loosened to 257: `cargo test -p mct-lang-css --lib`: 8 passed, 2 failed; restored: 10 passed.
- `cargo fmt --all --check`: exit 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`: exit 0, no warnings.
- `cargo test --workspace --no-fail-fast`: exit 0; summed: 1249 passed, 0 failed, 24 ignored (1244 at `992892c` + 5 new).
- `cargo run -p mct-eval`: Accuracy 1.000, "No regressions." (4 new cases without baseline, as on base).
- Pending: coordinator review; Linux/Windows CI.

Known problems and relevant decisions:
- `@scope` resets context to "no parent" (its `&` is the scoping root). `@container (...)` inside a rule is rejected by tree-sitter-css 0.25 (#116); tested as a named `@container` inside a rule and `@container (...)` around one.
- A top-level `&` or one in `@utility`/`@custom-variant` has no parent and is indexed as written, unmarked (not an expansion failure).
- Names are a cross product, not `:is()`; specificity differences of the desugaring are not modelled.

Dependencies and outstanding worker/process/session references: none running.

Files or symbols to inspect first: `crates/mct-lang-css/src/lib.rs` (`MAX_RESOLVED_SELECTORS_PER_RULE`, `UNEXPANDED_MARKER`, `Unexpanded`, `SelectorParts::resolve`, `Walker::resolve_nested`).

Exact next action and resume condition: coordinator reviews `a3e10b0..claude/css-nesting-115`, then pushes/opens a PR (worker not authorized to).
