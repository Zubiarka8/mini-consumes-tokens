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

## 2026-10-20 — ERR-019: zero tools after MCP startup timeout

- User reports Codex CLI `mini-consumes-tokens: connected (0 tools)`. Client logs show initial reindex followed by `timed out handshaking with MCP server after 29.999999917s`, with catalog assembly reporting tools unavailable.
- `scripts/unix/mcp-smoke.sh --root <empty temporary project> --expect get_indexing_status --list` succeeds: installed server advertises 22 tools. Installed CLI status opens the primary index (1303 files / 21237 symbols, timestamp 1792447378). The separate cargo-deps registration deliberately targets the smallvec fixture and is not primary-root coverage.
- Local project configuration now sets startup_timeout_sec to 180. Configuration write required sandbox escalation and succeeded; TOML value verified. Session reconnection/primary tool availability not verified; ERR-019 stays OPEN. No schema changes or index rebuild.

## 2026-10-20 — Local web follow-up

ERR-019 RESOLVED: primary MCP search/context/status calls now succeed; get_indexing_status returns 1303 files/21257 symbols, timestamp 1792448999, confirming project root and restored tools. Existing 19 parse diagnostics preserved (18 malformed fixtures/copies and review-check.ps1:29). ERR-014 caller mistake recurred in three build_context_pack calls using name for run/DuplicateReport/DuplicateMember; diagnostic ``failed to deserialize parameters: missing field `symbol` ``. Each retry with symbol succeeded; remains RESOLVED.

## 2026-10-24 — Dashboard fingerprint work

ERR-014 caller mistake recurred: `build_context_pack(name: SymbolListEntry)` returned ``failed to deserialize parameters: missing field `symbol` ``. Retried using `symbol`; context retrieval succeeded. Remains RESOLVED.

## 2026-10-24 — Adversarial dashboard audit index observation

get_indexing_status reports 1008 files / 17502 symbols at timestamp 1792854060 with 18 parse diagnostics: 16 intentionally malformed fixtures, the previously observed review-check.ps1:29 rejection, and generated web/spa/spa.css:1. Direct project CLI probe reproduces the CSS and PowerShell `syntax error` diagnostics (exit 1); ERR-020 opened for untriaged CSS coverage. No source exclusion or parser fix. An initial unsupported parse-probe CLI command was corrected to probe after checking help.

## 2026-10-24 — Dead-code editor work

ERR-011 class caller mistake: `find_callers` (inside `batch`) was called with `name: "cautions"` instead of the required `function`; diagnostic: ``invalid args for `find_callers`: missing field `function` ``. Retried as `find_callers(function: "cautions", path: "crates/mct-cli")`: succeeded (one resolved caller, `detail` in `view_dead.rs`). Caller argument error; no server defect. Remains RESOLVED.

## 2026-10-25 — Duplicate copy-label planning

`get_indexing_status({})` succeeded on dirty `feature/dead-code-editor` at
`b61cc7ce971fdc46a61259af95af7fac823ae352`: 1,024 files / 17,861 symbols,
timestamp `1792915025`. The same 18 parse diagnostics remain: 16 intentional
malformed fixtures (ERR-003, EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error`, and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020, OPEN).
No new parser reproduction or fix was attempted. Source-context batches
succeeded; the CSS rejection limits indexed stylesheet coverage, not evidence
of a browser failure. Existing next actions and statuses are retained.

2026-10-25 copy-label implementation startup: get_indexing_status({}) succeeded (1025 files, 17886 symbols, timestamp 1792915721). Same 18 parse diagnostics: 16 expected malformed fixtures (ERR-003), `scripts/windows/review-check.ps1: line 29: syntax error`, and `crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN). No new parser probe or resolution.


2026-10-25 database lifecycle planning: get_indexing_status({}) succeeded on dirty
feature/dead-code-editor, HEAD b61cc7ce971fdc46a61259af95af7fac823ae352: 1031 files,
17987 symbols, index timestamp1792917470. Same18 diagnostics: 16 expected malformed
fixtures (ERR-003), scripts/windows/review-check.ps1: line29: syntax error, and
crates/mct-cli/src/web/spa/spa.css: line1: syntax error (ERR-020 OPEN).
No new reproduction or resolution; source context batches succeeded.


2026-10-25 editor bulk controls implementation: get_indexing_status({}) succeeded
with 1032 files, 18001 symbols, timestamp1792917879. The same18 diagnostics
persist:16 intentional malformed fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error`, and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped context queries succeeded. No parser fix or resolution claimed.


2026-10-25 dashboard reliability audit: get_indexing_status({}) succeeded at
feature/dead-code-editor b61cc7ce971fdc46a61259af95af7fac823ae352 with local work:
1034 files, 18019 symbols, index timestamp 1792921220. The same 18 diagnostics
remain: 16 expected malformed fixtures (ERR-003),
`scripts/windows/review-check.ps1: line 29: syntax error` and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Context/skeleton queries succeeded. No primary reindex, parser fix or resolution
is claimed; those diagnostics do not establish browser styling failure.


2026-10-25 dashboard delivery follow-up: get_indexing_status({format: "toon"})
succeeded on dirty feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352:
1035 files, 18043 symbols, index timestamp 1792925238. Same 18 diagnostics:
16 deliberate malformed fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error` and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped build_context_pack(run, web.rs) succeeded. No new parser probe, primary
reindex or resolution claimed. Official asset generation now succeeds; this does
not resolve the generated CSS parser rejection or prove browser rendering.


2026-10-25 native acceptance recheck: get_indexing_status({}) succeeded on dirty
feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352: 1036 files,
18059 symbols, index timestamp 1792925556. Same 18 diagnostics: 16 deliberately
malformed fixtures (ERR-003 EXPECTED), scripts/windows/review-check.ps1: line 29:
syntax error, and crates/mct-cli/src/web/spa/spa.css: line 1: syntax error
(ERR-020 OPEN). Scoped context/search queries succeeded. No parser probe,
manual primary reindex, parser fix or resolution claimed; runtime socket EPERM
is an independent environmental observation, not attributed to these diagnostics.

2026-10-25 runtime/hook investigation: successful get_indexing_status({}) reports
1036 files / 18072 symbols, timestamp 1792926155, on dirty feature/dead-code-editor
b61cc7ce971fdc46a61259af95af7fac823ae352. Same 18 parse diagnostics (16 intentional
malformed fixtures, PowerShell line 29 and generated CSS line 1); ERR-020 remains
OPEN. Scoped SQLite-hook skeleton/context requests succeeded. No parser fix or
manual primary reindex. Hook socket-command check permits execution; independent
bind/connect still fail EPERM, and sandbox_check reports effective network denial.
Terminal UI is denied by Computer Use; Safari read-only accessibility succeeds.
See lifecycle section 15 for the bounded diagnostic and operator prerequisite.


2026-10-25 storage reliability audit: `get_indexing_status({})` succeeded on
b61cc7ce971fdc46a61259af95af7fac823ae352 plus existing local changes: 1,036 files,
18,081 symbols, index timestamp 1792929587. Same eighteen diagnostics: sixteen
intentional malformed corpus fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error`, and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped context packs and batch lookups succeeded. No parser probe, primary
reindex, exclusion change or resolution claimed. Existing classifications remain;
next parser action is the existing minimized CSS/PowerShell investigation.


2026-10-25 writer-coordination audit: project MCP get_indexing_status reported
1,045 files /18,183 symbols, timestamp1792931650. Same18 parse diagnostics:
16 intentional malformed fixtures (ERR-003), scripts/windows/review-check.ps1:
line29 syntax error and generated web/spa/spa.css:line1 syntax error (ERR-020).
No parser correction claimed. One caller lookup requested nonexistent
crates/mct-index/tests/read_only.rs: `was not found under the indexed project root`;
no source defect inferred; use the actual indexed test paths instead.

2026-10-25 Phase 3 audit: get_indexing_status({}) succeeded, 1049 files,
18240 symbols, timestamp1792935229. Same18 diagnostics as writer phase:16
intentional malformed fixtures(ERR-003), scripts/windows/review-check.ps1 line29
syntax error and web/spa/spa.css line1 syntax error(ERR-020). No parser fix or
primary reindex claimed. Scoped skeleton queries succeeded.

2026-10-25 Phase 4 audit: connected get_indexing_status succeeded with 1,052
files / 18,321 symbols at timestamp 1792937321; it reported the same 18 known
diagnostics (16 malformed corpus fixtures, PowerShell line 29, generated CSS
line 1). Two build_context_pack calls guessed symbols `with_index` and `lookup`
and returned `No symbol named ... found in the index`; these were caller lookup
errors (ERR-021). `list_symbols` followed by lookups for `lookup_exact`,
`validate`, and `check` succeeded. No parser probe or forced primary reindex.


2026-10-25 Phase 5 audit: get_indexing_status({format: "toon"}) succeeded: 1053 files, 18374 symbols, timestamp 1792940148. Same 18 diagnostics: 16 malformed corpus fixtures (ERR-003 EXPECTED), scripts/windows/review-check.ps1 line 29 and generated web/spa/spa.css line 1 (ERR-020 OPEN). Three batch build_context_pack calls incorrectly used name instead of symbol: `invalid args for `build_context_pack`: missing field `symbol`` (ERR-021 caller error). Retrying with symbol succeeded. No parser correction or production reindex claimed.


2026-10-25 Phase6: get_indexing_status({}) succeeded,1054 files/18403 symbols,
timestamp1792963155. Same18 diagnostics:16 intentional malformed fixtures
(ERR-003 EXPECTED), scripts/windows/review-check.ps1 line29 syntax error and
web/spa/spa.css line1 syntax error(ERR-020 OPEN). Three context lookups for
`effective_review`, `save_review`, `decision_validity` returned
`No symbol named ... found in the index`.
Classified ERR-021 caller discovery misses; list_symbols followed by state,
State, state_json, suppresses, set_inner and finding succeeded. No parser fix.


2026-10-25 Phase7: get_indexing_status({}) succeeded with1055files/18429symbols,
timestamp1792964470. Same18 diagnostics:16 intentional malformed corpus fixtures
(ERR-003 EXPECTED), scripts/windows/review-check.ps1:line29 syntax error and
web/spa/spa.css:line1 syntax error(ERR-020 OPEN). Scoped discovery/context
lookups succeeded. No parser fix, forced primary reindex or production action.

Phase7 dashboard follow-up: native Safari exposed rendered false-positive
reevaluation after unchanged reindex, distinct from MCP/parser diagnostics.
Same-server HTTP regression reproduced it; source-only read applicability
with unchanged strict write freshness corrected it. Final native candidate
2c0468eb... verified unchanged FP1/reevaluate0, then modified body FP0/
reevaluate1. Detailed evidence in storage-reliability-verification.md Phase7.

2026-10-25 Phase 8 audit: `get_indexing_status({})` succeeded with 1,056 files /
18,475 symbols, timestamp `1792966895`; 18 parse diagnostics remain the known
set (16 intentional malformed corpus fixtures, PowerShell line 29, generated
dashboard CSS line 1, ERR-020 OPEN). Catalog and scoped source queries succeeded.
No parser fix or forced project reindex was performed. No new parser error ID
was created.

ERR-021 caller-discovery recurrence, 2026-10-25: attempted MCP symbol names
`duplicate`, `duplicate_report`, `run_duplicates`,
`reviewed_duplicate_groups`, `find_dead_code_filters_valid_false_positives`,
and `build_context_pack` in an incorrect path did not resolve. The exact outputs
were “No symbol named … found in the index” or the tool's “try search_symbols”
guidance. Catalog/search discovery then found the indexed duplicate analyzer,
review filter, and CLI `run` definitions. No code was changed based on the
failed lookups; this is caller-side name/path discovery, not a server failure.

2026-10-26 Phase8.1: ERR-021 argument mistake (build_context_pack query vs symbol,
missing field `symbol`) corrected; scoped context calls succeeded. Current
status1060files/18543symbols/timestamp1792997392 retains18diagnostics, ERR-003
expected16 and ERR-020 open2. No parser resolution/production action claimed.
CLI call_one intentionally reindexes; use MCP read-only dispatcher when testing
that retrieval itself does not modify the index. Governance CLI API requests
returned Forbidden; authorized GitHub connector succeeded and created #233.

2026-10-26 Phase9: get_indexing_status({}) succeeded:1063files/18565symbols, timestamp1792999842. Same18 parse diagnostics:16 intentionally malformed fixtures ERR-003 EXPECTED, scripts/windows/review-check.ps1 line29 syntax error and crates/mct-cli/src/web/spa/spa.css line1 syntax error ERR-020 OPEN. No parser correction or forced primary reindex. Connected catalog lacks local find_duplicate_code; use newly built isolated process for acceptance, do not restart installed services.

2026-10-26 Phase9 browser hardening: Safari's inspector reproduced Alpine CSP errors for `activeFile.warning` before asynchronous finding load. Initial activeFile omitted warning; loaded rows supplied it. A new Node regression failed (`undefined !== ''`) before the one-property initialization fix. Security policy unchanged; no eval/inline workaround. Fresh embedded binary/native recheck pending. Also a real stale POST api/comment/update returned409 (27.9ms), retained draft A and saved newer B; native AX evidence redacted.

2026-10-26 Phase9 QA-001 native reproduction and red/green Node evidence recorded; QA-002 first whole-suite watcher failure retained despite isolated4/4pass; QA-003 npm403/fresh RustSec fetch block persists. No protections disabled. Isolated native fixture backup/verify/restore passed in156.250/47.530/76.664ms,57557B; no production restore. ERR-021 absent open_connection symbol was a caller discovery mistake; exact discovered main/background packs succeeded.

2026-10-26 Phase9 final official check PASS1959/0/27 plusfmt/strictClippy/eval. Retain initial watcher failure and ERR-009 backend distinction (kqueue tests versus normalFSEvents). QA-001 remains local-tested/native-followup-pending; fresh audit/npm gates remain blocked. No historical failure erased.

2026-10-26 Phase9.1: status retains known parse limitations; absent
spawn_background_indexer lookup was a caller discovery miss, resolved with
list_symbols and spawn_watcher_with_reconciliation (ERR-021). No parser fix.
QA-00210bounded repetitions4/4eachPASS, historical failure remains OPEN.
New QA-004: expired receive timeout still consumes queued Changed::Nothing,
confirmed deterministic regression red (Ok(Nothing) versus Err(Timeout)).
Deadline priority correction and two unit tests PASS; unrelated to QA-002 until
evidence links them. Capture bounded subprocess stderr on future failures.
Production-only build logs backend=Fsevent; native harness failed to observe
first create within8s with reconciliation600s. This is FAIL in this environment,
not proof of backend-wide corruption or a kqueue result. Preserve ERR-009.
Official npm registry403 persists after escalation; offline official build
returns ENOTCACHED for @alpinejs/csp metadata despite tarballs in cache.
Fresh audit in writable target also fails network after escalation. No integrity
checks, permissions or registry configuration were bypassed.

Phase9.1 native operator evidence (current session, supplied by owner): normal
MCP SHA25685df433b32305cd375b7f473f8e79a4dc14d5c06d69ed1cdc504839b04efa843,
macOS27.0ARM64, backend=Fsevent at2026-10-26T08:55:03Z. All10create/modify/
rename/delete rounds and64file burst PASS, failures0, reconciliation600s.
104observations,median2.144ms,p95151.056ms,variance4052.247ms²; observations
include already-absent/unchanged lookups, not104independent event latencies.
No fallback within the8s observation bounds. This establishes native delivery
for this fixture; agent-side missing events remain environment-specific evidence
and QA-002 is not closed by this success alone. Separately the agent's exact
Fsevent build51a1f6df... with supported60s interval recovers a new file at
60024.849ms (n=1). Initial interval1s harness run was rejected before MCP
startup; corrected validation honors the CLI minimum60, no product limit change.

Phase9.1 new full-check failure retained:1960passed/1failed/27ignored;
index_generations::source_edit_during_parse_aborts_instead_of_publishing_stale_bytes
found0old_a symbols instead of1. Isolated rerun passed. Test fixture allocator
used only SystemTime and create_dir_all: forced repeated nonce reproduces shared
DB paths deterministically. Fixed fixture allocation with process/atomic serial
and atomic create_dir; added repeated-clock regression and seed assertions.
All7generation tests and20repetitions pass. This removes a demonstrated fixture
isolation flaw, but the original run did not log the root, so its exact cause
is not proven. Keep incident QA-005 OPEN pending final evidence/risk review.
Clippy's two unwrap diagnostics in new test setup corrected with assert(is_ok);
no production panic allowance added. Final rerun recorded separately.

Phase9.1 QA-002 reproduction now diagnosed for the second run: child stderr
backend=Fsevent at09:01:10Z. Normal builds replaced sharedtarget/debug MCP
while compiled integration tests were running.1961/1/27,Clippy/evalPASS.
Coordinator corrects validation isolation with a separate target directory.
Original Phase9 incident lacks backend logs and remains unproven. Preserve
this failure; nativeTerminalFSEventsPASS and agentfallbackPASS are distinct.

## 2026-10-26 — Phase 9.1 final verification (Claude takeover)

ERR-022 (new, OPEN): Claude Code reported CONNECT_TIMEOUT after 30000 ms for the
primary mini-consumes-tokens MCP registration at session start. The installed
connector was not restarted. `mct-cli call --tool get_indexing_status` on the
same root succeeded: 1069 files, 18614 symbols, published generation 1,
timestamp 1793007675. The same 18 parse diagnostics remain: 16 intentional
malformed fixtures (ERR-003, EXPECTED), `scripts/windows/review-check.ps1`, and
`crates/mct-cli/src/web/spa/spa.css` (ERR-020, OPEN). Lookups used `mct-cli call`
(build_context_pack, list_symbols, search_symbols) successfully.

QA-002/QA-005 follow-up: the first fully isolated patched gate (target
phase91-isolated, CARGO_BIN_EXE paths verified in the isolated target) passed
1962/0/27 with strict Clippy and evaluation PASS. reindex_delay_e2e 4/4,
index_generations 7/7, background_watcher 12/12, writer_coordination 9/9.
The owner's native FSEvents acceptance on the patched normal MCP binary
839b5c85... passed 10 rounds + 64-file burst. These passes do not establish the
root cause of the original historical Phase 9 incidents; both stay OPEN pending
an owner-approved risk disposition.

## 2026-10-26 — Independent Phase 9.1 audit

ERR-022 (OPEN, mechanism reproduced): the MCP server sends its `initialize`
response only after startup indexing finishes. Primary root, seven installed
servers alive, load average 22-39: `initialize` 100.7 s / 96.7 s (installed
binary), 136.3 s (candidate 839b5c85). Isolated 2000-file fixture, no other
server: 57.6 s cold, 14.1 s unchanged, 22.6 s with 1000 files changed offline.
`scripts/unix/mcp-smoke.sh` on the primary root succeeded (22 tools) after
78.5 s. The client's 30 s CONNECT_TIMEOUT is therefore expected under these
conditions. The installed servers were observed, not stopped.

Audit tool calls: `find_dead_code` with an unknown argument (`bogus`) is
accepted and the argument is silently ignored (caller-facing leniency, not a
server crash). Invalid types, negative offsets, unknown tools, path traversal,
absolute paths outside the root and symlinks resolving outside the root were all
rejected as expected.


## 2026-10-27 — Rust-first Batch 1

- Preserved the dirty `feature/dead-code-editor` tree at base `b61cc7c`.
- ERR-023/D1 reproduced stale symbol/signature attribution in temporary Rust
  source; exact-byte indexed-hash validation now passes the focused regressions.
- ERR-024/D7 reproduced the enabled ignored failure; full and path indexing
  remove only the obsolete diagnostic and preserve the unrelated error.
- D8 now checks public paths/names/ranges during indexing, publication, rollback
  and recovery. No production concurrency behavior was changed.
- ERR-021 caller/fixture mistakes corrected: context `name` to `symbol`, calls
  `symbol` to `function`, and CLI `call --tool` syntax. Initial D8 assumed
  ranges corrected from fixture/source evidence. No product fix inferred.
- Existing query-cache test expected edited source before reindexing; updated
  to the approved D1 contract: metadata stays, source omitted, source restored
  after reindex. The cache-hit assertion is preserved.
- ERR-025/D3 remains reproducibly failing when explicitly run with `--ignored`.
- ERR-026 reproduced with a real CLI/MCP handler on a disposable Rust project;
  recorded as a name-based liveness limitation. No detector change authorized.
- No production database read/reindex, installed service restart or deployment.

## 2026-10-27 — ERR-022 recurrence

- Claude Code session on feature/dead-code-editor (b61cc7c) reported `mini-consumes-tokens` CONNECT_TIMEOUT after 30000 ms. Two `mct-mcp-server` processes were alive, load average 3-5; installed binary from 2026-10-19. Status stays OPEN; no connector restart, reinstall or index change.

## 2026-10-28 — Cargo MCP root repair

The local `cargo-deps` registration pointed to the missing
`vendor/mct-pilot/smallvec-1.16.1` path. The Cargo registry cache contains
`smallvec 1.16.1`, but starting the server against that read-only cache failed
while creating `.mct-index` (`Operation not permitted`). Copied the cached crate
to `/private/tmp/mct-cargo-deps/smallvec-1.16.1` and updated the ignored local
Codex config. Verified `initialize`, `tools/list` (22 tools), and
`get_indexing_status` (5 files / 291 symbols). ERR-027 resolved for this local
session; the copy must be recreated if `/private/tmp` is cleared.
