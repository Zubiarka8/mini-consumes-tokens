# Tasks

The coordinator owns this board. Statuses: TODO, READY, IN_PROGRESS, REVIEW, BLOCKED, DONE. See [Project Manager](../PROJECT_MANAGER.md#tasks-ownership-and-isolation). Only accepted, verified integration earns DONE.

## TASK-PM-001

Status: REVIEW
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

Proposed in [PR #96](https://github.com/Zubiarka8/mini-consumes-tokens/pull/96); awaiting maintainer acceptance and verified integration.

## TASK-LANG-001

Status: REVIEW
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
- [ ] Pull request opened from this branch against `main`.

### Required checks

- `scripts/unix/new-language-check.sh <suffix> --no-tests` for all crates.
- `cargo test -p mct-lang-md`; `scripts/unix/check.sh`.
- `cargo check --bins` in each new `fuzz/` directory.

### Checkpoint

Work was first preserved in commit `2d3fd3b` on `codex/preserve-local-work-20261009`; this branch carries the same changes as scoped commits.

### Result

Pending: PR from this branch to `main`.

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
