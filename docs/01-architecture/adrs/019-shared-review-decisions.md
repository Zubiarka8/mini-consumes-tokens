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
  `needs-reevaluation`. A finding without a content hash is never hidden; an
  empty stored hash counts as no hash.
- Conflicts are resolved by a fixed total order: latest time first, then the
  outcome that keeps the finding visible, then the newer format
  (`C3` > `B2` > `A2` > `A1`). An ambiguous legacy key decides nothing.
- Saves use an exclusive lock, a revision check (HTTP 409 on a stale view), a
  logical time of at least the last recorded time plus one millisecond, a
  check that the history only grows, a `.prev` copy, and an atomic rename.
- Any unreadable review file puts the store in degraded mode: nothing is
  hidden and every save is refused until the file is fixed or moved aside.

### Locking and crash recovery (protocol change, CR-16)

- The writer lock is an operating-system advisory lock (`File::try_lock`) on
  `.mct-index/review-decisions.lock`. The kernel releases it when the holder
  exits, however it exits. A killed writer therefore never blocks the next one.
- The lock file is never removed. The earlier protocol created it with
  `create_new` and removed it on drop, so a killed writer left it behind and
  every later save was refused until a person deleted it (observed in 135 to
  136 of 150 kills). That protocol is replaced; no release of `mct-cli` shipped
  it.
- A live writer makes the save fail with `Locked`, naming the path. Nothing is
  written, and nobody has to remove a file by hand.
- A save removes the temporary files that killed saves left, while holding the
  lock. Only regular files named exactly as this module creates them, for the
  canonical file and its backup, are removed. `.prev`, directories, links and
  unknown names are kept. Loading never removes files. A removal that fails is
  ignored and the file is counted by the next load. Limitations: a temporary
  file is up to 32 MiB. One save runs at a time, so a kill leaves at most one
  such file, and the next save removes it.
- Old binaries are not part of this protocol: `duplicate-reviews.lock` and
  `reviews.lock` belong to them and are untouched.

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
- With 100,000 canonical rows a query adds about 43 ms (load about 41 ms and
  resolution about 2.5 ms) and about 54 MB of peak memory (about 52 MiB), within
  the design targets of 50 ms and 64 MiB. The store is read line by line, so the
  file text is never held whole. Measurements are in
  `benchmarks/review-decisions.md`.

## Alternatives rejected

- Keep one file per kind and merge at read time: two writers could still lose
  decisions, and the ordering rule would be implemented twice.
- Rewrite the legacy files into the new format: this would break old binaries
  that a person still runs, and it destroys the evidence of earlier decisions.
