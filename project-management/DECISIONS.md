# Coordination Decisions

## TASK-STORAGE-RELIABILITY — 2026-10-25

Current owner: Codex, single agent. Base b61cc7ce971fdc46a61259af95af7fac823ae352,
primary feature/dead-code-editor checkout with pre-existing work preserved.
Status: REVIEW for the manual backup slice; overall PARTIALLY DONE. No integration/publication authorization.
Authoritative scope, limitations, checks and continuation:
[storage reliability checkpoint](checkpoints/storage-reliability.md).
Design: [SDD](../docs/01-architecture/storage-reliability-spec.md).
No worker was dispatched. Whole-system acceptance and production cutover remain
pending; earlier records below retain their historical task context.


Record durable coordination decisions here. Architecture ADRs remain in `docs/01-architecture/adrs/`; link them rather than duplicating or renumbering them. Do not log trivial actions or model-provider policy already maintained in rules.md.

## PM-DEC-001 — One coordinator owns shared state

Date: 2026-10-01
Status: Accepted for the proposed workflow
Context: Parallel workers editing the same task board or handoff summary would create conflicting status and merges.

Decision: The coordinator writes the seven shared records. Each worker checkpoints in its own branch in a task-specific document; the coordinator incorporates its result.

Reason: Preserve one authoritative state while allowing independent implementation and recoverable partial work.

Alternatives considered: Shared concurrent board edits; ignored temporary checkpoints as the only memory; full chat transcripts. These make state reconciliation, durable recovery, or context usage worse.

Consequences: Coordination remains an operational convention. Recovery checks Git and verifies previous writer termination; there is no implicit filesystem lock. Public records use portable worktree names and exclude private machine/account data.

## PM-DEC-002 — Keep engineering policy and process separate

Date: 2026-10-01
Status: Accepted for the proposed workflow
Context: Contributors may use a single client; model availability and commands change independently of the project-management process.

Decision: rules.md owns technical and provider policy. PROJECT_MANAGER.md owns coordination and recovery. The optional Codex-led Claude-worker arrangement activates only on user request; single-client contributors use the same task records.

Reason: Avoid duplicated policies and unnecessary provider/subscription requirements.

Alternatives considered: Embedding model tables throughout task/process documents; requiring two providers for every change.

Consequences: Task briefs record the selected provider/model/effort but link to the current policy. The documentation does not install a scheduler or messaging layer.

## Architecture decision links

- [ADR-004 — Grammar version requirements](../docs/01-architecture/adrs/004-grammar-version-requirements.md) (2026-10-10, TASK-LIB-001): grammars are written as the full validated `x.y.z` with caret semantics; `Cargo.lock` is the exact pin.

## Recovery product decisions — 2026-10-12

User-approved: migration11 and symbol_aliases (ADR-005); case-sensitive aliases;
retain all matching destinations for ambiguous note identities; no title fallback.
Public issue #135 decision record is still pending.

User-approved CSS: 256 expansions per rule; preserve marked unexpanded selectors
above the limit without aborting or emitting partial results; no duplicate literal
&:hover beside resolved names. Separate expansions remain per parent. Individually
grouping complex parents with :is() preserves CSS matching semantics without
collapsing the parent list; implicit relative parents participate in the cap.

User-approved PR policy is documented once in PROJECT_MANAGER.md.
User-requested Dogfooding retirement removes hook enforcement; SQLite/index access
constraints and the source-exploration guidance remain distinct.

- [ADR-005 — Markdown frontmatter alias storage](../docs/01-architecture/adrs/005-markdown-alias-storage.md).

## PM-DEC-207 — Contract-first, file-disjoint split for issue #207

Date: 2026-10-15
Status: Accepted by the user (three agents, one branch each, no shared file)
Context: Stage 1 (token source) gates every later stage, and the CLI cannot compile
without the analysis types, so three agents started from `main` would either collide
on files or build against guessed interfaces.

Decision: The coordinator first commits the frozen interfaces (`5efc8a7`). Then three
workers run in parallel on disjoint paths: A token sources, B analysis, C CLI and ADR
draft. `mct-index` receives tokens through `TokenSourceRegistry`, keeping it free of
grammar knowledge; `LanguageParser` is unchanged. End-to-end tests and Stage 8
measurements follow integration, because they need all three parts.

Alternatives considered: one agent per issue stage (stages 2–5 share one module);
a fourth agent for Go and JS/TS (out of the confirmed Rust + Python scope); two agents
(token and analysis work would share an owner and serialize).
