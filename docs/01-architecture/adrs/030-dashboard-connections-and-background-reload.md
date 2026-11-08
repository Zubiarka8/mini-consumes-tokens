# ADR-030: Dashboard connections and background snapshot reload

## Status
Proposed (2026-11-08). First block of the vault and web performance plan (branch `perf/vault-server`).

## Context
- `mct-cli web` served one connection at a time from its accept loop. A connection that sent nothing (a browser preconnect, a stalled tab) held every other request until the 2 s request timeout. Measured on a 20,001-file project: an idle connection then `/api/meta` took 1,969 ms (p50, n=5).
- A published reindex was reloaded inside the next request (`serve_connection -> follow_reindex -> reload`), so that request waited for a full snapshot build: 8,545 ms p50 on the same project, 20,695 ms with the Obsidian vault enabled.
- `Site` owns a SQLite connection and per-session state (`RefCell` latency samples, review decisions, the CSRF token). It is `Send` but not `Sync`.

## Decision
- **Connections.** The accept loop runs on its own thread and hands each connection to a thread of its own, at most 64 at once; beyond that the connection gets 503 and is closed. That thread reads and validates the request within the existing timeout and size limits (`http::read_request`), with no access to the site.
- **One owner of the site.** Complete requests go through a channel to the thread that owns `Site`, which answers them one at a time and sends the response back for the connection thread to write. Review decisions and `.mctignore` edits stay serial, and `Site` needs no lock.
- **Background reload.** Before each request, the owner checks whether the index was republished (the same rule as before: a replaced database file, or a new published generation). If so, it starts `Site::load` on another thread and keeps answering from the current snapshot. The first request after the build finishes swaps the new snapshot in, keeping the CSRF token and latency samples. A failed build records the stamp as tried, as a failed synchronous reload did.
- **Decisions during a reload.** The owner counts POST requests. A snapshot whose build started before the latest one may not contain it, so it is discarded and built again.

## Consequences
- An idle connection no longer delays others: 39.6 ms p50 (target 50 ms). The first request after a reindex answers from the previous snapshot in 56.7 ms p50; the change is visible 2,158 ms p50 after the file is saved (11,221 ms before).
- Responses may describe the previous index generation until the new snapshot is ready. They never mix generations: each request is answered from one `Site`.
- Each request pays a thread start and two channel hops: `/api/notes` steady state 2.50 ms vs 2.22 ms (median of three alternating runs of 30 requests).
- A build runs alongside request handling, so memory briefly holds two snapshots.
- Shutdown is unchanged: Ctrl-C ends the process; open connection threads end with it.

## Alternatives rejected
- A shared `Arc<RwLock<Site>>` with handler threads: `Site` is not `Sync` (`RefCell`, SQLite connection), and review writes would need their own coordination.
- An async runtime for the dashboard: a large dependency and rewrite for a local, low-traffic server.
- Shortening the request timeout: reduces, but does not remove, head-of-line blocking, and penalizes slow legitimate clients.

## Verification
- `crates/mct-cli/src/web/tests.rs`: `an_idle_connection_does_not_delay_other_requests` (real socket), `a_background_reload_keeps_answering_from_the_current_snapshot`, `a_decision_saved_during_a_background_reload_is_not_lost`, `a_failed_background_reload_keeps_the_current_snapshot`; existing wire-format tests drive `serve_connection`, which composes the same reading and writing functions.
