# Project State

## TASK-STORAGE-RELIABILITY — 2026-10-25

Current owner: Codex, single agent. Base b61cc7ce971fdc46a61259af95af7fac823ae352,
primary feature/dead-code-editor checkout with all pre-existing work preserved.
Status: Phase 4 implementation validated locally; operational and performance acceptance pending.
Overall storage reliability remains PARTIALLY DONE; no integration/publication authorization.
Authoritative scope, limitations, checks and continuation:
[storage reliability checkpoint](checkpoints/storage-reliability.md).
Design: [SDD](../docs/01-architecture/storage-reliability-spec.md).
No worker was dispatched. Whole-system acceptance and production cutover remain
pending; earlier records below retain their historical task context.

Phase 4 scope: idle filesystem reconciliation, one-transaction index generation
publication, source mutation detection and MCP snapshot consistency. The
checkpoint and verification document are authoritative for actual code, tests,
and remaining acceptance; previous task records remain historical evidence.


Last update: 2026-10-19
Updated by: Claude Code (issue #207 closure)

## Issue #207: delivered; last follow-up in review

Everything issue #207 asked for is in GitHub main (`19e3733`, verified 2026-10-19
with `gh`): the spike (PR #208, `5c308d2`), report cache and `--output` (PR #209,
`a7c81b9`), the I1–I6 improvements and evidence, verdicts, subgroups, partial
blocks and labeled evaluation (ADR-008–012), and the 2026-10-18 follow-ups:
cleanup #211 (`a38dcc5`), macOS watcher tests #212 (`cff2f35`), symbol coverage
#213/#216 (`7fded7b`, `19e3733`), presentation research #214 (`be8556f`) and the
`reduce-duplication` skill #215 (`6346fba`).

The only open follow-up is branch `fix/duplicates-cache-toon`: the report cache
moves from JSON to TOON and ADR-007/COMMANDS.md document the cache, `--no-cache`
and `--output`. This records branch closes #207 and should merge after it.

`feat/207-token-twins` holds nothing unintegrated: its eight local commits reached
main as `2987e8e` (#211) and its dirty worktree matches main or is older than it.
Do not push or merge that branch; merging it conflicts and only reintroduces
stale records. Worker worktrees `mct-207-*` remain; removal needs the user's
approval. Deferred by design, each needing its own issue: default normalization
change, more languages, an MCP tool, persisted fingerprints, Type-3 matching and
the proposals in `benchmarks/token-twins-presentation.md`.
ERR-009 (native FSEvents delivery) and ERR-015 (installed MCP server older than
the primary index) remain OPEN.

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
