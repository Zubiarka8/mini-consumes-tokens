# ADR-021: Project writer ownership and explicit index completion

## Status
Writer-coordination design and local Phase 4 implementation recorded 2026-10-25.
The implementation uses one SQLite transaction per reindex run and generation
metadata in the existing `index_meta` key-value table; no schema change or new
migration. Native platform and production acceptance remain gates. Deployment
and native platform acceptance are separate gates.

## Context
- WAL serializes SQL writers, not whole multi-file reindex operations.
- A process-local mutex cannot coordinate CLI and several MCP executables.
- Watcher events can be silently lost; current commits are per file.

## Decision
Use one project-scoped OS advisory file lock and one SQLite write transaction
for each complete index run. Increment `published_generation` in the existing
`index_meta` table in that transaction. Keep `last_full_reconciled_at` separate
and advance it only when a full walk reports no unreadable paths. Periodically reconcile
file hashes independently of notification delivery. Report read failures as
potentially stale, retaining their previous good rows.

Every MCP tool operation that makes related multi-query reads pins one SQLite
read transaction. The process-local async mutex still serializes a watcher run
and tool calls in that same server; other processes can read the old WAL snapshot
while the writer runs. Query-cache validation is fenced by the run generation.
Calls across pagination pages or independent subcalls are not a shared snapshot.

## Alternatives and tradeoffs
PID files and time-based lock stealing are rejected because crashes/PID reuse
make ownership ambiguous. Per-file SQLite commits allow another process's
readers to observe a mixed repository state. A permanent coordinator daemon or
distributed lease is unnecessary. One run-wide transaction increases writer
occupancy and can grow WAL while old readers remain; staging whole generations
would cost a schema migration and duplicate storage. The existing metadata table
is sufficient for a monotonic publication sequence.

## Consequences
OS crash cleanup avoids stale ownership without a custom distributed protocol.
The lock file must not be deleted while in use. Every writable entry point must
cooperate, including embedding refresh and maintenance. Old binaries that ignore
the gate require an explicit rollout/shutdown procedure; the new lock cannot
retroactively make them safe. Periodic hashing costs IO but repairs missing events.


## Reviewed minimum implementation

Use `std::fs::File::try_lock` / `try_lock_shared` (stable since Rust1.89), backed
by flock on Unix and LockFileEx on Windows. No new dependency or custom lease.
Persistent `<database>.access.lock` and `<database>.writer.lock` files live beside
that database, never in a global directory. File handles are RAII guards; drop
or process death releases ownership. Lock existence is never ownership evidence.

A shared access guard lives until after its SQLite connection closes. Exclusive
access is required for migration/maintenance, and therefore waits for connected
readers as well as writers. Read requests never acquire the writer lock. Opens
on an already current schema retain shared access and can coexist. A pending
migration releases provisional handles before exclusive access, rechecks schema,
migrates transactionally, and reopens with shared access. Index operations acquire
access before writer, never the reverse. Hooks run after writer release.

Index whole-run writes and embedding mutations use the writer gate. Independent
databases never share locks. Offline restore remains operator-only; the maintenance
guard is a prerequisite, not a replacement/cutover implementation.

Rejected alternatives: lifetime-exclusive Index/Store handles would prevent
multiple MCP processes unnecessarily; per-SQL locks would interleave
whole index runs; an SQLite row lease needs expiry/stealing logic and schema work.
A single global maintenance mutex would couple unrelated projects and databases.

The protocol does not stop old binaries, direct SQLite connections or filesystem
replacement tools. Native local rollout must identify all executable instances,
verify compatible versions/roots, back up user state, obtain shutdown/upgrade
authorization, then restart only cooperating versions. Do not claim otherwise.
The completed generation is a SQLite transaction boundary, not a frozen
filesystem snapshot: a file changed while parsing aborts the run after a
post-parse content-hash check; source edits after that check converge on a later
event or reconciliation. SQLite transaction recovery covers process
interruption; this does not claim protection from all storage-device or
power-loss failures.

Official reference: [Rust file locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).

Index mutations fail fast on writer contention; callers receive an actionable
error and the existing watcher retry/backoff can retry. This avoids sleeping
inside an MCP process-local Index mutex while another process scans. Opens and
maintenance use bounded five-second acquisitions (not an end-to-end deadline).

## Finding-query integration

Both MCP finding handlers pin the whole request: candidate detection and the
indexed hashes used to resolve review decisions describe one published
generation. The detector's internal snapshot alone ends before the caller
fetches hashes and therefore cannot provide this guarantee by itself.

`tests/live_reindex.rs` keeps a server and its cache alive across external
reindexes, checking changed review fingerprints, current symbols/source and
duplicate membership. `mct-index/tests/writer_coordination.rs` covers writer
exclusion and release, reader/maintenance barriers, independent databases,
contention without publication, recovery, and linked lockfile refusal.

### Dashboard snapshot boundary

`Site::load` holds one read snapshot from navigation through duplicate detection,
review fingerprints, dead-code candidates and reference counts. A concurrent
writer can publish while that reader finishes; the next dashboard reload follows
the new generation. The deterministic dashboard regression publishes between
navigation and the remaining queries, proving both consistency and next-request
visibility without sleeps.
