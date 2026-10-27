# ADR-020: Consistent user-state snapshots and isolated recovery verification

## Status
Local manual backup, Phase3 management and isolated recovery implementations in
review. Whole-system/native-platform and production operational acceptance remain
pending. No automatic production replacement. See [SDD](../storage-reliability-spec.md).

## Context
- Dashboard annotations are irreplaceable; analysis is reconstructible.
- Committed rows may exist only in WAL. File existence is not backup validity.
- Existing schema migrations and content identities must remain authoritative.

## Decision
Keep both databases. Snapshot only dashboard user data with SQLite Online Backup
through a read-only source connection without migration/import. Pin a source read
snapshot, use bounded copy steps/retries, then release it before verification.
Validate integrity, foreign keys and current schema, reopen the artifact, hash it
and atomically publish a TOON completion manifest in a private unique directory.
Verify restoration in a disposable database before considering operator cutover.
No automatic production overwrite. Retention/scheduling are a subsequent slice.

## Alternatives and tradeoffs
Copying just the main DB is rejected because WAL may contain committed changes.
Clean-shutdown copying requires verified exclusive shutdown and remains an
operator fallback. `VACUUM INTO` is consistent and compact but rewrites every
page and offers less convenient stepped cancellation; it remains an alternative.
Online Backup is already supported by rusqlite via a feature flag, without a
new service. A pinned reader prevents restart starvation under continuous writes,
but can temporarily retain WAL pages; bound copy duration/size and monitor it.

## Consequences
Snapshots protect only their captured state; bounded history is not arbitrary
point-in-time recovery. SHA-256 detects accidental changes, not authenticity.
Owner-controlled directories and OS permissions protect confidentiality; Windows
ACL validation remains required. Completion publication is a manifest boundary,
not a claim that multi-file production cutover is atomic.

## Phase 3 decision: one-shot managed maintenance

Approved for local implementation on 2026-10-25 under the owner request.
`dashboard-backup-manage --enabled` performs one due tick. An owner-installed
cron/launchd/Task Scheduler trigger calls it with the same explicit policy flags.
The command is disabled by default; this change installs no schedule. We reject
an in-process-only timer because protection would disappear when MCP/web closes,
and reject a daemon because the OS scheduler already provides process lifecycle.
A hybrid can reuse this tick later if a measured UX requirement justifies it.

The destination owns a separate cooperative management lock and atomic TOON
scheduling state bound to a canonical source digest. Manual snapshots share that
lock. Shared database access still permits dashboard WAL writes and MCP reads.
Missed intervals coalesce into one attempt; persisted UTC success/attempt/retry
cross restarts, monotonic copy/lock deadlines bound contention. Wall-clock
rollback refuses mutation. One attempt per invocation, exponential five-minute
retry delay capped at one hour; no retry worker or aggressive loop.

Managed receipts identify retention ownership and the exact scheduled attempt.
A temporary manifest supports isolated verification before atomic completion
publication. A crash after publication can be adopted without a second snapshot;
missing final manifests remain incomplete. All readers revalidate recovery points
rather than trusting receipts/state. Preserved, manual, foreign, incomplete,
corrupt and unknown-content bundles are never automatically deleted. Quotas may
conflict with safety minima; report and refuse further growth once already full,
rather than sacrificing protected data. Directory capacity cannot be reserved
portably: one new verified point may temporarily exceed quota before retention.

Isolated export compares every table row/type and refuses an existing destination.
It reserves a new private directory and publishes its validated database; it is
not an atomic production cutover or an automatic replacement command. Process
termination releases OS ownership; incomplete output requires operator inspection.
Native Windows ACL/directory-flush and Linux/Windows process acceptance remain
required. Advisory ownership does not constrain legacy/external nonparticipants.
