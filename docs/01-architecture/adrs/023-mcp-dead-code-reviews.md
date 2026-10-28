# ADR-023: Shared review decisions in MCP dead-code findings

## Status

Accepted for local implementation; delivery depends on ADR-019 and ADR-022.

## Context

The CLI and dashboard resolve TOON decisions while MCP still returns hidden
candidates. Using candidates alone for identity would assign the wrong ordinal
when a same-named sibling gains a reference.

## Decision

Use `scan_dead_code` and `dead_code_findings`, retaining every scoped symbol for
stable identities. Resolve through `Reviews` before paging; never import the
backup SQLite review engine. Keep both output formats and existing parameter
defaults. Add optional inclusion and estimated-budget parameters. Expose counts,
identity, review label and store health, without notes or history. An unreadable
store never suppresses findings. Pagination describes the current result set,
so callers restart after index or review changes.

## Consequences

The CLI, dashboard and MCP share one suppression policy. Output budgeting can
return fewer rows than limit and explicitly supplies a continuation offset.
The budget limits rendering, not the cost of scanning the index. Hashes remain
whole-file fingerprints; finer provenance rebinding would need a separate
decision and TOON format design rather than a direct SQLite port.

## Verification

`tests/review_decisions.rs` exercises all six states, restoring hidden rows,
filtering before paging, edits, degradation and offsets beyond the result set.
