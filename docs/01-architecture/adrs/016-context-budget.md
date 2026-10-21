# ADR-016: Estimated token budget and progressive detail

## Status

Accepted for the local implementation, 2026-10-21.

## Context

Tools capped output by item and line counts (`limit`, `snippet_lines`,
`source_lines`) but never by tokens, and callers could not ask for less detail
first and escalate. The only token figure in the repository was `chars / 4`.
No tokenizer is a dependency, and none matches Claude's.

## Decision

Two optional arguments, `detail` (`overview`, `summary`, `detailed`) and
`max_tokens`, on `build_context_pack`, `search_symbols_like` and
`hybrid_search_like` (so on their deprecated aliases). All three go through the
same `mct_index::select`; there is one selection algorithm. Omitting both leaves the response exactly as
before; `detailed` is the old content. No new MCP tool, no schema migration.
Markdown notes and sections use the same pack, because headings are already
symbols with exact line ranges.

- **`max_tokens` is an estimated budget**, counted with `EstimatedCounter`, a
  dependency-free heuristic. It is not a model limit. Counts carry a `Basis`
  (`Estimated`, `Measured`, `Billed`); only `Estimated` is produced here.
- **The whole response is counted**: header, rows, field names, paths, line
  numbers, delimiters, the footers and the `Budget:` line. For
  `hybrid_search_like` the first-line note and the string-literal section of an
  exact phrase are counted too; they are not rows, so they are never cut.
- **Fitting.** `mct_index::select` ranks, de-duplicates, paginates and then
  fits. Over budget, the lowest-ranked rows lose one detail level at a time,
  then rows are dropped from the tail. Each step shrinks the row count or the
  sum of levels, so the loop ends. The response says how many rows were lowered
  or dropped. The level steps are coarse (a definition goes from its whole
  source to its signature), so a final pass spends what is left on the leading
  lines of a lowered definition; the row stays flagged as lowered.
- **Minimal response.** If not even one `overview` row fits, the response is
  valid but has no rows, and says which budget one row needs (`No row fits`).
  If the empty response alone exceeds the budget, it is still returned.
- **Order and pagination.** Total order: quantized score, evidence kind, path,
  start line, end line, symbol id, role. `next_offset` counts ranked,
  de-duplicated candidates consumed, so on a stable index pages neither skip
  nor repeat. Search reports it as `more: offset=N`; a pack has a single page.
- **De-duplication** drops exact duplicates and fragments contained in a
  better-ranked fragment of the same symbol. Overlapping spans of different
  symbols, files or roles are distinct evidence and are kept.
- **Levels never drop identity.** Every row keeps name, kind and `path:line`
  at every level.
- **The `Budget:` footer exists only when it says something** (stabilization,
  see below): never on a `detailed` pack without a budget that lost nothing,
  never on a search that lost nothing and has no budget. When present it is
  `Budget: complete|partial [~spent/max]` followed by only the parts that apply:
  `lowered=N`, `dropped=N`, `full=N` (estimated cost of showing every row at
  the requested level), `avail: callee=4 caller=3` (what exists but is not
  shown), `next: summary~890 detailed~1775` (estimated cost of each higher
  level) for packs, and for searches `N of T seen; next: offset=K`.
- **Complete versus insufficient.** `complete` means every row of the
  requested page is shown at the requested level. `partial` means rows were
  lowered, dropped or a later page exists, and always names how to continue. A
  query with no match says `No symbol matches`; a budget that fits no row says
  `No row fits` and the budget one row needs. The search header counts all
  matches (as the default page does), not just the rows that fit.
- Budgeted searches bypass the query cache.

### Index changes between pages

`Index::change_marker` moves on every symbol, relation, file addition, edit or
removal (`tests/change_marker.rs`), so it has no false negatives for those. It
also moves on a reindex that changes nothing (run metadata is written), so it
is conservative. It is not included in responses: pages carry no guarantee if
the index changes between calls, and a caller needing one should compare
`get_indexing_status`.

## Stabilization (2026-10-21)

The first benchmark found four problems. Causes, each reproduced by a test:

| Finding | Cause | Change |
|---|---|---|
| `locate a function`, overview: 140 → 166 | A ~70-character footer on a page that lost nothing | Footer omitted unless it carries information (`a_complete_overview_with_no_budget_costs_exactly_the_default`) |
| overview → detailed costs 29% more than one call | The second call re-sends the identity rows the first already sent; inherent | Higher levels are priced in the footer (`next: detailed~N`) so the caller can skip a level; guidance below; measured, not hidden |
| ADR lookup at `max_tokens=300` lost the evidence | Three ADRs share the heading; ranking is a tie, the labelled one is last; 300 tokens fit four rows. The evidence was reachable but the answer read as complete (`4 match(es)`) | Header counts all matches; `Budget: partial, 4 of 6 seen; next: offset=4`; following it recovers the hit in 2 calls (`following_the_continuation_recovers_evidence_a_tight_page_missed`) |
| `detailed` with `max_tokens=1200` returned ~490 tokens | Level steps are coarse: whole source → signature, stranding more than half the budget | Leading source lines fill the leftover budget (`leftover_budget_buys_leading_source_lines_instead_of_stranding_it`) |

### Choosing a level

- `overview`: find *where*. Identity, `path:line`, relations. No source.
- `summary`: add signatures and the first line of a definition.
- `detailed`: today's output; use it when the task needs source.
- If the task needs source, ask once with `detail=detailed` and a
  `max_tokens`. Escalating `overview` → `detailed` costs more than the single
  call (measured), and only pays off when the overview lets you stop.
- `summary` is not always smaller than the default: a default search page has
  no snippets, so `summary` adds a line per hit (measured in
  `benchmarks/context-budget.md`).

No percentage in this repository is a guarantee; they are measurements of the
stated scenarios.

## Adaptive level selection: measured, not adopted

An offline simulation over 57 labelled cases (`benchmarks/adaptive-detail.md`)
compared the unchanged tools, `detailed`, `summary`, "`overview` then
escalate", a budgeted `detailed`, a query-only rule and a caller-intent rule.
A query-only rule saved 7.8% of estimated tokens on tasks without their own
budget, only about 3% more than starting at `overview`, with a gain that
depends on how many tasks need source (break-even 63% against 52% in the set)
and about twice the p95 per task. The caller-intent rule saved 19% but needs
information a server does not have. No `auto` value was added and no default
changed. The catalog grew by about 523 estimated tokens (+3.7%), all in six
schema properties of three tools.

## Consequences

- Smaller responses when asked for, at the price of evidence that sits in
  lower-ranked rows or in source an `overview` does not show. Both are
  signalled; neither is silent (`nothing_is_ever_missed_silently_by_a_lowered_or_cut_answer`).
- Escalating from `overview` to `detailed` costs more than one `detailed`
  call. It pays off only when the overview lets the caller skip the detail.
- A budgeted call is never served from the query cache.
- Rust callers that build `BuildContextPackArgs`, `SearchSymbolsArgs` or
  `HybridSearchArgs` by struct literal must add the two fields. JSON clients
  are unaffected (`tests/budget_schema.rs`).
- Token counts are estimates. No tokenizer for Claude models is published, so
  `EstimatedCounter` cannot be calibrated offline; `examples/token_calibration.rs`
  compares it with the official `count_tokens` endpoint when an API key is
  available (not run in this change: no key). Billed usage is never measured.
