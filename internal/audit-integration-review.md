# Integration review: first audit batch

- **Branch:** `codex/audit-integration-review` (local; not pushed and no PR opened).
- **Audited base:** `1880048dd8274b0b3a7f82b07bedd3219bde5ca1`; PR #90 remains a dependency.
- **Integrated commit:** `a953dda70c0c0ff04d5434db5c450adcb78004e7`.
- **Review fix:** `3a049de`. The commit adding this report is the final HEAD of the branch and the base for the next batch.

## Reviewed commits

| Delivery | Commit | Merge on integration branch |
|---|---|---|
| Exclusions (F03): reload `.mctignore`/`.gitignore` without restarting | `52aa751` | `178104c` |
| Lua (F04): register in CLI and MCP server | `39dd255` | `696b066` |
| Rust dead code: value uses and test attributes | `331218e` | `a953dda` |

The three original branches were not modified. The diff from the base covers 28 files, all within the three deliveries' scope. It contains no secrets, `.env`, `.mcp.json`, logs, or personal configuration. The SQLite schema, `LanguageParser` trait, and MCP tool signatures are unchanged. An internal public `mct-index` API did change: `ExcludeSet::clone` now shares rules, and `for_project`, `reload`, and `is_rules_file` were added. This API is not among the changes that the repository rules require to have a prior issue.

## Verification evidence

| Check | Result |
|---|---|
| `scripts/unix/check.sh` on `a953dda`, before the review fix | Tests: **784 passed, 0 failed, 17 ignored**; CI Clippy clean; evaluation had no regressions (accuracy 1.000) |
| `cargo fmt --all --check` and `scripts/unix/check.sh` on `3a049de` | Formatting clean; **785 passed, 0 failed, 17 ignored**; Clippy clean; evaluation had no regressions |
| New Lua tests with the base-branch registrations (removed each `LuaParser` registration from both `build_registry` functions, then restored it) | `a_lua_file_is_probed_indexed_and_queryable_through_the_cli_registry` and `a_lua_file_is_indexed_with_its_symbols_and_calls` fail |
| New Rust tests with the base parser (`git show 1880048:crates/mct-lang-rust/src/lib.rs`, then restored) | `dead_code.rs`: 3 of 4 tests fail, matching the three behaviors fixed by the delivery |
| Exclusion tests with `reload()` forced to return `false` to simulate the base behavior, then restored | `exclude_reload.rs`: 4 of 5 fail; `background_watcher.rs`: the real watcher test and `changed_paths` test fail |
| `scripts/unix/corpus-report.sh rust` without `--bless` | 12 passed. Totals 239 / 817 (218) match `internal/corpus-progress.md`; 42 references match the 42 snapshot rows added |

The macOS watcher tests passed in this environment without permission issues. Each command used only `SDKROOT=…MacOSX26.5.sdk` in its environment; no global binaries were installed. Source exploration used this worktree's MCP, whose path was confirmed with `list_symbols` on `crates/mct-index/tests/exclude_reload.rs`, a file that exists only on this branch. Behavior reproductions used locally compiled tests.

## Resolved findings

**R1 — P3: Rust struct-pattern shorthand was not counted as a local binding.**

In `crates/mct-lang-rust/src/lib.rs::collect_identifiers`, only `identifier` nodes were recognized, not `shorthand_field_identifier`. For `let Config { root, .. } = c; root` (and the equivalent parameter, `match` arm, or closure pattern), a later `root` use could be misclassified as a reference to a same-named `fn root`. `find_dead_code` would then hide a genuinely unused function, contradicting the delivery's stated shadowing behavior.

Commit `3a049de` also accepts `shorthand_field_identifier`. Regression test `a_struct_pattern_shorthand_binding_shadows_a_same_named_function` returned four `root` references before the fix and none afterward. Corpus snapshots did not change.

**R2 — P3: documentation overstated the guarantee provided by `#[test]`.**

In `crates/mct-index/src/dead_code.rs::looks_like_test_name`, the text implied that a `#[test]` reference was guaranteed. It now clarifies that resolution is project-wide by name and can hide a same-named symbol elsewhere (for example, an unused `fn parse` alongside `#[test] fn parse`). This is F02's general limitation, not a safety check. Fixed in the same commit.

## Open findings (none block this integration)

**E1 — P3: `ExcludeSet::reload` reads, compares, and replaces rules in separate steps.**

The watcher (without the index lock) and an active reindex (with the lock) can reload rules concurrently while `.mctignore` is being edited. The last write wins, so stale rules can exist temporarily, and a reindex could observe mixed rules if the watcher changes them mid-walk. This cannot persist: watcher reloads that detect a change queue a full reindex, which waits for the lock and rereads the files; `reindex_paths` also rechecks under lock and performs a full walk if rules no longer match. Follow-up: hold the write lock for the entire reload. This was not done here because no deterministic regression was possible without injection points.

**E2 — P3: a read error for `.mctignore` clears project rules.**

An unreadable file (for example, due to permissions) or a read during an editor's truncate-and-rewrite window can clear project exclusions. A full reindex can then index excluded paths again; built-in secret exclusions remain active. A partial-write case self-corrects on the next event; a persistent read error does not. The behavior already existed at startup but can now occur while the server is running. Follow-up: preserve prior rules for read errors other than “file not found.”

**RS1 — Known F02 limitation, out of scope:** relations resolve by project-wide name. New value-use and `#[test]`/`#[bench]` references can suppress a same-named symbol in another file or language from `find_dead_code`. `find_references`, `impact_analysis`, and `build_context_pack` also show synthetic references from the `tests` module to test functions. This is intentional and documented in R2. Evaluation showed no regressions and all 784 tests passed. Resolve under F02, not in this integration.

**RS2 — P3: Rust bindings outside functions.**

Closure parameters in a `const`/`static` initializer (for example, `const F: fn(u32) -> u32 = |helper| helper;`) and identifiers inside `macro_rules!` are not treated as local bindings. They can be recorded as references to a same-named function in the file. Shadowing also ignores order and block scope: a binding with that name anywhere in a function is enough. This documented over-approximation can suppress references but cannot invent them. It is rare; handle as follow-up work.

**L1 — Informational: Lua and publishing.**

- `mct-cli` adds `mct-mcp-server` as a path-only dev dependency. `cargo publish` removes it, as it already does for `mct-corpus`.
- `mct-lang-lua` is publishable (`publish = false` is absent) and has a version in `[workspace.dependencies]`.
- `every_language_crate_in_the_workspace_is_registered_in_production` reads `../` from `CARGO_MANIFEST_DIR`, so it runs only in the workspace, never from a packaged `.crate`.
- `omni-app/luatools/build.lua` was updated (`return tostring(target)`) to provide a call to verify. Updated counts (28→29 files, 101→104 symbols, Lua coverage 1 file / 3 symbols) match the test results.

## Decision

**READY FOR THE NEXT BATCH.** The integrated test suite, CI Clippy, formatting, and evaluation pass on this branch's HEAD. Each delivery has tests that fail against the previous behavior. The one clear in-scope bug (R1) is fixed with a regression test. E1, E2, and RS2 are P3 follow-ups; RS1 belongs to F02. No schema, `LanguageParser`, or MCP signature change requires a prior issue. Publishing remains subject to coordination and PR #90.
