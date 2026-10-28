# Add review-aware duplicate-code retrieval over MCP

Published as [issue #233](https://github.com/Zubiarka8/mini-consumes-tokens/issues/233)
on 2026-10-26. Owner approved the contract in-session before local registration.
This file retains the complete issue proposal; implementation verification lives
in storage-reliability-verification.md, not in the issue acceptance checklist.

## Problem and use cases

Agents can query dead-code candidates through MCP but cannot retrieve duplicate
candidates or their effective review visibility. Symbol search cannot reconstruct
the detector's grouping or distinguish intentional duplication already reviewed.
A read-only query lets an agent discover actionable groups, then request focused
symbol context with existing tools. Dashboard remains the history/edit interface.

## Proposed contract (Phase 8.1)

Add `find_duplicate_code` following current snake_case tool names. Reuse
`find_duplicate_groups`, `build_token_sources`, stable `review_keys` and
`ReviewSnapshot`. No detector, storage, migration or dependency changes.

Optional inputs: `path` (root-relative exact file/directory), `language` (indexed
id), `normalization` (`broad` default, `consistent`, `exact`), `min_lines` (6),
`min_tokens` (40), `limit` (10, range 1..100), `offset` (0),
`include_reviewed` (false), `max_tokens` (estimated response budget, default
4000, range 512..16000). Reject unknown fields, invalid paths, oversized strings,
zero thresholds and invalid enum values. Do not expose approximate similarity.

Return compact TOON text in the existing MCP text content envelope: published
generation, detected/actionable/suppressed counts before pagination, analysis
completeness, skipped reason counts, unsupported languages, deterministic page
and continuation offset, explicit truncation. Groups contain stable identity,
language, member count, token/line counts, saved/effective group decision,
validity/source verification/actionability and bounded member references
(path, symbol, kind, inclusive lines). No source bodies, notes, comments or
history. If a group exceeds the output budget, report omitted members explicitly;
never silently claim complete context. Responses state that groups are heuristic
candidates, not proof of redundant behavior.

## Review and consistency semantics

Only a valid group-level false_positive or ignored decision suppresses default
results. Pending/confirmed remain actionable. Copy workflow statuses do not
suppress groups. Changed, unavailable or ambiguous fingerprints cannot inherit
suppression. All-suppressed differs from no groups. Apply filtering before
pagination; `include_reviewed` explicitly includes suppressed groups with their
nonactionable status. Read one index snapshot plus one independent review
snapshot per operation. No cross-database atomicity or cross-request pinning is
promised. Report generation so callers can restart pagination after changes.
Do not cache results: review/source changes must be checked on every call.

## Security, resources and compatibility

Read existing project data only; never create/migrate dashboard state. Use
existing source limits, symlink/root isolation, SQL parameterization and review
reader. Treat retrieved text as untrusted data. No execution of repository code.
Errors must not disclose source text or private review content. Analyze under
existing detector limits; disclose that page/output caps do not bound whole
repository analysis cost. Reject excessive inputs. Backward compatibility is
additive: existing tools/negotiation unchanged; catalog cost increases and must
be measured independently from response/workflow tokens.

## Alternatives and value evaluation

Keep CLI/dashboard as a fallback (no catalog cost, but no direct MCP equivalent),
or overload symbol search/dead-code (misleading semantics; rejected). Compare
CLI report + focused MCP lookups against the new discovery response on identical
fixtures, counting calls and estimated response/catalog/total workflow tokens.
Do not claim detector precision/recall from classification agreement or fewer
visible groups. Report detection parity and labeled review-suppression accuracy
separately; collect latency sample counts/median/p95 and limitations.

## Acceptance and tests

- Pending and confirmed groups visible; valid false_positive/ignored hidden.
- Unchanged reindex yields zero repeated actionable dismissed groups.
- Material changes/renames/membership changes never inherit unsafe suppression.
- Copy rejection cannot suppress a group; notes/history remain untouched.
- All-suppressed, empty, partial and missing-source states are distinguishable.
- Stable deterministic pages, bounded output, invalid input rejection.
- Coherent generation under concurrent reindex; no stale review cache.
- CLI/dashboard comparison and read-only database preservation fixtures.
- Real rmcp client tools/list and tools/call, batch and catalog/TTC coverage.
- Fmt, strict all-feature Clippy, workspace tests, evaluator and diff check.
- Record estimated catalog overhead and complete workflow costs, without
  unsupported savings claims. Cross-platform/production acceptance separate.

## Governance and scope

This is the issue-first contract proposal required by rules.md and ADR-022.
Creating the issue records the proposal; it does not imply maintainer approval,
merge authorization, deployment, or authorization to change production data.
