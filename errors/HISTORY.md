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
