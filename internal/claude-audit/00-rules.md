# Shared rules for remediation agents

You are a Rust engineer working on mini-consumes-tokens. Deliver the assigned task with small changes and verifiable evidence. Follow the model and effort selected for this audit in [`Claude-Plan.md`](../../Claude-Plan.md), and follow the general selection policy in [`rules.md`](../../rules.md). Do not change the selected configuration or create subagents.

## Context and isolation

`GPT-Report.md` audited commit `1880048dd8274b0b3a7f82b07bedd3219bde5ca1`. Its findings are hypotheses verified against that snapshot: reproduce each case against your code before fixing it. The suite passed 762 tests at that time, with 17 ignored; do not report those numbers as validation of your changes.

Work only in your assigned worktree and branch. At the start, check `pwd`, `git branch --show-current`, and `git status --short`. The coordinator provides `BASE_COMMIT` and `WORKTREE`. Do not modify the original checkout, another branch, or global configuration. Preserve pre-existing changes. Read the applicable `CLAUDE.md`/`AGENTS.md` entry point and follow these shared rules and any user instructions in the session.

## Exploration and context

Explore code only through this project's MCP or `mct-cli`: status, symbol listings, skeletons, context, and relations. Start with `get_indexing_status` and a query that confirms the MCP path points to your worktree. Use `batch` when you know several queries. Reading whole files is appropriate when preparing a specific edit, not as a substitute for exploration. Access SQLite only through the project's software. If a capability is missing, report the limitation; do not silently fall back to grep or external SQLite queries.

If MCP does not connect, use the repository's diagnostics. Do not reinstall global binaries shared with other agents; build a local binary if needed and explain how to reconnect it. Keep your own index. Do not read or print secrets, `.env` files, personal configuration from other checkouts, or tokens. The supplied MCP configuration is specific and sanitized.

## Scope and quality

Implement only the assigned task and its directly necessary tests/documentation. Keep `mct-core` independent of tree-sitter/SQLite/MCP, and the index independent of grammars. Do not add external dependencies without explaining why; registering an existing internal dependency is allowed when the task requires it. Do not change public contracts or the schema until the issue required by `AGENTS.md` has been opened. If that blocks part of the work, deliver the design and reproduction and state the exact outstanding requirement.

Do not delete symbols just because `find_dead_code` reports them. Do not use `unwrap`/`panic` on repository input. Do not refresh baselines or snapshots to hide regressions. If you change a corpus, update `internal/corpus-progress.md` using the repository tools. Do not add a full long fixture corpus to a small fix.

Add a regression test that fails before and passes after the fix. Run tests for affected crates, formatting, and appropriate Clippy checks, preferably with `scripts/unix/check.sh`. Watcher tests on macOS may require an unrestricted environment: report a permission failure rather than changing product behavior. Do not run a global audit or repeat checks without a reason.

## Delivery

You may edit, validate, explicitly stage your files, and create local commits such as `fix(scope): ...`. Do not use `git add .`; do not include coordination files, local configuration, logs, or secrets. Do not push, open/merge PRs, publish issues, or run destructive reset/clean operations. Publication will be coordinated later, including the dependency on PR #90.

At the end, report in English: branch and SHA, issue resolved, files changed, tests run and their actual results, risks/blockers, and a brief proposed PR title/body. If a prior issue is required, provide its title and ready-to-publish text, and clearly leave the cross-cutting implementation pending. Share only meaningful progress backed by tool results.
