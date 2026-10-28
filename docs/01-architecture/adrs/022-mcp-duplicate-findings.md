# ADR-022: Review-aware duplicate findings over MCP

## Status

Accepted for the local implementation. Builds on ADR-019 (shared review
decisions): the MCP server reads the same store as the dashboard and the CLI.

## Context

MCP exposed `find_dead_code` but no duplicate-finding query. The CLI and the
dashboard already composed `find_duplicate_groups`, the token-source registry
and the shared review decisions. Symbol search cannot recreate those groups or
their review state, so an agent had no way to ask for duplicates, and could not
skip the groups a person had already rejected.

## Decision

Add a read-only `find_duplicate_code` tool. It reuses the detector, the
token-source registry, the stable group identities and `mct_index::reviews`.
It adds no detector, no store, no schema and no dependency, so reporting a
decision cannot create a second suppression engine.

- Defaults match the CLI: `broad` normalization, six token-bearing lines,
  forty tokens. `consistent` and `exact` are explicit.
- Groups are resolved against the review decisions before pagination. A group
  is hidden only when a `false-positive` or `dismissed` decision still matches
  its exact code (ADR-019). `confirmed`, `in-review`, `resolved`, an edited
  group (`needs-reevaluation`) and an ambiguous group all stay actionable.
- `include_reviewed` shows the hidden groups, marked `actionable: false`.
- When a review file cannot be read nothing is hidden, and the response says
  `review_store: degraded`. The tool never writes review state.
- No notes, history or source bodies are returned: only stable group ids and
  member references (`path`, symbol, kind, lines). Use the existing symbol and
  context tools for source.
- Output is bounded by validated limits (`limit` 1..100, `max_tokens`
  512..16000, root-relative `path` without traversal) and an estimated token
  budget. Counts come before pagination, and truncation is explicit
  (`truncated`, `next_offset`, `members_omitted`): a group whose members were
  cut can be retried with a larger budget.
- Paging is deterministic for one index state. It is not pinned across
  reindexes, so a caller restarts paging when the index changes.

## Alternatives and tradeoffs

- Keep it in the CLI and dashboard only: no catalog cost, but no direct MCP
  discovery.
- Overload `find_dead_code` or symbol search: rejected, the results and the
  semantics would be misleading under those names.
- A dedicated query: chosen for direct agent workflows and domain reuse. It
  adds permanent catalog bytes; the catalog entry is kept compact for that
  reason, and the size guard in `tests/catalog.rs` still holds.

The detector's cost is the cost of the CLI's: the analysis holds the server's
index mutex for its duration, and the budget bounds the output, not the work.

## Verification

`crates/mct-mcp-server/tests/duplicate_code.rs` runs the real detector against
decisions saved through the shared store: pending, confirmed and suppressing
states, an edit that brings a dismissed group back, filtering before
`limit`/`offset`, absence versus suppression, an unreadable review file,
parameter validation, budget truncation and `batch` dispatch.
