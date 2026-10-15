# Project State

Last update: 2026-10-15
Updated by: Claude Code coordinator (issue #207)

## Active: issue #207 duplicate-function spike

TASK-207 in TASKS.md. Branch `feat/207-token-twins`, contract `5efc8a7`; workers A/B/C
in their own worktrees. Next action: review each worker's diff, merge the three
branches into `feat/207-token-twins`, add end-to-end tests, run `scripts/unix/check.sh`,
then Stage 8 measurements. Local only: no push or PR without the user's approval.

## Earlier state (2026-10-12, Codex recovery coordinator)

Observed local base: `a3e10b0f450a93107061152feb862a1d242820a1` (`origin/main`).
Remote freshness is unverified: GitHub DNS failed during this session.
Scope: finish the four reviewed task branches and retire the requested hook.

## Current objective

Deliver separately reviewed docs, Markdown aliases, CSS nesting and integration
policy candidates, together with a verified local combination and durable handoff.
The original primary checkout and interrupted worker's probe are preserved.

## Candidates awaiting integration

All tasks remain REVIEW until accepted integration into main:
- TASK-DOCS-MCP-PROSE: `docs/semantic-search-prose-limits` at `62c677f`, published
  by the prior Claude coordinator according to its report and local tracking ref.
- TASK-MD-ALIAS-135: recovered as `claude/md-alias-135` at `6bdbbcd` (recovery ref retained).
  ADR-005, schema assertion and alias cases are complete; independent review passed.
- TASK-CSS-NESTING-115: `claude/css-nesting-115` at `a361b23` (recovery ref retained).
  Includes repeated-parent products, implicit relative parents and complex grouping.
- TASK-PR-POLICY: `docs/pr-integration-policy-draft` at `14a2586`.
  Independent review passed; `3ca8f70` is an ancestor of this candidate.
- Hook retirement: `codex/retire-dogfooding-hook` at `6a0daf3`.
  No active Dogfooding registration; cached calls execute a harmless entry point.

## Verification and limitations

See REPORT.md for exact commands and findings. The combined code tree `ddbc17b`
passed formatting, CI Clippy, evaluation and website types/build/export/resources.
Full workspace run: 1260 passed, 3 failed, 24 ignored. The same three filesystem
watcher tests fail on base `a3e10b0` in this environment; the actual event delivery
cause is unverified. No tests were skipped or weakened. Schema migration tests pass.
The later hook-only merge does not change the tested Rust or website code.

## Publication and preservation

No new push, PR, issue comment, main merge, branch deletion or worktree cleanup.
The four original worktrees remain; user-requested local hook retirement is active.
Recovery used an isolated repository because original Git metadata writes failed.
Candidate refs and history were imported into the original repository from
`mct-recovery-final.bundle`. Original aliases and CSS branches were fast-forwarded
to 6bdbbcd and a361b23; the temporary probe and local hook settings remain intact.

## Next action

Recovery import and feature fast-forwards are complete. Record the approved schema decision on #135 before the feature PR. Recheck watcher
behavior on an unrestricted host or CI before claiming a fully green integration.
Publication beyond the existing docs branch and integration into main are pending.

## Historical context

Prior coordination, language-base, registry and website tasks were integrated as
recorded in TASKS.md. Unrelated #11/#17/#51 work and older worktree inventory have
not been audited by this recovery; do not infer their current status or remove them.
