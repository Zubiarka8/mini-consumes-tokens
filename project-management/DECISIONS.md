# Coordination Decisions

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
