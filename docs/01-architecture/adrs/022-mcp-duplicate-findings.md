# ADR-022: Review-aware duplicate findings over MCP

## Status

Accepted for local implementation, 2026-10-26. The owner explicitly approved
[issue #233](https://github.com/Zubiarka8/mini-consumes-tokens/issues/233)'s
contract after it was created, satisfying rules.md's issue-first requirement.
Merge, deployment and production acceptance remain unauthorized.

## Context

At the Phase 8 audit, MCP exposed find_dead_code but no duplicate-finding query.
CLI/dashboard already composed find_duplicate_groups, build_token_sources and
shared review visibility under an index snapshot. Symbol search cannot recreate
those groups or their effective review decisions. The Phase 8 proposal was
therefore deferred; #233 now authorizes the additive local implementation.

## Decision

Add read-only find_duplicate_code. Reuse the detector, token-source registry,
stable group identities, verified source fingerprints and ReviewSnapshot. Share
duplicate_states diagnostics with existing duplicate_visibility, so reporting
saved/effective decisions cannot create a second suppression engine.

Defaults remain broad normalization, six token-bearing lines, forty tokens,
line ranking and trivial-call demotion. Exact/consistent modes are explicit.
No partial-block/approximate detector, schema change or dependency is introduced.

Default results contain actionable groups. Only valid group-level
false_positive/ignored decisions suppress. Copy labels never suppress groups;
they remain in dashboard workflows and are omitted from this compact MCP view.
Changed/unavailable/ambiguous identities remain eligible for review. No notes,
comments, history or source bodies are returned. include_reviewed explicitly
shows suppressed groups as nonactionable.

One index snapshot covers detector/navigation/generation; review state has its
own independent read snapshot. No cache, persistent cross-request snapshot or
cross-database atomicity is promised. Offset ordering is deterministic within a
published generation; callers restart paging when generation changes.

Validated limits and estimated token budgets bound output, not whole analysis
cost. Return stable group identities and member references, counts before
pagination, coverage diagnostics and explicit member/page truncation. A group
whose members are omitted can be retried at the same offset with a larger
budget; use existing symbol/context tools for source. The contract and SDD
are in mcp-protocol-spec.md Phase 8.1 and #233.

## Alternatives and tradeoffs

- Keep CLI/dashboard only: no new catalog overhead, but no direct MCP discovery.
- Overload symbol search or dead-code: rejected because results and semantics
  would be misleading under those existing names.
- Dedicated query: selected for direct agent workflows and domain reuse; adds
  catalog cost. Measurements must count this permanent cost as well as response
  size. Synthetic discovery payloads are smaller than the full CLI report, but
  large scans can be slower and hold the existing server index mutex. No
  universal speedup, full-task token saving or detector accuracy gain is claimed.

## Verification and consequences

Implemented with isolated review/identity/reindex/generation/budget/security
fixtures, real rmcp client calls, CLI/dashboard SQLite parity and backup
compatibility tests. The new Rust/Python evaluation suite joins existing CI
suites. Catalog-only baseline updates are intentional; per-case existing quality
and response-token baselines are preserved. Actual results and remaining native
platform/resource/production acceptance live in storage-reliability-verification.md.
