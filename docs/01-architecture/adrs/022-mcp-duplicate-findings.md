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
- No notes, history or source bodies are returned. Stable group ids and member
  references include 1-based global member positions, byte columns and subgroup
  ids. Bounded aligned token differences, context, types, priority and verdict
  reasons accompany them. These are syntactic evidence, not proof of extraction
  safety; type compatibility remains unknown. Use context tools for full source.
- Output is bounded by validated limits (`limit` 1..100, `max_tokens`
  512..16000, root-relative `path` without traversal) and an estimated token
  budget. Counts come before pagination, and truncation is explicit
  (`truncated`, `next_offset`, `members_omitted`): a group whose members were
  cut keeps `next_offset` at that group. Continue using `continuation_group`
  as `group_id`, `next_member_offset` as `member_offset`, and `snapshot` as
  `expected_snapshot`, retaining the original filters and normalization. A
  focused group query returns at most one group; its `next_offset` resumes
  ordinary group pagination after its final member. Nonzero group `offset`
  and `group_id` cannot be combined. Member offsets without a group are rejected.
- `members_omitted` counts members outside the current member window, including
  earlier pages. Member indices always address the whole group. Subgroup summary
  caps do not trim the engine's complete member-to-subgroup map; every paged
  member carries its subgroup id. Single and all-distinct partitions need no
  summary rows. Summary row/member and evidence-value omissions are explicit.
  `truncated` describes group/member pagination; evidence-specific omission
  counters and `evidence_omitted` describe the bounded evidence view separately.
  A page verdict counts the group even when only part of its member list is shown.
- Responses identify the project, binary version (build revision is unavailable
  unless supplied by the build), effective filters and published generation.
  `snapshot` hashes effective analysis inputs/results and review resolutions;
  it excludes page/budget parameters. `expected_snapshot` rejects a changed
  analysis or review result so the consumer can restart instead of mixing pages.
  This detects changes, not a historical snapshot or cross-request transaction.
- Detected groups, members, distinct files and subgroups describe the complete
  analysis. Actionable/suppressed totals precede review filtering. Verdict tables
  distinguish analysis, eligible groups and the returned page. A partial group
  counts as one returned group representation and one page verdict.
- Candidate coverage refers only to indexed candidates in the chosen scope;
  `index_errors` describes the whole index, independently of query filters.
  Unindexed files are not discovered. At small budgets metadata/evidence may be
  omitted with explicit flags. Increasing the budget recovers that detail;
  member continuation also works for groups larger than the maximum budget.
  `skipped`, `index_errors`, `unsupported_languages` and the `skips` table are
  coverage signals, not evidence: they are present at every budget, as before
  this change.
- A cursor always advances. When not even one member row fits the budget (paths
  are never clipped), the response reports `next_member_offset: none` and
  `next_offset: none` with `truncated: true` and a reason to raise
  `max_tokens`, instead of repeating the same member offset.
- Breaking presentation changes for positional readers: the `members` table now
  starts with `member_index` and adds `column`, `subgroup` and
  `symbol_truncated`; symbol names longer than 160 characters are clipped and
  flagged. Read TOON tables by header, not by column position.
- Review validity states whether a decision exists, matches the current
  fingerprint, needs reevaluation, or is held by an unreadable review source.
  Fingerprints deliberately retain whole-file hashes and member ranges. Unrelated
  edits or line shifts can require reevaluation; changed membership/paths/names
  normally create a new identity. Old decisions remain stored; no automatic
  suppression inheritance or history migration is introduced.

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

The 2026-11-07 evidence/continuation change extends those regressions with a
700-member group, complete member recovery across group pages, budgets from 512
to 16000, changed-snapshot rejection and review lifecycle/history preservation.
The duplicate quality suite also checks provenance, member indices, types,
subgroups and analysis verdicts. Its token baseline was deliberately regenerated
with the repository evaluator: the five default queries total approximately
4405 response tokens (previously 1462), including 1328 for two groups (previously
373). The catalog is approximately 12167 tokens (previously 12033). Scores stay
at 1.0; no accuracy or budget threshold was relaxed. This is the measured cost
of returning previously omitted evidence and provenance, not a token-saving
claim. Smaller budgets explicitly omit detail; the maximum budget is unchanged.
