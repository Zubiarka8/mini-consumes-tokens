# Multi-agent review with Codex

This directory contains a local multi-agent review workflow. It requires
`.codex/config.toml` and six role definitions in `.codex/agents/`; the role
definitions are not present in this checkout, so the workflow cannot run as
documented until they are supplied and validated. Their origin and intended
configuration have not been established.

## Layout and roles

- `.codex/config.toml`: local agent configuration.
- `.codex/agents/*.toml`: definitions for `safety`, `concurrency`,
  `performance`, `dry`, `security` and `aggregator`.
- `AGENTS.md`: instructions for the root agent.
- `scripts/codex/review/`: orchestration scripts.
- `.review-output/`: generated reports and logs.

The five reviewers cover safety, concurrency, performance, cross-crate
duplication, and security. The aggregator deduplicates, checks and prioritizes
their reports.

## Sequence

1. `./scripts/codex/review/00-setup-dirs.sh`
2. `./scripts/codex/review/01-validate-config.sh`
3. `./scripts/codex/review/02-validate-agents.sh`
4. `./scripts/codex/review/03-launch-native.sh` (native mode), or
   `./scripts/codex/review/04-launch-parallel.sh` (manual parallel mode).
5. `./scripts/codex/review/05-aggregate.sh` if the aggregator was not launched.
6. `./scripts/codex/review/06-cleanup.sh`.

The validation script attempts to stop a running Codex daemon. Inspect the
scripts and their effects before running them. The original instructions
specified Codex CLI 0.162.1 or later and the `gpt-5.6-sol` model; current
availability and compatibility have not been verified. Record the repository
state before review and follow the current model and delegation policy in
`rules.md`.

Output is written to `.review-output/aggregated.md`, with individual reports
in `.review-output/reports/` and logs in `.review-output/logs/`. The intended
review sandbox is read-only; failed workers make the aggregated review
incomplete. The original configuration limited nesting to depth 1 and threads
to 6, and did not allow workers to spawn further workers.
