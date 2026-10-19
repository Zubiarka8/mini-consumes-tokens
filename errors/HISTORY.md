# Error history

## 2026-10-10 — Initial inventory

- Evidence: `get_indexing_status` called twice with the same result; 556 indexed files, 11,205 symbols, 18 parse failures, index timestamp `1791629091`.
- Checkout revision: `7558113d1926a99beffdd265913bcd75f15f8e6e`, branch `docs/fumadocs-site`, with unrelated uncommitted work. No reindex performed.
- Created ERR-001 and ERR-002 for the Bash and PowerShell script diagnostics; root cause unverified.
- Classified 16 malformed corpus fixture rejections as EXPECTED (ERR-003), based on the corpus contract.
- Recorded CSS/Tailwind, React JSX and Python limitations from existing project documentation (ERR-004 through ERR-007). These are not additional failures in the index snapshot.
- Direct reproduction: `scripts/unix/parse-probe.sh scripts/unix/pr-body.sh scripts/windows/pr-status.ps1` exited 1 and reported the same two syntax errors. ERR-001 points at a `case` line containing parameter expansion; ERR-002 points at the `gh pr view` line opening a multiline quoted jq expression.
- `bash -n scripts/unix/pr-body.sh` passed (exit 0), so the Bash diagnostic is a parser rejection of shell-accepted syntax. Native PowerShell syntax validation and script runtime checks were not performed.
- This change adds records and maintenance instructions only; the two parser failures remain OPEN.
- Recorded ERR-008: `get_tool_schema` returned ``failed to deserialize parameters: missing field `name` `` after a caller used `tool` instead of `name`. Corrected retry succeeded in this session; classified RESOLVED, caller error.
- Updated rules.md to require user notification and registry/history updates for every project MCP error, including connection/query failures and corrected invocation errors.

## 2026-10-10 — Library maintainability validation

- MCP status snapshot: 559 indexed files, 11,213 symbols, index timestamp `1791629316`; the same 18 parse diagnostics remain (ERR-001/002 and ERR-003's fixtures).
- The isolated library-maintainability refactor preserves implementation bodies, the public parser entry point, manifest unit tests and corpus snapshots. Focused `mct-index` and JS/TS tests passed.
- Created ERR-009 after the full sandbox run failed three MCP server watcher tests: `watcher_applies_ignore_file_edits_without_a_restart` (`expected the new rule to drop hidden.rs`), `watcher_reindexes_after_a_file_change_settles` (``expected the watcher to have auto-reindexed and picked up `two` ``), and `watcher_drops_a_deleted_file_and_follows_a_rename_incrementally` (`expected the deletion and the rename to be indexed`). Rerun outside the sandbox requested to distinguish environment restrictions from a product failure.
- ERR-009 resolved as an execution-environment failure: `cargo test --offline --locked -p mct-mcp-server --test background_watcher` outside the sandbox passed all 11 tests (exit 0, 20.30 seconds), using the same watcher test binary with no code changes. The full suite is being repeated outside the sandbox to cover the tests after the original failure.
- The original check run's formatting, CI Clippy and evaluation gates passed; its aggregate exit code remains 1 because of the sandbox watcher failures. Evaluation reported no regressions (accuracy 1.000).
- Full native rerun, `scripts/unix/check.sh --only test`, passed: 1,178 tests passed, 0 failed, 24 ignored. No ignored-test policy was changed.
- Reviewed refactor saved in local commits `10110f5` (manifest formats), `fec4fb5` (JS/TS private modules), and `9dfa1cc` (implementation guide and maintenance rule). Verified source/documentation files copied to the primary checkout after checking its originals against the base; unrelated website work preserved. No push or PR created.
- Existing parser failures and documented framework limits retain their previous statuses. This refactor changes maintenance boundaries, not framework coverage.
- After integration, MCP status reports 568 indexed files and 11,236 symbols at timestamp `1791630208`, with the same 18 parse failures and dependency counts. `list_symbols` finds the five declarations in the new JS/TS `walker/exports.rs`; the watcher has indexed the new layout without a forced reindex. No new indexing failure observed.

## 2026-10-10 — Library onboarding audit (TASK-LIB-001)

- Registry incorporated into `claude/library-onboarding-audit` by cherry-picking `e9280d8` (PR #168, still open); entries updated here instead of starting a second registry.
- Observation: `mct-cli --root . reindex --force` and `status` at `dab3059`, no `.mctignore`; index timestamp `1791636301`, 550 files, 11,501 symbols, the same 18 diagnostics.
- ERR-001 → KNOWN_LIMITATION. Isolated with one-line `parse-probe.sh` files: tree-sitter-bash 0.25.1 (latest) rejects `;` inside `${var:+…}`. `bash -n` and a `bash -c` run accept it.
- ERR-002 → KNOWN_LIMITATION. tree-sitter-powershell 0.26.4 (latest) rejects comma-separated bareword arguments (`Select-Object Name,Id`, `--json number,title`); the multiline jq string parses line by line. The earlier jq hypothesis was wrong. Native PowerShell validation not run (no `pwsh`; Windows CI requires a push).
- Both rejections are also recorded in `internal/corpus-progress.md`. Upstream issues not filed.
- ERR-003 unchanged. The optional `.mctignore` line that keeps the fixtures out of a working index is documented in CONTRIBUTING; fixtures and their rejection tests are kept.
- ERR-006 evidence refreshed: the React test moved to `crates/mct-lang-js-ts/src/libraries/react/mod.rs` and passed.
- Created ERR-010 (OPEN): `find_references RustParser` found none of the registry uses (`Arc::new(mct_lang_rust::RustParser)`); the compiler found them when the registry moved to `mct-languages`. Reproduced by an `#[ignore]`d regression test in `crates/mct-lang-rust/tests/parse.rs`; the parser fix is not made.
- No project MCP tool returned an error in this session.
- ERR-002 evidence: the Windows CI step "PowerShell scripts" (PR #170, run 38055596733) parsed `pr-status.ps1` with PowerShell 7's parser without errors, so the rejection is the grammar's. The same step found a real error the grammar accepts: `scripts/windows/pr-body.ps1:22` used `"$checkRel:"`, a drive-qualified variable reference; fixed as `${checkRel}:`. Not an MCP tool error, so no registry entry.
- ERR-010 → RESOLVED on `claude/library-onboarding-audit` (PR #170). The Rust walker records a path in value position as `References` with call-path evidence; the pinning test is no longer ignored, a second test checks that `other::helper` carries `module: other`, and the Rust corpus snapshot was re-blessed (+73 references). `mct-cli dead-code --language rust` on the worktree drops two false candidates (`default_suite`, `Rules`). Live MCP recheck pending a server rebuild.

## Independent PR #170 review (2026-10-10)

- Reviewed revision: `57d536544556af0becdec3317d4705747302bbbd`. An isolated MCP server built from that revision indexed its own review checkout; `reindex` with `force: true` reported 550 parsed files, 11,509 symbols, and the same 18 known diagnostics: ERR-001/ERR-002 plus the 16 intentionally malformed ERR-003 fixtures. No new diagnostic was found.
- ERR-010 live verification completed: `find_references` with `symbol: RustParser`, `format: toon` reports seven references, including `build_registry` at `crates/mct-languages/src/lib.rs:12:47`, uniquely resolved to `crates/mct-lang-rust/src/lib.rs:15`. The lookup succeeds after forced reindexing with the reviewed parser; an unchanged index built by an older installed CLI does not automatically acquire new relations.

## 2026-10-10 — ERR-011 caller correction

- Recorded the previously reported `find_references` mistake: `name` was supplied instead of `symbol`. The original response is unavailable; this entry distinguishes it from the controlled reproduction.
- `get_tool_schema` with `name: "find_references"` succeeded and identified `symbol` as required.
- Reproduced with `name: "RustParser", format: "toon", limit: 1`: ``failed to deserialize parameters: missing field `symbol` ``. Notified the user.
- Corrected retry with `symbol: "RustParser", format: "toon", limit: 1` succeeded: eight references, one displayed, seven omitted by the requested limit. ERR-011 is RESOLVED; no source or server change was required.
- The connected MCP serves the preserved primary checkout/index, whose HEAD is `af20fe955f58d1c3fab1d0e2221208f34a30058f`. This checks argument validity; it does not validate current main's symbol graph or refresh any previous diagnostic snapshot.
- Prevention: consult the required input schema before calls and distinguish discovery's `name` from relation queries' `symbol`.

## 2026-10-12 — ERR-004 fix candidate (TASK-CSS-NESTING-115)

- Reproduced ERR-004 on `a3e10b0`: 13 new nesting tests in `crates/mct-lang-css/tests/parse.rs` failed before the change; nested rules were named `&:hover`, `& > .b`, `> .c`.
- Branch `claude/css-nesting-115` (`1fa4b3f`) resolves nested selectors from the tree-sitter `nesting_selector` nodes. The CSS corpus snapshot changed by 11 renamed and 2 added rules; `cargo test -p mct-lang-css` and `scripts/unix/check.sh` pass.
- Status stays KNOWN_LIMITATION: the fix is not merged and no refreshed index was checked. DC1 (whether to keep literal `&` names) and DC2 (the expansion limit) remain open product decisions.
- No project MCP tool was used or returned an error in this task.
- Follow-up (`3cbb88c`): repeated `&` now takes every parent combination (`.a, .b { & + & {} }` gives four names; before, only `.a + .a` and `.b + .b`, shown failing by `repeated_ampersands_combine_every_parent_selector`). Over 256 resolved names, a rule is indexed as written with a `/* mct: not expanded, over 256 selectors */` suffix instead of an unmarked literal. The user settled both open decisions: no literal `&` names, 256 per rule. The CSS corpus snapshot is unchanged by this follow-up.

## 2026-10-12 — Privacy review environment

- ERR-012 observed and resolved: local MCP initialization in `feat/index-privacy-hygiene` failed inside the filesystem sandbox with `Error: database error: unable to open database file`, `Error code 14: unable to open database file`. Notified the user; retried with authorized filesystem access, using the project's own server binary and the same worktree root.
- Successful `get_indexing_status` on the dirty tree based on `32f8a79ccfcd09c3d790e43edc284d7602e9737b`: timestamp 1791792386, 594 files and 12,370 symbols. The 16 malformed-corpus parse failures are expected (ERR-003); no other parse diagnostic was observed in this worktree.
- Source lookup proceeded through local `build_context_pack` calls. A connected MCP with an unconfirmed root was not used for the code review.
- Intentional malformed ignore-file and permission-denial fixtures belong to the synthetic regression tests; their diagnostics and reproduced privacy defects are recorded in `project-management/checkpoints/index-privacy-hygiene-review.md`.

## 2026-10-15 — Issue #207 planning

- ERR-013 and ERR-014 observed and resolved in the same session: two caller mistakes inside one `batch` call (a non-existent file path for `get_file_skeleton`, and `name` instead of `symbol` for `build_context_pack`). The other batch sub-queries succeeded; both lookups succeeded on retry with correct arguments. The user was notified. No server defect.

## 2026-10-15 — Token twins improvement validation

- ERR-016: the connected MCP index reports only 6 files and cannot locate `GroupEvidence` in the requested path. CLI-backed MCP queries with an explicit checkout root succeeded (1,188 files). Connected index coverage remains unverified; implementation exploration uses the project's CLI tool calls.
- ERR-009 reproduced before F1.3 code changes: `scripts/unix/check.sh` exits 1 both inside and outside the sandbox on the dirty `ac525ca` tree. Each reports 1,306 passed, 3 failed, 23 ignored; the failing tests remain `watcher_reindexes_after_a_file_change_settles`, `watcher_applies_ignore_file_edits_without_a_restart` and `watcher_drops_a_deleted_file_and_follows_a_rename_incrementally`. Formatting, strict workspace Clippy and evaluation pass (accuracy 1.000). No watcher change or test weakening is included in F1.3.
- ERR-009 after the F1.3 spelling-evidence increment: native `scripts/unix/check.sh` exits 1 with 1,312 passed, 3 failed, 23 ignored. The same three watcher assertions fail; fmt, strict workspace Clippy and evaluation pass (accuracy 1.000, catalog approximately 9,021 tokens). All newly reached detector tests pass. A workspace `--no-fail-fast` run is used to verify test binaries left unexecuted after the watcher failure.

- ERR-015 observed and resolved: an old local release MCP server rejected the temporary tracked-crates index before initialize with `Attempt to migrate a database with a migration number that is too high`. Rebuilt the release server from the current branch; initialize, tools/list and get_indexing_status then succeeded. Only the 16 expected malformed fixtures were reported. The primary index was not changed.
- ERR-009 reopened: full checks reported three background-watcher failures (1,287 passed, 3 failed, 23 ignored). Native focused rerun reproduced them, and an isolated unmodified `ac525ca` source build reproduced the same three failures outside the sandbox in an isolated release target (8 passed, 3 failed, 20.18 s). Previously resolved sandbox-specific evidence is retained; the current cause is unconfirmed and is not introduced by the token-twins changes.

- ERR-009 baseline coverage extended: full native workspace --no-fail-fast reports
  1,521 passed, 9 failed, 24 ignored. In addition to the three background_watcher
  failures, a_saved_file_becomes_searchable_only_after_the_delay and all five
  reindex_delay_timing cases fail. A native isolated reconstruction of the
  pre-F1.3 dirty source, with mct-core, mct-tree-sitter, adapters, index and server
  recompiled, reproduces all six additional failures (3 passed / 6 failed).
  These are pre-existing failures; the root cause remains unconfirmed. No watcher
  implementation or assertion was changed, skipped or weakened.

## 2026-10-16 — #207 Codex-work optimization session

- ERR-015 recurred and is OPEN again: the session's `mini-consumes-tokens` MCP server failed with `CONNECTION_CLOSED`. `scripts/unix/mcp-smoke.sh` in the primary checkout reported `migration error: ... Attempt to migrate a database with a migration number that is too high` from the installed `mct-mcp-server`. The user was notified. The reinstall/reindex fix was not applied because the session's brief excludes the primary checkout; exploration used `mct-cli call` with the `feat/207-token-twins` worktree as root and its own freshly built index. Resolution needs a reinstall from the intended checkout and a successful initialize.

## 2026-10-16 — Semantic proposal documentation review

- ERR-017 OPEN: installed `mct-cli --root . status` on dirty `claude/semantic-model-eval` at `ac525cac1453f2b296588abe621d0e002ac96d66` reports `Error: migration error: rusqlite_migration error in migrations definition: Attempt to migrate a database with a migration number that is too high`. No reinstall, schema reset or direct database access. Local release MCP initialized and supplied status/context packs, so documentation review could continue; installed CLI recovery remains unverified. Related ERR-015 server instance remains resolved.
- ERR-016 remains OPEN: connected MCP status is 6 files / 293 symbols at `1792140027`; explicit-root local release MCP reports 1,192 files / 19,548 symbols at `1792140247`. Connected scope is unsuitable for this review; reconnect/reconcile its root.
- ERR-003 EXPECTED: local release status reports 16 malformed corpus fixtures and two malformed copies in the discarded PR snapshot under `prompt/`. No unexpected parse failures in the reported 18. Counts differ from the historical semantic benchmark and must not be treated as an identical corpus.

## 2026-10-18 — ERR-009 macOS watcher reproduction

- Investigated the three recorded watcher failures in an isolated clone of `feat/207-token-twins` at `71ec6d0`. On Darwin 27, a direct `RecommendedWatcher` probe timed out after creating a file, showing no event reached the callback; temporary instrumentation confirmed that the watcher thread itself started and acquired the index.
- The same direct probe delivered a create event with `KqueueWatcher`; all 11 `background_watcher` tests then passed with `notify/macos_kqueue` enabled. This isolates the observed failures to FSEvents delivery in this host, rather than `changed_paths` or index reindexing.
- Added a target-specific macOS dev-dependency enabling kqueue in test build graphs. `cargo test --locked -p mct-mcp-server --test background_watcher` passes 11/11 without feature flags. `cargo tree --target aarch64-apple-darwin -e normal -i notify` confirms normal builds without dev-dependencies still use `macos_fsevent`. All-target builds can also activate the dev-dependency feature through Cargo feature unification.
- The broader package check exposed `files_excluded_by_ignore_rules_never_trigger_an_update`: kqueue's parent-directory notification caused a hash-skipped scan (`parsed=0`, `unchanged=1`, `removed=0`) to be announced as an index update. The worker now logs no-op scans at debug level and keeps the directory reconciliation needed to discover new paths. The test failed before this change and passed afterwards; its assertions and positive control are unchanged. All five timing tests and all four delay end-to-end tests pass, covering the six historical failures.
- An independent notify 8.2.0 probe reproduced `backend: Fsevent` followed by `Error: Timeout`; selecting kqueue produced `backend: Kqueue` and `received: Create(File)`. ERR-009 stays OPEN for native FSEvents delivery in this host. No default production backend was changed, and passing kqueue tests do not establish FSEvents runtime coverage.
- Full candidate validation: `CARGO_NET_OFFLINE=true scripts/unix/check.sh` passes formatting, 1,588 tests (zero failed, 24 existing ignored), strict workspace Clippy, and evaluation with accuracy 1.000 and a catalog of approximately 9,021 tokens. Tests were not removed, ignored, or weakened by this change. At validation time, application to the original worktree was blocked because writes outside the runner's writable roots were rejected; no commit, push, or PR was created.
- The user subsequently applied the patch to the original worktree. For separate PR preparation, the watcher change was copied into an isolated branch and rebased onto GitHub main `4c3f155`, without altering the source worktree. Full validation there passed formatting, 1,600 tests (zero failed, 24 existing ignored), strict workspace Clippy, and evaluation (accuracy 1.000, catalog approximately 10,727 tokens). Native FSEvents delivery remains open.
- After PR #211 merged, its ERR-015 record was reconciled with ERR-009 on main `a38dcc5`. The combined tree passed `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/mct-207-watcher-fix/target scripts/unix/check.sh`: formatting, 1,600 tests (zero failed, 24 existing ignored), strict Clippy, and evaluation with accuracy 1.000 and a catalog of approximately 10,727 tokens.

## 2026-10-18 — Explicit source reindex

- ERR-016 reconfirmed: connected `cargo_deps.get_indexing_status` reports 6 files / 293 symbols. Forced connected-MCP `reindex` completes but still covers only those 6 files; `build_context_pack(symbol="spawn_watcher", path="crates/mct-mcp-server/src/background.rs")` returns `No symbol named spawn_watcher found in the index`. Explicit-root `target/release/mct-cli --root . reindex --force` indexes the checkout successfully (1,206 parsed files, 19,660 symbols). It reports the 16 expected malformed corpus fixtures plus two malformed copies under `prompt/github/discarded/resolve-pr151-snapshot/` (ERR-003). The connected MCP root remains mismatched; use explicit-root CLI-backed lookups until it is reconnected.

## 2026-10-18 — Concurrent-agent binaries and parser triage

- Scope confirmed by the user: `claude/semantic-model-eval`, dirty `ac525ca`, where the discarded snapshot exists. The sibling `claude/semantic-model-eval-run` has no snapshot and is outside this investigation.
- ERR-016 cause clarified: `cargo_deps.get_file_tree(depth=2)` shows the dependency corpus `smallvec-1.16.1`. Its six indexed files are not an incomplete project index; it is the wrong server for project-symbol lookups. Explicit-root isolated CLI/server operations succeed. Client reconnection remains pending.
- ERR-017 reproduced for both installed tools: CLI status and installed-server smoke each exit 1 with `Attempt to migrate a database with a migration number that is too high`. Existing release and newly built isolated binaries open the same project index. Global installation was not changed.
- ERR-018 opened: normal-target CLI build exits 101 with five errors referring to older core APIs, while those APIs exist in the source. Cargo check exits 0. CLI and server builds using a fresh target both exit 0 without a Rust source edit. Fresh CLI forced reindex and fresh-server initialize/status succeed. This verifies the isolated-build mitigation; the exact cache/writer mechanism is not established.
- ERR-003 expected diagnostics: 18 observed (16 malformed corpus fixtures plus two byte-identical snapshot copies), rather than 28. Existing and newly built CLIs reproduce exclusion counts 18 → 16 → 0 on a disposable corpus while preserving the valid control; originals and project rules are unchanged. The focused malformed-contract test passes. Current `pr-body.sh` and `pr-status.ps1` also pass direct probes; historical minimized grammar repros were not rerun.
- Temporary branch-guarded CLI/server launchers and a shell activation file were created outside the repository. Shell syntax and correct-root CLI status pass. Separate worktrees/indexes are recommended if agents edit different sources or schemas. The smoke script still selects the installed global server despite a PATH override, so the successful isolated-server test invoked its absolute path directly. Full evidence and alternatives: [investigation checkpoint](../project-management/checkpoints/index-parse-investigation-2026-10-18.md).

## 2026-10-19 — ERR-015 recurrence during the #207 closure

- ERR-015 recurred: the session's `mini-consumes-tokens` MCP server reported `CONNECTION_CLOSED`. `scripts/unix/mcp-smoke.sh` in the primary checkout (`claude/semantic-model-eval` at `ac525ca`, dirty) printed `migration error: ... Attempt to migrate a database with a migration number that is too high` from the installed server. Not fixed: `scripts/unix/reinstall.sh --reindex` rebuilds the primary checkout's index and was not requested. The session only compared Git state and needed no index query. Status stays OPEN.

## 2026-10-19 — ERR-015, ERR-016 and ERR-017 resolved

- Ran `scripts/unix/reinstall.sh --reindex` from the primary checkout on `main` (`83dff6d`): installed `mct-mcp-server` and `mct-cli` v0.2.0-326-g83dff6d, rebuilt the project index (1289 parsed, 21031 symbols) and `mcp-smoke.sh` printed `server ok` with 22 tools.
- The user reconnected with `/mcp` (`Reconnected to mini-consumes-tokens`). `get_indexing_status` answered with 1289 files / 21031 symbols and 18 parse failures: the 16 malformed corpus fixtures plus two copies under `prompt/` (ERR-003, expected).
- ERR-015 and ERR-017 (installed binary older than the index schema) and ERR-016 (connected server on the wrong corpus) are RESOLVED. ERR-018 stays OPEN: it concerns stale artifacts in the normal Cargo target, which this reinstall did not exercise.
