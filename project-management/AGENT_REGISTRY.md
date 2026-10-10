# Agent Registry

Last update: 2026-10-10. The coordinator is the only writer. Resolve local absolute worktree paths with `git worktree list --porcelain`; keep public records portable.

| Agent | Provider | Task | Branch | Worktree name | Base | Status |
|---|---|---|---|---|---|---|
| PM-01 | Codex | TASK-PM-001 | codex/rules-navigation | mct-rules-navigation | 2488ec1e9e9d8cfdfc5c9f7b3ffa50cd141ce033 | completed |
| LANG-01 | Claude Code | TASK-LANG-001 | docs/lang-crate-base-and-fuzz | main checkout | 5956a7611b344c4ac01d3f1613b6589a420e6908 | completed |
| LIB-01 | Claude Code | TASK-LIB-001 | claude/library-onboarding-audit | mini-consumes-tokens-claude-library-onboarding-audit | 1ad997242c5629df14d77acb1f5c36baec202140 | completed |
| WEB-01 | Codex, then Claude Code | TASK-WEB-169 | codex/reconcile-website-169 | mct-reconcile-website-169 | 4d7d2ea5993bf8e3d5fad45fa10dad2f285e03d2 | review |
| FW-01 | Codex | #51 JSX references | codex/framework-followup-51 | mct-framework-followup-51 | 883d18fd0a86ce858614e6ae08688ef0a8dd5413 | review |
| MD-01 | Codex | #135 aliases | codex/markdown-alias-135 | mct-markdown-alias-135 | 883d18fd0a86ce858614e6ae08688ef0a8dd5413 | recovered |
| TOOLS-01 | Codex | #11/#17 | codex/tool-attention-contracts | mct-tool-attention-contracts | a94d10129d95c43d8cd56d80ba1656517a354beb | recovered |
| CSS-01 | Codex | #115 | codex/css-nesting-115 | mct-css-nesting-115 | 2d3def5add49e4f3d29b280149dbb5099ca482c5 | recovered |

The Codex workers were stopped by the user on 2026-10-10; "recovered" rows hold uncommitted partial diffs awaiting a new owner.

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
