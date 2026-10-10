# Handoff

The coordinator records the current continuation here; workers checkpoint in their own task branch. See [checkpoint requirements](../PROJECT_MANAGER.md#checkpoints-and-handoffs).

## TASK-PM-001

Previous agent: PM-01 / Codex
Status: REVIEW
Branch: `codex/rules-navigation`
Worktree name: `mct-rules-navigation`
Base commit: `2488ec1e9e9d8cfdfc5c9f7b3ffa50cd141ce033`
Latest prior implementation commit: `25673a5`; resolve the current branch HEAD through Git for subsequent documentation changes.

### Objective

Implement the requested English coordination process and durable records in the existing documentation PR #96.

### Completed

Model and effort routing, provider-specific commands, quota/context guidance, and project-management process with seven operational records are documented. Shared-state ownership and worker checkpoints are separated. AGENTS.md and CLAUDE.md link through rules.md to the process.

### Current state

Documentation proposed on the task branch; no Claude process is running for this task. Primary main checkout was clean at startup. PR #96 was OPEN when checked on 2026-10-01. Refresh state before continuing.

### Remaining work

Maintainer review, any resulting corrections, authorized integration, and post-merge state update. Other worktrees have unverified ownership and are outside this task.

### Checks

See [Review Report](REPORT.md) for documentation checks. No runtime tests or quota-failure simulation are claimed. MCP/index health was not checked because source exploration was unnecessary.

### Known problems and decisions

No automated orchestrator, locks, failover, or provider-to-provider messaging is installed. See [Coordination Decisions](DECISIONS.md). Preserve all unrelated worktrees and changes.

### Files to inspect first

`rules.md`, `PROJECT_MANAGER.md`, `project-management/TASKS.md`, and the current PR diff.

### Exact next step

Check Git status/HEAD and PR #96's latest revision. Review the documentation diff, resolve findings, and integrate only when accepted and authorized. Record the merge commit before marking the task DONE.

## TASK-LIB-001

Previous agent: Claude Code session
Status: REVIEW
Branch: `claude/library-onboarding-audit`
Worktree name: `mini-consumes-tokens-claude-library-onboarding-audit`
Base commit: `1ad997242c5629df14d77acb1f5c36baec202140`
Latest verified implementation commit: `9109edd`; resolve the branch HEAD through Git.

### Completed

- `4a59515`: cherry-pick of `690f5b0` (manifest-format modules, JS/TS walker modules, `docs/01-architecture/library-support.md`). Its blobs match `codex/library-maintainability` (`9dfa1cc`).
- `a128332`: `60cd44e` layout, one `mod.rs` per library, no per-library READMEs; `corpus-report.sh/.ps1` also run `--lib libraries::`.
- `5ec49ba`: `crates/mct-languages` is the single `build_registry`; CLI uses it, `mct-mcp-server::registry` re-exports it; scripts and docs updated.
- `9109edd`: Bash/PowerShell grammar rejections recorded in `internal/corpus-progress.md`.

### Checks

`scripts/unix/check.sh` on `9109edd` (tree dirty only with this record): fmt ok; tests 1199 passed, 0 failed, 24 ignored; CI clippy ok; mct-eval no regressions. `new-language-check.sh lua` ok; `corpus-report.sh css` 18 passed, `go` 17 passed. Windows scripts were edited but not run (no Windows host). Fuzz workspaces not rebuilt (they do not depend on the registry).

### Known problems and decisions

- The two remaining index parse failures (`scripts/unix/pr-body.sh`, `scripts/windows/pr-status.ps1`) are upstream grammar rejections, not yet filed upstream.
- The 16 `tests/corpus/malformed/` rejections in `get_indexing_status` are intentional. `.mctignore` is local-only in this repository (`.gitignore`), so the exclusion `crates/*/tests/corpus/malformed/` was not committed.
- `errors/` lives on unmerged `chore/mcp-error-registry`; not duplicated here.

### Exact next step

Maintainer review of `1ad9972..claude/library-onboarding-audit`; push and open a PR only when authorized.
