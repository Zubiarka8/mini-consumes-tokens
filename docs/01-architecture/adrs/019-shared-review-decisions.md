# ADR-019: Shared review decisions across the dashboard, the CLI and MCP

## Status

Accepted for the local implementation. Partly supersedes ADR-015: the
duplicate-group decision file is no longer written. ADR-015's semantics
(reversible decisions, a notice that the group is still analysed) are kept.

Number: `017` is free on `origin/main` but is claimed by an uncommitted draft
(`markdown-link-folding-and-alias-backlinks`), and `018` by the uncommitted
web-dashboard draft. This ADR takes `019` so that neither draft has to move.

## Context

Review decisions about duplicate groups (ADR-015) and dead-code candidates
were stored in per-kind files and read by the web dashboard only. The CLI and
the MCP server ignored them, so the same candidate could be shown as open in
one surface and as rejected in another, and an agent re-reviewed findings a
person had already rejected.

## Decision

- One canonical store: `.mct-index/review-decisions.toon`, `version: 3`. Only
  this file is written. Times are Unix milliseconds.
- Read-only compatibility with the earlier files: `duplicate-reviews.toon`
  (`A1` version 1, `A2` version 2, in seconds) and the unmerged draft
  `reviews.toon` (`B2`, version 2, in seconds). They are never rewritten.
  Legacy seconds are multiplied by 1000 when read; they are never reinterpreted.
- A finding is hidden only by a `false-positive` or `dismissed` decision whose
  identity and exact code fingerprint both match. Changed code shows as
  `needs-reevaluation`. A finding without a content hash is never hidden.
- Conflicts are resolved by a fixed total order: latest time first, then the
  outcome that keeps the finding visible, then the newer format
  (`C3` > `B2` > `A2` > `A1`). An ambiguous legacy key decides nothing.
- Saves use an exclusive lock, a revision check (HTTP 409 on a stale view), a
  logical time of at least the last recorded time plus one millisecond, a
  check that the history only grows, a `.prev` copy, and an atomic rename.
- Any unreadable review file puts the store in degraded mode: nothing is
  hidden and every save is refused until the file is fixed or moved aside.

## Consequences

- Dashboard decisions written to `review-decisions.toon` are not visible to
  binaries that only read `duplicate-reviews.toon`. Those binaries keep the
  decisions they had, and they do not see later ones. Output compatibility
  (identical default results) is kept; bidirectional decision interoperability
  is not.
- `duplicates --reviewed` and `dead-code --reviewed` bypass the duplicate cache
  when the flag is set, so a review decision is never stale.
- A store is capped at 32 MiB. At about 170 bytes per row this is roughly
  190,000 rows in total; a save beyond it is refused, nothing is dropped.
- Loading 100,000 canonical rows costs about 100 ms and about 98 MB of peak
  memory, above the design targets. Measurements and the candidate fixes are in
  `benchmarks/review-decisions.md`.

## Alternatives rejected

- Keep one file per kind and merge at read time: two writers could still lose
  decisions, and the ordering rule would be implemented twice.
- Rewrite the legacy files into the new format: this would break old binaries
  that a person still runs, and it destroys the evidence of earlier decisions.
