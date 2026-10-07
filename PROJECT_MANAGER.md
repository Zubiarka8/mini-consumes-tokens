# Project Manager — Multiagent Coordination

## Purpose and authority

This document defines durable project management, delegation, review, recovery, and integration. Read it at the start of substantial work. [rules.md](rules.md) remains the authority for architecture, conventions, MCP usage, commands, model selection, effort, and billing. Do not duplicate those policies here. Follow the active client's permissions and the user's instructions; documentation does not grant tools or bypass approvals.

The user owns product direction and authorizes external actions. When the user requests the Codex–Claude workflow, Codex is the project manager, architectural coordinator, reviewer, integrator, and recovery owner. Claude performs bounded implementation, investigation, debugging, testing, or independent review. Claude may propose architecture changes; the coordinator reviews them before acceptance. Claude must not become project manager automatically when Codex is unavailable. Any role reassignment requires explicit user authorization.

Single-client contributors, including Cursor users, use the same task and recovery process with their available client as coordinator. Cross-provider execution is optional and must be requested. This protocol does not require additional subscriptions or launch agents on its own.

Prioritize correct architecture, functional correctness, quality, regression safety, maintainability, speed, then usage efficiency. Choose sufficient capability through `rules.md`; neither cheap incomplete work nor expensive duplication meets the quality bar.

## Durable project memory

Models and conversations are temporary. Git, identified commits, branches, worktrees, requirements, tests, issues, PRs, and compact operational records preserve the project. Never leave an essential decision, blocker, or next step only in chat. A local uncommitted checkpoint survives a process failure but not loss of that machine: commit and push scoped work when authorized. Never promise recovery of changes that were never persisted.

The coordinator maintains these records:

| Record | Purpose |
|---|---|
| [PROJECT_STATE.md](project-management/PROJECT_STATE.md) | Objective, active work, blockers, integration backlog, risks, next action |
| [TASKS.md](project-management/TASKS.md) | Task ownership, scope, base, dependencies, acceptance, checks, result |
| [AGENT_REGISTRY.md](project-management/AGENT_REGISTRY.md) | Agent assignment, branch, worktree identity, base, lifecycle |
| [DECISIONS.md](project-management/DECISIONS.md) | Coordination decisions and links to architectural ADRs |
| [HANDOFF.md](project-management/HANDOFF.md) | Latest recoverable state and exact continuation step |
| [REPORT.md](project-management/REPORT.md) | Findings and acceptance decision for an identified revision |
| [SESSION_LOG.md](project-management/SESSION_LOG.md) | Dated significant events, without transcript dumps |

Keep one authoritative task entry and link to its details rather than copying it across records. Bound active summaries; summarize closed tasks with their outcome and commit/PR links. Preserve durable decisions and unresolved work. Architecture ADRs stay in the existing `docs/01-architecture/adrs/` convention; the decisions register links to them instead of starting a competing archive.

### Prevent administrative conflicts

Only the coordinator edits the seven shared records in the coordination branch. Workers record checkpoints in their own branch at `project-management/checkpoints/<task-id>.md`, or in their task's existing dedicated document, and return that path and commit. The coordinator incorporates the result. Do not cherry-pick a worker's shared-board edits blindly. State updates accompany the relevant integration where practical; do not mix unrelated code in administrative commits.

These are operational ownership rules, not filesystem locks or automated enforcement. Record portable worktree names and branches in public files; resolve absolute paths with `git worktree list` locally. Never publish credentials, account identifiers, machine-specific paths, private conversations, or billing details. Public records hold engineering facts; private runtime data stays local.

## Session startup and reconstruction

1. Read `AGENTS.md`, `CLAUDE.md`, `rules.md`, and this document at session start. During the session revisit relevant sections, or reread changed instructions.
2. Check `git status --short --branch`, `git branch --show-current`, `git rev-parse HEAD`, and `git worktree list --porcelain`. Identify uncommitted changes before editing; never overwrite unexplained work.
3. Read Project State, Tasks, and Agent Registry. Read the relevant pending handoff/report and decision/log entries. Follow task references to relevant requirements, PRs, and issues when access exists; record unavailable checks rather than inventing status.
4. Reconcile records with actual Git state. A branch's existence or age does not prove that an agent is active or that work is abandoned. Check ownership before reuse or cleanup.
5. Before source exploration, check the MCP root and index health/freshness using the project's own tools as required by `rules.md`. Each worktree must use its own root and index. If the tools are absent, follow the reconnect instructions; do not substitute direct SQLite access or forbidden source searches. Documentation-only work can proceed without source exploration; record that index health was not checked.
6. Identify the current task, base, dependencies, writer, and next safe action before starting new work. Ask the user only for ambiguity that the available records cannot resolve.

Read compact current records, not entire historical logs or roadmaps on every turn. Git observations can be newer than a recorded checkpoint: reconcile discrepancies and mark uncertainty explicitly.

## Tasks, ownership, and isolation

The task lifecycle is `TODO → READY → IN_PROGRESS → REVIEW → DONE`. Use `BLOCKED` with a reason and resume condition when progress requires unavailable input, access, or quota. A rejected review returns the task to `IN_PROGRESS`. `DONE` means acceptance and integration are verified, not merely that a process exited. Completed implementation waiting for integration stays `REVIEW`.

Before dispatch record: task ID, objective, owner, reviewer, branch, worktree name, full base commit, allowed write scope, exclusions, dependencies, priority, risk, provider/model/effort choice, acceptance criteria, required checks, checkpoint location, and deliverable. A task becomes `READY` only when its dependencies and brief are sufficient. Use `not applicable` or `not checked` with a reason instead of leaving ambiguous blanks.

Every code-writing agent has an exclusive task branch and worktree. Never allow concurrent modifications in the same checkout or branch. Use descriptive English names: `codex/<task-id>-<description>` and `claude/<task-id>-<description>`. Reuse a suitable owned worktree only after checking its state; do not create unnecessary checkouts. Existing branch names need not be renamed just to match this convention.

The base is the exact commit the task starts from. Record it before dispatch and review changes against it. If a dependency or rebase changes the base, record the previous and new base and repeat affected checks. Give each worktree its own MCP root; verify results describe that checkout before relying on them.

Check expected file/module overlap before parallel work. Prefer sequencing, narrower scopes, explicit dependencies, or an intermediate accepted base. Overlap requires a recorded integration plan. An allowed write scope is a logical ownership contract: other agents may read within the exploration rules but must coordinate before editing it. Shared contracts and the seven administrative files need particular care.

Agent states are `planned`, `active`, `waiting`, `blocked`, `review`, `completed`, `abandoned`, or `recovered`. Record observable state; a missing session is not proof of abandonment. The previous writer must have stopped or relinquished ownership before recovery takes over its worktree.

## Bounded delegation

Use the supported dispatch route and client options described in `rules.md`. Do not invent cross-client messaging, sessions, permissions, billing caps, or completion notifications. A process result or persisted checkpoint is a deliverable; a missing final message is not evidence that no work happened.

A worker brief contains:

```text
TASK ID:
OBJECTIVE:
BRANCH / WORKTREE NAME:
BASE COMMIT:
ALLOWED WRITE SCOPE:
DO NOT MODIFY:
DEPENDENCIES:
RELEVANT CONTEXT AND RULE LINKS:
ACCEPTANCE CRITERIA:
REQUIRED CHECKS:
MODEL / EFFORT / AUTHORIZED LIMITS:
CHECKPOINT PATH AND FREQUENCY:
DELIVERABLE: status, commit, changed paths, check results, risks, remaining work
```

Send the specific requirement, relevant context pack/snippets/diff, and links. Do not dump the whole repository, roadmap, reports, operational history, or conversation. Use one provider by default; duplicate work only for justified independent review, deliberate comparison, difficult diagnosis, or critical risk.

Codex may implement directly when global context, urgency, worker unavailability, or lower coordination cost makes it appropriate. It still uses a task, exclusive branch/worktree, scoped commits, acceptance criteria, and review proportional to risk.

## Checkpoints and handoffs

Persist a checkpoint before a large change, after a meaningful implementation slice, before risky operations or external waits, when context may be lost, and before ending a session. Do not checkpoint every tool call. A sudden stop may prevent the final write, so checkpoints must describe the last verified state, not expected future work.

Each task checkpoint or handoff contains:

```text
Task / previous agent / status:
Branch / worktree name / base commit / latest commit:
Objective and acceptance criteria:
Completed / in progress / not started:
Changed paths and uncommitted changes:
Checks executed, exact results, and checks still pending:
Known problems and relevant decisions:
Dependencies and outstanding worker/process/session references:
Files or symbols to inspect first:
Exact next action and resume condition:
```

Keep operational facts and evidence; omit internal reasoning transcripts. Worker checkpoints live in their owned branch. The coordinator keeps the authoritative continuation summary in HANDOFF and task entries. Retain useful recovery references until integration is verified; do not delete a worktree or checkpoint needed by unfinished work.

While waiting, persist the dependency and resume condition. Use an exposed bounded wait or completion event, not repeated model prompts or indefinite polling. Independent accepted work may continue in a different owned worktree. Do not implement against an unconfirmed API or write another worker's area.

## Recovery when an agent stops

Distinguish context exhaustion, model-family limits, shared usage limits, spend caps, and process errors using the client evidence. `rules.md` defines model/effort restrictions, client-specific compaction commands, and billing recovery. Compacting context cannot restore an exhausted usage allowance.

| Failure | Coordinator or surviving agent action |
|---|---|
| Claude loses context | Reconstruct its task from Tasks, checkpoint/Handoff, base, commits, diff, and check evidence. Resume only the remaining scoped work. |
| Claude exhausts quota or stops unexpectedly | Mark the assignment blocked; inspect branch, commits, uncommitted diff, and latest checkpoint. Preserve partial work. Stop or verify termination of the old writer before takeover. Codex may continue, schedule the same worker after reset, or create a recovery task. Revalidate relevant checks when authorized. |
| Codex loses context | A replacement session reads the startup records and Git state, reconciles the pending review/integration, and resumes the exact next action. Do not restart completed work from a fresh prompt. |
| Codex exhausts quota while Claude works | Claude may finish only the sufficiently specified, already delegated task, run authorized checks, create scoped commits/checkpoints, and return implementation for review. It must not promote itself, expand scope, or integrate into main without explicit prior authorization. Codex later reviews and integrates. |
| Both providers are unavailable | Persist whatever state remains accessible, preserve worktrees and diffs, and record the reset/resume condition. Resume after availability returns; never enable paid usage without approval. |
| No final report exists | Inspect persisted artifacts; classify unknown claims as unverified. Recover from the latest proven state and test the recovered implementation as appropriate. |

An unsafe implementation may be replaced in a recovery branch after preserving its original history and diff. Destructive discard, deletion, reset, or cleanup that could lose work requires appropriate user authorization. Do not keep waiting solely for a final message from an unavailable agent.

## Review, findings, and completeness

The preferred cycle is: Codex plans and assigns → Claude implements and checkpoints → Codex independently reviews → REPORT records findings → Claude fixes → Codex revalidates → accepted integration.

Review an identified commit and actual base-to-head diff. Check branch/base, changed paths, scope surprises, architecture, behavior, edge cases, error handling, regression coverage, formatting/lint, documentation, compatibility, and introduced debt. Record actual command outcomes and revision; a worker's claimed passing result is not an independently reproduced result. Run the required checks when authorized; otherwise report the gap and do not claim approval based on missing evidence.

REPORT entries use stable finding IDs and concrete file/line or symbol evidence, severity, problem, impact, expected fix, and required verification. Separate critical, major, and minor findings; identify missing tests/docs, possible regressions, and unverified assumptions. Results are `APPROVED`, `CHANGES_REQUIRED`, or `BLOCKED`. Findings remain traceable when fixed; a change after review invalidates approval for the affected revision until revalidation.

After significant implementation, compare requirement → implementation → checks. Look for unfinished behavior, TODO/FIXME/HACK markers, placeholders, `unimplemented!`, stubs or empty functions, uncovered paths, ignored errors/tests, commented-out tests, permanent mocks, temporary code, unfinished flags/configuration/migrations, outdated scripts, broken references, missing CLI/API docs/tests, and stale related issues or branches. Classify each as **confirmed**, **probable**, **unverified**, or **intentional**, with evidence. Do not label every TODO or old branch a defect.

This audit must obey source-exploration restrictions in `rules.md`: use the project MCP/CLI, targeted context packs, permitted diff/pre-edit reads, and allowed prose inspection. If a required lookup is outside tool coverage, request the specific exception rather than running a forbidden broad source search.

Check crate boundaries, cross-cutting contracts, MCP/schema/client impact, arbitrary-input safety, and relevant Linux/macOS/Windows behavior. Existing passing tests do not establish full requirement coverage. Record missing platform evidence accurately. High-risk schema, MCP signatures, LanguageParser, migrations, index/relation algorithms, concurrency, security, deletion, and platform-wide changes require the design, prior issue when required, focused checks, and possibly independent review described in `rules.md`.

## Integration and completion

1. Require task status REVIEW, correct dependencies/base, and an identified candidate revision.
2. Review the actual diff; resolve REPORT findings and acceptance gaps. Verify required checks and documentation.
3. Preserve unrelated local changes; integrate only from a suitable clean checkout. Choose merge/rebase according to the existing workflow and ownership. Never rewrite another contributor's published branch without authorization.
4. Validate the actual combined revision: passing branches separately does not prove their combination passes. Record check commands, results, and the tested commit or tree.
5. Push, publish, or merge only within the user's authorization. An approved PR awaiting merge remains REVIEW; record approval separately from integration.
6. After verified integration update Tasks, Project State, Registry, Handoff, report references, and relevant issues/PRs. Record the integration commit, summarize obsolete handoffs, and inspect remaining worktrees before any authorized cleanup.

Commits must be small, coherent, English, and follow `rules.md`. Keep features, unrelated refactors, docs, and accidental formatting changes separate. State-only commits may refer to a preceding implementation commit; they cannot contain their own final hash, so resolve the branch's latest HEAD through Git at recovery.

## Session closure and autonomy

Before ending: update state, task status, registry, current handoff, pending report, important log events, and exact next action. Identify commits and any uncommitted work. Do not mark delivery or checks complete without evidence. Retain unresolved work and decisions; administrative cleanup must not erase recovery information.

Proceed autonomously on clear, reversible, in-scope work already authorized. Ask when functional interpretations conflict, product/business direction is undecided, credentials/access are missing, consequences are irreversible, or data loss is possible. State the decision, options, consequences, and recommendation. Existing authorization persists; permissions and tool availability still apply.

Success means a new session can reconstruct ownership, objective, base, completed work, remaining acceptance checks, findings, dependencies, and the next safe action after either provider disappears. This documentation specifies the protocol; it does not by itself implement a scheduler, message bus, filesystem locks, quota detector, or automatic failover.
