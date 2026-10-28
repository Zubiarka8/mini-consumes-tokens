# MCP findings delivery

Status: DELIVERED. The slices below reached `main` as #246-#251 and #254 on
2026-10-28. This is the trimmed, durable part of the original delivery report
(draft PR #252, head `8b6409b`); the superseded sections are listed at the end.
Line references name the code at the original commits and may have moved.

The task preserves the shared TOON review model. No SQLite review store is ported,
no review notes/history/source bodies are exposed by finding queries, and no
backup PR, website, primary checkout or deferred refactor is changed.

## Review findings

- A-001 (major, `duplicate_query.rs:260`, fixed in 8d06959): a duplicate group with omitted members could
  return `next_offset: none`, making its remaining members unreachable. A
  40-member group at max_tokens 512 reproduces it. Continuation now retries the
  incomplete group and explicitly asks for a larger budget.
- A-002 (minor, `duplicate_query.rs:311`, fixed in 8d06959): the metadata-budget fallback omitted review
  store health. It now preserves `review_store`.
- D-001 (major, `server.rs:2829`, fixed in 1022c54): duplicate detection's internal snapshot ended
  before review fingerprints were fetched. The handler now pins both operations.
- F-001 (major, `web.rs:116`, fixed in bf91ccf): dashboard navigation and later reference/finding queries could
  describe different index generations. A deterministic external publication
  reproduces the mismatch; the web fix pins the complete load.

Duplicate detection still scans before pagination; this inherited cost is recorded,
not optimized. Dead-code identity uses all same-named symbols when assigning
ordinals. Inserting/removing same-named siblings can still shift identity. The
whole-file fingerprint prevents silently hiding changed code; provenance rebinding
and finer fingerprints are separate product decisions.

## Dependency correction

The supplied #246 did not contain #234. Source verification, generations,
watcher reconciliation and cache fences already exist in #234 (10ee064), so
feature/mcp-source-revisions merges that implementation rather than replacing it.
The conflict preserves the review-aware dead-code handler and its read snapshot.
#234's new anyhow dependency required synchronizing the index fuzz lockfile:
locked metadata failed before the seven-line update and passed after it.

## Baselines and limits

Catalog baselines rise from #246's 11939 to 12033 for the optional dead-code
arguments and contract. The omni-app unreferenced-symbol response rises from
402 to 845 tokens for explicit review IDs/labels/counts. Both were regenerated
with mct-eval; ranking/accuracy expectations are unchanged. The 4800-byte catalog
description guard is unchanged. No new dependency is introduced except the
inherited #234 dependency and its lockfile correction.

## Exclusions and decisions

No finer fingerprint, SQLite review store, protocol-version #238 change,
backup-PR closure, #240 closure, LICENSE, errors/, prototype/, tasks/plan.md,
scripts/phase9*, website/ change, main merge, auto-merge or branch deletion.

## Delivered slices

| Slice | Merged PR | Branch |
|---|---|---|
| Duplicate pagination and store health | #247 | `fix/mcp-duplicate-pagination` |
| Dead-code reviews | #248 | `feature/mcp-dead-code-reviews` |
| Source revisions | #249 | `feature/mcp-source-revisions` |
| Live reindex | #250 | `feature/mcp-live-reindex` |
| Web findings generation | #251 | `fix/web-findings-generation` |
| Stale web review decisions | #254 | `fix/web-stale-review-main` |

#246 (`feature/mcp-find-duplicate-code`) is the merged base of the stack.

## Verification evidence (historical, per slice at its original head)


| Slice | Commands | Result |
|---|---|---|
| A | duplicate regression, duplicate_code; check.sh -p mct-mcp-server; check.sh | red 0/1, then 8/0; crate 396/0/1; workspace 1798/0/31 |
| B | review_decisions; toon_default; check.sh -p mct-mcp-server; check.sh | four old-implementation failures, then 6/0 and 6/0; crate 402/0/1; workspace 1804/0/31 |
| C | source_revision; index_generations; check.sh -p mct-mcp-server; check.sh | source red 3/3 then 6/0; generations 7/0; crate 413/0/1; workspace 1824/0/30 |
| D/E | live_reindex; writer_coordination; check.sh -p mct-mcp-server; check.sh -p mct-index; check.sh | 1/0; 5/0; crates 414/0/1 and 434/0/1; workspace 1830/0/30 |
| Web fix | dashboard_snapshot_never_mixes_navigation_and_references_across_reindex; check.sh -p mct-cli; check.sh | red 0/1, green 1/0; crate 129/0/7; workspace 1831/0/30 |
| Build | cargo build --locked -p mct-cli | passed on the web fix tree |
| Fuzz lock | cargo metadata --offline --locked --manifest-path crates/mct-index/fuzz/Cargo.toml --format-version 1 | failed before synchronization; passed afterward |

Counts use passed/failed/ignored. Every reported check.sh gate passed formatting
and CI Clippy; full gates passed evaluation (latest accuracy 1.000, catalog 12033).
No new ignore, disabled assertion, weakened guard or changed quality score.
Raw command history and intermediate corrections: [checkpoint](../checkpoints/mcp-findings.md).
The final docs-only slice uses link validation and git diff --check; it does not
change the tested Rust tree bf91ccf.

## Still open from the original report

Safari acceptance was BLOCKED when the report was written (the fixture server
could not bind its localhost port in the sandbox) and no later run is recorded.
It is not claimed here. Checklist, unchanged: duplicate and dead-code lists,
filters, counts and pagination; confirm / false-positive / dismiss / restore and
reload persistence; edit a dismissed finding's source and reindex externally to
see `needs-reevaluation`; add/rename/remove fixture code and reindex externally
with the dashboard open; a stale-source save must return 409 without overwriting
a decision.

## Omitted from the original report

Superseded by the merges above: the "Local candidates and exact bases" table
(no PR numbers existed then), the "Remote delivery and acceptance" section
(sandbox network blocks; publication happened later), the Safari MCP setup note
and the six draft PR bodies. The full original is preserved in git on branch
`docs/mcp-findings-delivery` (`8b6409b`) while it exists.
