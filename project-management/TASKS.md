# Tasks

The coordinator owns this board. Statuses: TODO, READY, IN_PROGRESS, REVIEW, BLOCKED, DONE. See [Project Manager](../PROJECT_MANAGER.md#tasks-ownership-and-isolation). Only accepted, verified integration earns DONE.

## TASK-PM-001

Status: DONE
Owner: Codex coordinator
Reviewer: Repository maintainer; Codex performs the documentation consistency review
Branch: `codex/rules-navigation`
Worktree: `mct-rules-navigation` (resolve local path using Git)
Base commit: `2488ec1e9e9d8cfdfc5c9f7b3ffa50cd141ce033`
Dependencies: Existing shared-rule changes in PR #96; no code-task dependency
Priority: High
Risk: Documentation policy; no runtime/code change
Model/provider: Current Codex session; no new model selection or Claude dispatch

### Goal

Make the requested multiagent process recoverable from repository records while keeping technical/model policy centralized and all new repository content in English.

### Scope

`AGENTS.md`, `CLAUDE.md`, `rules.md`, `PROJECT_MANAGER.md`, and `project-management/*.md`. No source-code changes or unrelated worktree cleanup.

### Acceptance criteria

- [x] Shared technical, model, effort, billing, and command policy remains in rules.md.
- [x] English project-manager guide covers ownership, isolation, dispatch, checkpointing, recovery, review, completeness, and integration.
- [x] Seven compact state files exist and reference the current task.
- [x] Shared state has one writer; worker checkpoints use separate task branches.
- [x] Single-client contributors can follow the workflow.
- [ ] Maintainer accepts the latest PR revision.
- [ ] Integration into main is verified and recorded.

### Required checks

- Documentation consistency, relative links, and `git diff --check` before publication; record outcomes in REPORT.
- Runtime tests: not applicable to this documentation-only task. No failure/recovery simulation is claimed.

### Checkpoint

See [Handoff](HANDOFF.md#task-pm-001). Prior routing/recovery implementation commit: `25673a5`. Obtain the latest documentation commit from this branch; avoid a self-referential commit hash.

### Result

Merged in [PR #96](https://github.com/Zubiarka8/mini-consumes-tokens/pull/96) as `adbc43a` (2026-10-01).

## TASK-LANG-001

Status: DONE
Owner: Claude Code session (requested by the repository maintainer)
Reviewer: Repository maintainer
Branch: `docs/lang-crate-base-and-fuzz`
Worktree: main checkout (no separate worktree)
Base commit: `5956a7611b344c4ac01d3f1613b6589a420e6908`
Dependencies: None
Priority: Medium
Risk: Low; adds tests, fuzz harnesses, CI matrix entries and check-script strictness. No parser behavior change.
Model/provider: Single session; no cross-provider dispatch

### Goal

Give every `mct-lang-*` crate the same base: a syntax-error test, a fuzz harness, a CI fuzz-smoke entry and the wiring checked by `new-language-check`. Provide a shared base document for new languages.

### Scope

- `crates/mct-lang-md/tests/parse.rs`: syntax-error test (the parser already returns `ParseError::Syntax`; the test was missing).
- `crates/mct-lang-{css,html,kotlin,md,xaml,xml}/fuzz/`: fuzz harnesses copied from the existing pattern, with `Cargo.lock`.
- `.github/workflows/ci.yml`: fuzz-smoke matrix entries for the six crates above.
- `scripts/{unix,windows}/new-language-check.*`: fuzz harness, fuzz target and CI matrix entry become required.
- `docs/06-templates/lang-crate-base.md` and a pointer in `CONTRIBUTING.md`.

Exclusions: parser logic, README and `internal/checklist.md` rows (warnings, not in scope), fuzzing campaigns.

### Acceptance criteria

- [x] Every `mct-lang-*` crate passes `new-language-check.sh <suffix> --no-tests` with exit 0.
- [x] `cargo test -p mct-lang-md` passes, including the new syntax-error test.
- [x] Each new fuzz harness compiles (`cargo check --bins` in its `fuzz/` directory).
- [x] `scripts/unix/check.sh` passes.
- [x] Pull request opened from this branch against `main` (#157).

### Required checks

- `scripts/unix/new-language-check.sh <suffix> --no-tests` for all crates.
- `cargo test -p mct-lang-md`; `scripts/unix/check.sh`.
- `cargo check --bins` in each new `fuzz/` directory.

### Checkpoint

Work was first preserved in commit `2d3fd3b` on `codex/preserve-local-work-20261009`; this branch carries the same changes as scoped commits.

### Result

Merged in https://github.com/Zubiarka8/mini-consumes-tokens/pull/157 as `7558113` (2026-10-09).

## TASK-LIB-001

Status: DONE
Owner: Claude Code session (requested by the repository maintainer)
Reviewer: Repository maintainer
Branch: `claude/library-onboarding-audit`
Worktree: `mini-consumes-tokens-claude-library-onboarding-audit` (resolve the local path using Git)
Base commit: `1ad997242c5629df14d77acb1f5c36baec202140`
Dependencies: Recovers `codex/library-support-layout` (`60cd44e`, which contains `codex/library-maintainability` `9dfa1cc` rebased as `690f5b0`). Carries the `errors/` registry from PR #168 (`chore/mcp-error-registry`, cherry-picked as `57f834e`); rebase once #167 or #168 merge.
Priority: Medium
Risk: Medium; new workspace crate `mct-languages`. No change to the shipped language set, `LanguageParser`, the schema or MCP signatures.
Model/provider: Single session; no cross-provider dispatch

### Goal

Audit and improve the process for adding and maintaining a language or library, from wiring through indexing, tests, diagnostics and release, by fixing root causes.

### Scope

Recovered: the manifest-format modules and JS/TS walker split (`690f5b0`), and the `src/libraries/<name>/` test layout (`60cd44e`, without its five duplicated READMEs). New: `crates/mct-languages` as the single language registry; `corpus-report` also runs `--lib libraries::`; grammar rejections recorded in `internal/corpus-progress.md`. Exclusions: `website/`, parser behavior, the user's local `.mctignore`.

### Acceptance criteria

- [x] Both reference branches compared with main; only the missing, useful changes recovered.
- [x] One registration point per language; `new-language-check` passes for an existing language.
- [x] Library tests moved out of `tests/corpus.rs` still run in `corpus-report`.
- [x] Guard test fails when a language is left unregistered (checked by removing Lua temporarily).
- [x] `scripts/unix/check.sh` passes on `9109edd` (see Handoff).
- [x] Windows CI "PowerShell scripts" step passes (run 38056059719).
- [ ] Maintainer review and integration.

### Required checks

`scripts/unix/check.sh` (fmt, workspace tests, CI clippy, mct-eval); `new-language-check.sh lua`; `corpus-report.sh css` and `go`.

### Checkpoint

See [Handoff](HANDOFF.md#task-lib-001).

### Result

Merged in https://github.com/Zubiarka8/mini-consumes-tokens/pull/170 as `a94d101` (2026-10-10), including the ERR-010 fix. PRs #167 and #168 were closed as exact patch duplicates carried by #170. ERR-010 was re-verified through a live MCP `find_references` call after a forced reindex (`errors/HISTORY.md`).

## TASK-WEB-169

Status: DONE (integration verified 2026-10-10)
Owner: Codex coordinator; integration continued by a Claude Code session
Reviewer: Repository maintainer
Branch: `docs/fumadocs-site` (PR #169), updated from `codex/reconcile-website-169`
Worktree: `mct-reconcile-website-169` (resolve the local path using Git)
Base commit: `4d7d2ea5993bf8e3d5fad45fa10dad2f285e03d2`
Dependencies: None
Priority: Medium
Risk: Medium; website dependencies (React Three Fiber, i18next, Motion, Playwright) and the Pages workflow. No Rust change.
Model/provider: Codex worker, Claude Code integration review

### Goal

Ship one website: the Fumadocs reference manual from PR #169 plus the EN/ES Code Atlas landing, instead of two competing sites.

### Acceptance criteria

- The reconciled #169 candidate preserved all 27 reference documentation files. Subsequent maintainer-merged #174 intentionally updated architecture, extending, libraries and limitations; the other 23 remain unchanged. This accepted follow-up supersedes byte-for-byte preservation of those four files.
- Types, formatting, translation resources, prefixed and default builds, export link checks and WebKit browser tests pass.
- PR #169 has no merge conflict with `main` and its CI is green.

### Result

`405fea8` reconciles the landing with the manual; `6b1bede` merges `main` (CHANGELOG conflict resolved keeping both entries) and was pushed to PR #169. Local checks on `6b1bede`: `npm ci` (0 vulnerabilities), types, format, 102 translation keys, prefixed build, 29 HTML / 1127 links export check, 18 WebKit tests passed. PR #169 merged as `d016df971edac5929efeab67df08f10c72f2dd2f`. Follow-ups [#171](https://github.com/Zubiarka8/mini-consumes-tokens/pull/171), [#173](https://github.com/Zubiarka8/mini-consumes-tokens/pull/173) and [#174](https://github.com/Zubiarka8/mini-consumes-tokens/pull/174) merged as `d148df9`, `d12127b` and `fca28b1`; all four merge commits are ancestors of current main `eee98e9ed0f1467dac32fe172cb90eb846628f83`. #171 adds atlas work, #173 moves details into a dialog, and #174 aligns four manual pages with the language registry/layout.

[Docs run 38083267842](https://github.com/Zubiarka8/mini-consumes-tokens/actions/runs/38083267842), on `fca28b1`, completed successfully: types, format, resources, static build, export links, artifact upload and Pages deployment. Main's website/workflow tree matches that tested revision (`git diff --exit-code fca28b1 origin/main -- website .github/workflows/docs.yml` passed). PR #169's checks show 42 successful jobs and one expected skipped PR deployment, with no failing or pending checks. Default/prefixed build and 18 WebKit results on `6b1bede` are historical local evidence; #173 reports 21 WebKit passes in its PR, not independently rerun here. No runtime/browser tests were added or executed for this bookkeeping change.

## New task entry template

```text
TASK-ID / Status / Owner / Reviewer:
Branch / Worktree name / Full base commit:
Dependencies / Priority / Risk / Provider-model-effort:
Goal:
Allowed write scope / exclusions:
Acceptance criteria:
Required checks and why:
Checkpoint path / latest known implementation commit:
Result / PR / integration commit:
```

## TASK-DOCS-MCP-PROSE

Status: REVIEW. Candidate: `docs/semantic-search-prose-limits` at `62c677f`.
Owner: prior Claude docs worker/coordinator; recovery validates the combined site.
Base: `a3e10b0f450a93107061152feb862a1d242820a1`.
Scope: tools, client count, prose limits and task checkpoint.
Acceptance: accurate 20 defined / 19 default tools, conditional select_tools,
semantic_search feature behavior, text-format list budget, bounded prose search.
Evidence: checkpoint TASK-DOCS-MCP-PROSE.md; combined site checks in REPORT.md.
Published previously; no PR or main integration claimed by recovery.

## TASK-MD-ALIAS-135

Status: REVIEW. Candidate: `codex/recover-alias-135-final` at `d0fe58f`.
Owner: Codex recovery coordinator; independent alias reviewer.
Base: `a3e10b0f450a93107061152feb862a1d242820a1`.
Scope: parser aliases, generic storage/migration11, relation view, tests, ADR005,
and related docs. No MCP signature changes.
Acceptance: exact aliases; titles excluded; ambiguity preserved; path links exact;
incremental/full consistency; one-time migration; bounded query cost.
Evidence: checkpoints/TASK-MD-ALIAS-135.md and REPORT.md.
Remaining: record approval on #135 and verify baseline watcher limitation on CI.
No alias publication or main integration.

## TASK-CSS-NESTING-115

Status: REVIEW. Candidate: `codex/review-css-115-final` at `a361b2`.
Owner: CSS recovery worker; coordinator review.
Base: `a3e10b0f450a93107061152feb862a1d242820a1`.
Scope: CSS walker, focused/corpus/library tests, error registry and docs.
Acceptance: AST ampersand replacement; full products; implicit relative parent;
complex-parent grouping; 256 per-rule limit with marked unexpanded fallback.
Evidence: 71 pass / 0 fail / 1 ignore in crate; checkpoints/TASK-CSS-NESTING-115.md;
combined workspace and website evidence in REPORT.md.
Remaining: baseline watcher CI verification, authorized feature publication.

## TASK-PR-POLICY

Status: REVIEW. Candidate: `docs/pr-integration-policy-draft` at `14a2586`.
Owner: prior policy worker; independent Codex policy reviewer.
Base: `a3e10b0f450a93107061152feb862a1d242820a1`.
Scope: PROJECT_MANAGER.md and task checkpoint.
Acceptance: user-approved integration policy separate from GitHub enforcement;
merge commits, PRs for every change, review, no auto-merge/direct main pushes;
no universal red-before-change test mandate.
Evidence: diff review, script inspection, diff check; GitHub refresh unavailable.
No policy publication or main integration.

## TASK-HOOK-RETIREMENT

Status: REVIEW. Candidate: `codex/retire-dogfooding-hook` at `6a0daf3`.
Owner/reviewer: Codex coordinator.
Scope: only Dogfooding registration, retired entry point and related hook docs.
Acceptance: remove registration and ensure stale calls never block source reads.
Evidence: JSON parse, silent successful invocation, diff check and live source
read after retirement. SQLite guard remains active. No publication.
