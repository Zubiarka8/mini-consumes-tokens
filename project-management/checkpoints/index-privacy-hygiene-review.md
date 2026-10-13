# Index privacy and hygiene — independent review

Date: 2026-10-12
Reviewer: Codex
Status: REVIEW; APPROVED after scoped corrections, not integrated
Branch: `feat/index-privacy-hygiene`
Worktree name: `mini-consumes-tokens-feat-index-privacy-hygiene`
Base / latest commit: `32f8a79ccfcd09c3d790e43edc284d7602e9737b`
Reviewed implementation and corrective tests are committed in this task branch as `fix(index): enforce exclusion privacy on every MCP query`. The task remains in REVIEW pending maintainer review and integration. Push and PR creation were requested after review.

## Scope and evidence

Git's worktree list identified the existing task worktree. Its status confirmed the required branch and uncommitted index/MCP/tests/documentation changes. The preserved `feat/landing-scroll-tour` checkout was not used for implementation or review source lookups. No Claude report was available in the task records; acceptance is based on the actual diff and independently executed checks.

Read repository rules, project-manager instructions and current coordination records. Those records predate this assignment; this task-specific checkpoint records the current review without editing the seven shared boards. Used the project-local code-review skill. Targeted code exploration used a locally launched project MCP binary with the confirmed worktree root. The initially connected MCP did not identify its root, so its graph was not used for source review. ERR-012 records the resolved local startup sandbox failure. The task worktree's index health showed only the 16 expected malformed corpus fixtures (ERR-003).

Privacy fixtures use temporary projects and synthetic symbols, Markdown prose, dependency names and malformed source. A legitimate public source file remains indexable. No external SQLite tools, network suites, downloaded dependencies, downloaded models or provider benchmarks were used. Existing real private content was not searched for or read.

## Findings and corrections

### PRIV-001 — original stale-row defect, confirmed

After a valid ignore-rule edit, the existing index still contains the private symbol before any reindex. The strengthened MCP test asserts this state directly, then verifies that the actual tool boundary removes it. The server's stdio test exercises `tools/call` with the watcher disabled. Claude's per-call generation check fixes the normal stale-row defect. Rules added, removed, imported from `.gitignore`, and reloaded through the watcher's clone are covered.

### PRIV-002 — major, unreadable rule file reintroduces private content, corrected

The preexisting opt-in reader treats read errors as empty rules. The new per-call path activates a full reindex after such a read, so invalid UTF-8 in `.mctignore` produced a successful `find_symbol` response containing `mct_private_sentinel_7f3a` from `secret_area/notes.py`. The added regression failed before the correction. This is an omission in the new query boundary using an existing permissive reader; it is not a new parser defect.

Added a checked internal reload that preserves previous rules and propagates non-missing read errors through indexing and the MCP boundary. The watcher's existing boolean reload retains prior rules and keeps the existing diagnostic. Missing opt-in files retain their existing removal semantics. Repeated failures reject queries; restoring valid rules restores service. Both `.mctignore` and imported `.gitignore` failures are covered. Documented this behavior in COMMANDS.md. No published signature changed.

### PRIV-003 — major, explicit exclusion loses to unreadable-path retention, corrected

The existing full-index sweep retains old rows beneath unreadable parents. With `locked/private.py` already indexed, the rule `locked/private.py` and an unreadable `locked/` parent caused CLI `reindex --force` to report zero removals, and `dead-code` still exported `mct_private_sentinel_7f3a`. The index regression also failed before the correction. The retention logic predates Claude's diff, but is used by its new sweep and prevents that correction from meeting its privacy contract.

Explicit exclusions now take precedence when sweeping source rows, independent index issues and dependency rows. Read failures for legitimate paths remain visible and preserve their previous retention behavior. Index/MCP tests cover the excluded symbol, relation, parse issue and manifest with a legitimate control. The CLI reproduction now reports one removal and exports only the control, and removing the rule makes the private fixture indexable again.

## Files changed by Codex

- `COMMANDS.md`: document rejection of unreadable rule files.
- `crates/mct-index/src/exclude.rs`: checked reload and preservation of prior rules.
- `crates/mct-index/src/lib.rs`: propagate checked reload failures at the new boundary.
- `crates/mct-index/src/indexer.rs`: checked reload and exclusion precedence in the three sweeps.
- `crates/mct-index/tests/exclude_reload.rs`: unreadable-parent regression.
- `crates/mct-mcp-server/tests/exclusion_privacy.rs`: stale-row precondition, imported-rule edit, unreadable rules and unreadable-parent coverage.
- `errors/REGISTRY.md`, `errors/HISTORY.md`: required environment-error record, ERR-012.
- This checkpoint.

Claude's changes in README.md and the server dispatch guards remain in the candidate. No global exclusions were added for `.agents/`, Markdown, documentation, code or hidden paths. No schema, LanguageParser or published MCP signature changed.

## Executed verification

- `cargo test --offline --locked -p mct-index --test exclude_reload -p mct-mcp-server --test exclusion_privacy`: original 7 + 3 tests passed. After both production fixes, 8 + 4 passed; later added MCP coverage is included in the final server check below.
- `cargo test --offline --locked -p mct-mcp-server --test exclusion_privacy an_unreadable_rule_file`: failed before PRIV-002 correction with a successful response exposing the synthetic private path; passed in the subsequent targeted/full runs.
- `cargo test --offline --locked -p mct-index --test exclude_reload an_explicit_exclusion`: failed before PRIV-003 correction; passed in the subsequent targeted/full runs.
- `cargo fmt --all`: applied formatting only to the changed tests. Final workspace formatting check passed.
- `CARGO_NET_OFFLINE=true scripts/unix/check.sh -p mct-index`: final source/behavior run, fmt passed; 238 passed, 0 failed, 2 ignored. Clippy rejected the new test's non-octal permission literal. Changed it to `0o0`, then `CARGO_NET_OFFLINE=true scripts/unix/check.sh -p mct-index --only clippy` passed with no warnings.
- `CARGO_NET_OFFLINE=true scripts/unix/check.sh -p mct-mcp-server`: final fmt passed; 279 passed, 0 failed, 1 ignored; CI clippy with all targets/features passed with no warnings. All six privacy tests passed, including the actual server over stdio and the Unix permission case. Lexical cache and batch are relevant dispatch routes and are included in the tests.
- `cargo build --offline --locked -p mct-cli`: passed before the CLI reproduction and again with the corrected sweep.
- Temporary synthetic CLI runner: real `init`, `reindex --force`, and `dead-code --output <temporary CSV>` operations. Initially reproduced PRIV-003; after correction the private row was removed, the public control remained, and removing the ignore rule reintroduced the fixture. Passed. The first runner attempt read the command summary instead of the CSV, and its precondition failed; corrected it to use the documented `--output` contract before reproducing the defect.
- `CARGO_NET_OFFLINE=true scripts/unix/check.sh --only eval`: passed, no regressions, accuracy 1.000, catalog approximately 8,541 tokens. This is the default local fixture/tool evaluation, not a model benchmark.
- `git diff --check`: passed.

Early crate checks ran concurrently and share fixed log filenames; their printed totals are not relied upon. The final crate checks above ran separately and supply the acceptance evidence. The initial local index startup and script-log write failures were sandbox restrictions, resolved by authorized native execution. The two red regressions and the corrected permission-literal lint are the only change-related failures recorded here.

## Coverage limits and next action

Verified on macOS. Permission-denial regressions are Unix-only and explicitly report when a privileged user prevents the denial precondition; they executed here. No Windows/Linux runtime run or concurrent filesystem-edit stress test was performed. Semantic/vector searches with a real embedding model were not exercised; no model was installed. CLI refresh remains explicitly manual according to its documented contract. Full workspace tests were not rerun; affected crate checks and the shared fixture quality gate passed.

The new guarantees apply at the real MCP dispatcher and read-only/batch dispatcher. Existing raw library queries and direct Rust tool-method calls still depend on callers applying the boundary; the actual server paths were tested. No transversal contract change was needed.

Remaining action: maintainer review and integration of the PR. Keep both worktrees and all local files intact. The preexisting untracked `internal/claude-hooks/__pycache__/` remains untouched. No merge was authorized.
