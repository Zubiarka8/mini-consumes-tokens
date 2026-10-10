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

None. PR #96 merged as `adbc43a`; the task is DONE.

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
- `errors/` comes from PR #168 (cherry-picked as `57f834e`, see the follow-up below); not duplicated.

### Exact next step

Maintainer review of `1ad9972..claude/library-onboarding-audit`; push and open a PR only when authorized.

### Follow-up session (2026-10-10)

- `57f834e`: PR #168's `errors/` registry cherry-picked (`-x e9280d8`); `283cbc5` updates it (ERR-001/002 root causes, ERR-003 exclusion note, ERR-010). PR #167 is identical to `4a59515`. Rebase after either merges drops the duplicate patch.
- `ac5227d`: ADR-004 grammar version policy; three requirements moved to the locked x.y.z; `new-language-check` enforces it.
- `dab3059`: ignored regression test for ERR-010; `46d722f` lists it in ISSUES_PENDING.md (required by `repo_ledgers`).
- `1a064b1`: Windows CI step parses every `scripts/windows/*.ps1` and runs `new-language-check.ps1 lua --no-tests` and `corpus-report.ps1 css`. Not run yet: it needs a push or PR.
- Checks on `46d722f`, clean tree: `check.sh` fmt ok, 1199 passed / 0 failed / 25 ignored, CI clippy ok, eval no regressions. `new-language-check.sh <lang> --no-tests` exit 0 for all 17 crates; exit 1 with the old `"0.25"` form. `corpus-report.sh` ok for rust, bash, php, powershell, js-ts, python. `cargo metadata --locked` ok. `cargo package -p mct-languages` fails only on unpublished `mct-core`, same as `mct-cli`.

### Draft PR session (2026-10-10)

- `4c2cbd3`: CI `build`/`test`/`clippy` and the quality report pass `--locked`; local runs of the three commands exit 0, and an unsatisfiable requirement fails `cargo metadata --locked` (exit 101).
- Pushed; draft [PR #170](https://github.com/Zubiarka8/mini-consumes-tokens/pull/170). First run 38055596733: Windows "PowerShell scripts" failed on `scripts/windows/pr-body.ps1:22` (`"$checkRel:"`, a drive-qualified variable). Fixed in `418e295`; `pr-status.ps1` parsed natively, so ERR-002 is grammar-only (`29dcdff`).
- Run 38056059719 on `29dcdff`: 41/41 checks pass. Windows step: every `scripts/windows/*.ps1` parses, `new-language-check.ps1 lua --no-tests` all ok, `corpus-report.ps1 css` 18 passed, 0 failed.
- ERR-010 was still OPEN at this point (fixed below).

### ERR-010 fix (2026-10-10)

- `b3a1490`: the Rust walker records a path in value position as `References` with call-path evidence; pinning test un-ignored, `by_path` became a positive test (`module: other`); Rust corpus snapshot re-blessed (+73 references, each reviewed as a real value use). `d92d4f6`: ERR-010 RESOLVED, ISSUES_PENDING entry 9 closed.
- Checks on `d92d4f6`, clean tree: `check.sh` 1201 passed, 0 failed, 24 ignored; CI clippy ok; eval no regressions. `mct-cli dead-code --language rust` on the worktree: two false candidates gone (`default_suite`, `Rules`).
- Both commits revert cleanly on their own if the maintainer prefers to integrate with ERR-010 documented.

Closed: PR #170 merged as `a94d101` (2026-10-10). No package publication. Live MCP `find_references` recheck after the installed server is rebuilt (`scripts/unix/reinstall.sh --reindex`).

## TASK-WEB-169

Status: REVIEW
Branch: `docs/fumadocs-site` (PR #169), from `codex/reconcile-website-169`
Latest commit: `6b1bede`

### Completed

- `405fea8`: EN/ES Code Atlas landing with an optional deferred 3D atlas next to the unchanged reference manual; strict types fixed for the Fiber JSX augmentation; export, resource and WebKit checks; the Pages workflow runs format, resource and export checks.
- `6b1bede`: merge of `main`; the only conflict, `CHANGELOG.md`, keeps both entries.

### Checks

On `6b1bede`: `npm ci --offline` (0 vulnerabilities), `types:check`, `format:check`, `test:resources` (102 keys), prefixed build, `test:export` (29 HTML, 1127 links), `test:browser` (18 WebKit tests). No Chromium run, screen-reader or device audit, or Pages deployment.

### Exact next step

Wait for PR #169 CI, then maintainer merge. Merging deploys GitHub Pages.
