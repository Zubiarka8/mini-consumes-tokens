# ADR-028: One background maintainer per index

## Status
Proposed (2026-11-07). Plan: [idle resource usage](../../plans/2026-11-07-idle-resource-usage.md), decisions D1 and D4.

## Context
- Every process that serves an index runs its own filesystem watcher and its own periodic full reconciliation: each MCP session (one per Claude Code or Codex conversation) and the `mct-cli web` dashboard. On 2026-11-07 one project had five MCP sessions and one dashboard, so six processes walked the same tree every five minutes and reacted to every edit.
- The writer lock (`<database>.writer.lock`) serializes writes but never decides who maintains. It is taken per write operation, not held.
- Until the change below, each of those reconciliations also wrote `index_meta` and advanced `published_generation`, even when nothing changed. That invalidated the generation-keyed caches of every other process and made the dashboard reload its whole snapshot.

## Decision
- **No-op runs publish nothing (D1).** A reindex publishes a generation only when it wrote a row (counted with `total_changes` inside the run's transaction), when the exclusion rules changed, or when the index was never published. Manifest and issue writes skip rows that would not change. Otherwise the transaction commits empty and the WAL is untouched. The post-reindex hook runs after a published run, and once per process. The explicit `reindex` tool still refreshes it every time. `last_indexed_at` means the last published change. Each process also keeps a process-local `last_verified_at`.
- **Lease.** `Index::try_acquire_maintainer` takes a non-blocking exclusive OS lock on `<database>.maintainer.lock`, through the same coordination code as the other locks (private 0600 file, no symlinks or hard links). The lock lasts as long as the returned `MaintainerLease` and is released by the OS when its process exits or crashes. The file only records the holder's process id for diagnostics; the lock is the lease. An in-memory index always gets a lease.
- **Roles.** `mct_mcp_server::maintainer::spawn` runs in every MCP server whose watcher is enabled, and in the dashboard. The holder runs the existing watcher and reconciliation. A follower serves queries, keeps the explicit `reindex` tool, and retries the lease every 30 s plus up to 10 s of jitter, parked in between rather than polling. Edits made before a watch is registered produce no event, so on takeover the new owner starts its watcher first and then runs one hash-skipped full reindex. At first start, the first reconciliation runs one debounce after the watcher starts, instead of a full interval later. That covers edits since the startup reindex and keeps the `--reindex-delay` semantics.
- **Freshness of followers.** Followers read the same database file. Their caches are validated against `published_generation`, so the owner's publications reach them on their next query.
- **Fallback.** If the lease cannot be coordinated at all (the lock file cannot be opened or locked), the process maintains anyway, as before.

## Consequences
- One full scan per interval per index, however many sessions and dashboards are open. An edit is parsed once, not six times.
- An edit made between the owner's exit and a takeover is indexed up to 40 s late. Explicit `reindex`, and the `mct-cli call` refresh, still run in any process.
- Old binaries do not know the lease and keep maintaining on their own. They interoperate through the writer lock as before.
- The dashboard is usually a follower now. `mct-cli web` without any MCP server becomes the owner.

## Alternatives rejected
- PID marker files: a crashed owner leaves them behind, and reused process ids make them lie.
- Electing the owner through a row in SQLite: needs heartbeats and expiry, and writes the WAL on every heartbeat, which is the cost D1 removes.
- Holding the writer lock for the owner's lifetime: it would block every explicit reindex in other processes.
- Exiting idle sessions: Claude Code and Codex do not reliably relaunch a stdio server that exits (plan, E8).

## Verification
- `crates/mct-index/tests/noop_reindex.rs` and `post_reindex_hook.rs`: D1.
- `crates/mct-index/tests/maintainer_lease.rs`: one holder, release on drop and on SIGKILL, no blocking of writers.
- `crates/mct-mcp-server/tests/maintainer.rs`: one owner of two, takeover, catch-up after a gap.
- `crates/mct-mcp-server/tests/background_watcher.rs`: an idle reconciliation without change publishes nothing.
