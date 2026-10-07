# Agent Registry

Last update: 2026-10-01. The coordinator is the only writer. Resolve local absolute worktree paths with `git worktree list --porcelain`; keep public records portable.

| Agent | Provider | Task | Branch | Worktree name | Base | Status |
|---|---|---|---|---|---|---|
| PM-01 | Codex | TASK-PM-001 | codex/rules-navigation | mct-rules-navigation | 2488ec1e9e9d8cfdfc5c9f7b3ffa50cd141ce033 | review |

No Claude worker was launched for TASK-PM-001. A registry row is an assignment record, not proof of a running process. Check live ownership before dispatch or takeover.

Lifecycle: planned, active, waiting, blocked, review, completed, abandoned, recovered. An interrupted writer must be confirmed stopped before another writer takes ownership.

## Unverified existing worktrees

Observed by Git on 2026-10-01. These are inventory entries, not agent assignments. Ownership and task/PR state must be investigated before reuse, integration, or cleanup. No branch is classified as abandoned.

| Branch | Worktree name | Observed HEAD |
|---|---|---|
| main | mini-consumes-tokens (primary) | 2488ec1 |
| codex/centralizar-reglas | mct-centralizar-reglas | 1880048 |
| codex/mit-project-license | mct-mit-project-license | 2cca543 |
| codex/project-agent-hooks | mct-project-agent-hooks | df2f89d |
| codex/project-skills | mct-project-skills | be9b9a8 |
| codex/audit-dead-code | audit-dead-code | 331218e |
| codex/audit-exclusions | audit-exclusions | 52aa751 |
| fix/audit-config-docs | audit-integration-review | aff998b |
| codex/audit-lua | audit-lua | 39dd255 |
| codex/english-documentation | english-docs | 9a4b095 |
| feat/fts5-search-symbols | mct-fts5-search-symbols | 84d7e9b |

Existing names are recorded verbatim as Git identifiers, including legacy non-English names; renaming them is outside this task.
