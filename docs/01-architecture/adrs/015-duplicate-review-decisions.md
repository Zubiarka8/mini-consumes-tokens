# ADR-015: Reversible user review decisions for duplicate groups

## Status

Accepted for the local implementation, 2026-10-20. Supersedes ADR-014's original
HTTP read-only restriction for the review-preference endpoint only.

## Context

People need to remove irrelevant groups or false positives from their normal
review queue, without deleting code or losing the underlying analysis. Group
numbers change with ranking and cannot safely identify a decision after restart.
Generated-directory exclusions are a different concern and already belong to
`.mctignore`, shared by indexing, MCP and derived views.

## Decision

The web provides Active and Discarded views, reasons `false-positive` and
`dismissed`, and Restore to active. Filtering and paging apply after review state;
visible file/overview counts describe active groups. The original duplicate
report, CLI output, verdicts and TOON download remain complete and unchanged.

Store optional user decisions in `.mct-index/duplicate-reviews.toon`, version 1,
with a `reviews[n]{key,reason}` table. This is local review preference state,
not source/index data. SQLite remains the source for indexed metadata; no schema
migration, second SQLite index or MCP signature change is needed.

A versioned SHA256 key includes normalization, group language and sorted member
paths/names/kinds/line ranges/source-file content hashes, using length-prefixed
fields. Ranking and member order do not matter. Any member file or membership
change conservatively resurfaces the group. Whole-file hashes mean even unrelated
comments can resurface a reviewed group. Old keys stay available for content
reverts; only currently detected groups appear in the UI. Renames have new keys.

A same-origin `POST review` through the random capability path requires Origin,
a per-process CSRF nonce, a current group key and an allowed action. GET/HEAD never
mutate state. Headers and forms have 16 KiB / 4 KiB bounds and a 2-second read deadline.
No source edit, reindex, arbitrary path, shell execution or vault mutation route.

Each save reloads valid state under an exclusively created per-project lock,
merges the requested key, writes/syncs a unique temporary file and atomically
replaces the old file before updating visible memory. Other writers' keys are
preserved. State is bounded to 8 MiB / 100,000 rows and rejects unknown formats,
malformed/duplicate keys, count mismatches and symlink ancestors. Unix also
rejects hard-linked files; atomic replacement avoids editing a shared inode on
other platforms.
Failures preserve prior decisions and leave the UI unchanged with an error.

`ignore-init --generated` adds optional editable generated-directory rules to
`.mctignore`, ahead of existing overrides, without changing built-in exclusions.
This preserves real source trees when names such as bin are meaningful. Reindex
purges old rows and updates the vault. The public navigation snapshot also
filters stale excluded files/symbols/imports and checks project rule reloads,
so read-only web/vault views cannot expose those rows while waiting for reindex.
The duplicate analyzer already applies the same exclusion set before file reads.

## Consequences and limitations

- Dismissal is reversible and never deletes source, SQLite records or vault notes.
- The TOON file can be backed up locally, but `.mct-index` is normally Git-ignored.
- Currently running web processes refresh other processes' preferences at their
  next save or restart. Source/ignore changes still need a new startup snapshot.
- Same-key concurrent decisions use the last successfully submitted value;
  different keys are merged. No multi-user accounts or audit history are added.
- A crash can leave `duplicate-reviews.lock`; recover only after checking no writer
  is active. Atomic rename is not a guarantee of power-loss durability. Concurrent
  external mutation of the state directory is outside the supported model.
- Obsolete keys are retained rather than silently deleting user decisions. The
  bounded store can eventually require explicit user cleanup.
- Native HTTP/browser validation remains a separate platform check. Automated
  tests cover the complete request/response path with an in-memory transport.
