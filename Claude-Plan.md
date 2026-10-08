# Remediation plan

Source: `GPT-Report.md`, audited commit `1880048dd8274b0b3a7f82b07bedd3219bde5ca1`.
Before starting a task from this plan, read [rules.md](rules.md) for shared engineering constraints. Use your available editor or assistant; no particular AI provider, model, or subscription is required. Model names in completed-session records describe historical audit runs and are not requirements for contributors.

## Organization

Each agent works in its own worktree and branch. Initial maximum: three agents. Each branch delivers one reviewable fix and a proposed PR. The branches start from the audited commit, which belongs to the work for [PR #90](https://github.com/Zubiarka8/mini-consumes-tokens/pull/90). Do not open them directly against `main` until #90 is integrated, or prepare stacked PRs against a published base that contains that exact starting point. Do not publish commits belonging to other work in the base.

The task briefs reference the [audit rules](internal/claude-audit/00-rules.md). Read the applicable repository rules and task brief in the correct worktree. Historical client-specific launch details do not require contributors to use that client.

| Agent | Branch | Prompt | Timing |
|---|---|---|---|
| 01 — Exclusions | `codex/audit-exclusions` | [F03](internal/claude-audit/01-exclusions.md) | First batch |
| 02 — Lua | `codex/audit-lua` | [F04](internal/claude-audit/02-lua.md) | First batch |
| 03 — Dead code | `codex/audit-dead-code` | [F07](internal/claude-audit/03-dead-code.md) | First batch |
| 04 — Relations | `codex/audit-relations` | [F02](internal/claude-audit/04-relations.md) | After 01 and 03; update the base |
| 05 — Obsidian | `codex/audit-markdown` | [F05/F06](internal/claude-audit/05-markdown.md) | After 04; update the base |
| 06 — Configuration and docs | `codex/audit-config-docs` | [F01/F09](internal/claude-audit/06-configuration.md) | After 02; may overlap with 04 |
| 07 — Duplication | `codex/audit-shared-parsers` | [F08](internal/claude-audit/07-duplication.md) | After 01–05 stabilize |
| 08 — Quality and risks | `codex/audit-quality-gates` | [R01–R04 and metrics](internal/claude-audit/08-quality.md) | On the integrated changes |

Tasks 04 and 05 may require contract or schema changes. `rules.md` requires opening an issue first. The agent must define the design and check this requirement before implementing cross-cutting work; it is not authorized to bypass it. Compatible fixes may proceed.

### First batch merged (PR #91)

These sessions were launched on September 29 with `claude-opus-5-5` and `--effort medium`. All three delivered a local commit, which was then combined and merged into `main` as [PR #91](https://github.com/Zubiarka8/mini-consumes-tokens/pull/91) on 2026-09-30.

| Delivery | Commit | Recorded tests | Status |
|---|---|---|---|
| 01 — Exclusions | `52aa751` | 769 passed, 0 failed, 17 ignored | Merged in #91 |
| 02 — Lua | `39dd255` | 766 passed, 0 failed, 17 ignored | Merged in #91 |
| 03 — Dead code | `331218e` | 773 passed, 0 failed, 17 ignored | Merged in #91; global name-based resolution remains a limitation |

Clippy passes, and `mct-eval` records accuracy 1.000 with no regressions on all three branches. The agents used `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk` to work around a local linker incompatibility with the default SDK. They did not change global configuration. These results apply to each branch individually.

| Agent | Claude session | Worktree |
|---|---|---|
| 01 | `2114f805` | `/Users/arkaitz/.codex/worktrees/audit-exclusions/mini-consumes-tokens` |
| 02 | `b9081107` | `/Users/arkaitz/.codex/worktrees/audit-lua/mini-consumes-tokens` |
| 03 | `3eb8179a` | `/Users/arkaitz/.codex/worktrees/audit-dead-code/mini-consumes-tokens` |

To view current status, run `claude agents`. To join a session, for example, run `claude attach 2114f805`. The other five tasks are prepared and require a later launch on the specified base; their start has not been scheduled automatically.

Later batches are prepared, not started merely because their prompts exist.

## Efficiency

- Run a separate MCP server for each worktree, with `--root` pointing to that worktree and its own index. Do not reuse a configuration that points to the original checkout.
- Keep context minimal: shared rules and assigned task; consult only the relevant report section. Do not repeat a general audit for each agent.
- Use `batch` for known queries, `build_context_pack` with a path/language, and `toon` listings; never query the index through external SQL.
- Do not launch additional subagents in each session. Allow at most three builds; each worktree keeps its own `target/` to avoid shared locks and logs.
- Add one meaningful regression test per behavior, then run crate tests and appropriate CI checks. Do not rerun the full suite after every edit.
- Agent 08 runs final verification on the actual integration. Passing separately on three branches does not prove that their combination passes.

## Review before opening a PR

Each delivery must include its branch, base commit, final SHA, changed files, before/after reproduction, checks, and outstanding limitations. Inspect `git diff <base>...HEAD`; do not mix changes from other agents. Prepare a title and body describing the problem, resulting behavior, and validation.

The F01 credential must be rotated in the relevant account. Task 06 prepares shareable configuration and prevents accidental commits; that does not revoke the credential, and F01 must not be marked fully resolved until rotation is confirmed.

## Execution details for this audit

The installed CLI supports `--bg`, `--model`, `--effort`, `--mcp-config`, and `--strict-mcp-config`. Authentication works outside the sandbox through the existing Claude session. Do not use options that bypass all permissions.

The model policy, provider comparison, and effort-selection guidance live only in [`rules.md`](rules.md). Do not duplicate or override that guidance here. The command below reproduces the recorded configuration for the original audit run; use it again only when the current task and user instructions still call for that configuration:

```text
--model claude-opus-5-5 --effort medium
```

Official Claude Code references: [model configuration](https://code.claude.com/docs/en/model-config). Session IDs and actual run status are recorded in `target/claude-audit/runs.json` and reported in chat. That file is local and must not be committed.

The prompts authorize editing and testing in isolated project copies. Check the path and branch before pasting them into another Claude session with system access.
