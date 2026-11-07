# ADR-029: Opt-in idle sleep of background maintenance

## Status
Proposed (2026-11-07). Plan: [idle resource usage](../../plans/2026-11-07-idle-resource-usage.md), decision D5. Builds on [ADR-028](028-one-maintainer-per-index.md).

## Context
- An MCP session lives as long as its assistant conversation, often for hours or days, used or not. Its watcher and periodic reconciliation run the whole time.
- Exiting the process when idle is not an option. Claude Code and Codex do not reliably relaunch a stdio server that exits mid-session, so the next tool call would find a closed connection.
- Dropping the watcher's event channel counts as an unexpected disconnection, which schedules one final full reconciliation. Sleeping needs a stop that does not.

## Decision
- **Stop path.** `background::spawn_watcher_with_reconciliation` returns a `WatcherHandle`. `stop()` sets a flag and closes the event stream. An update already running is allowed to finish rather than aborted. No reindex starts after `stop()` returns, and no final reconciliation runs. `stop()` joins the indexing thread. The event callback no longer blocks on the full one-slot channel: it sets an "overflowed" flag, which the worker turns into a full walk, so stopping never waits on a stuck callback. Dropping the handle without `stop()` keeps the old behaviour, a final reconciliation.
- **Flag.** `--maintenance-idle <duration|off>`, with `MCT_MAINTENANCE_IDLE` as the environment fallback. The default is `off`. The dashboard never sleeps.
- **Activity.** Every tool call that reads the index records the time and, if maintenance sleeps, wakes it; `batch` counts per sub-query. Index-free tools (`get_indexing_status`, discovery, `select_tools`) do neither.
- **Sleep.** After the timeout, the maintainer thread stops its watcher through `stop()`, **releases its lease**, and stops retrying it. This deviates from the plan's "keeps its lease": a sleeping owner that kept the lease would leave an active follower's queries unwatched. Releasing it lets the active process take over within one retry interval.
- **Wake.** The first index-using request marks the state `Waking` and waits; later requests wait on the same refresh. The maintainer thread takes the lease if it is free and starts the watcher before reconciling, so no edit falls in a gap. It then runs one hash-skipped full reindex, which covers edits, additions, deletions, renames and ignore-rule changes. Only then does it release the waiting requests. If the lease is held elsewhere, it reconciles as a follower and resumes retrying.
- **Failure.** If the wake reconciliation fails, the waiting request fails with the reason and a hint to call `reindex`, as a failed exclusion sync already does. Later requests are answered from the index as it stands.

## Consequences
- An idle session costs no scans and no watcher events. A sleeping process holds only a parked thread.
- The first call after a sleep pays for one reconciliation. On this repository that is at most 1 s with a release build (measured 2026-11-07).
- Idle time is counted from the start of the last request, not from its end. A single request longer than the timeout can be followed by a sleep, which the next request then wakes.

## Alternatives rejected
- Exiting the process: depends on unverified client relaunch behaviour (plan, E8).
- Keeping the watcher and dropping only the reconciliation: the watcher is the part that reacts to build churn.
- Marking the index dirty while asleep and refreshing only changed paths: needs the watcher running, which is the cost to remove.

## Verification
- `crates/mct-mcp-server/tests/background_watcher.rs`: `stop()` with a saturated channel returns, and nothing is indexed afterwards. A dropped handle still reconciles once.
- `crates/mct-mcp-server/tests/maintainer.rs`: no publication while asleep. The first wake sees an addition and a deletion. Eight concurrent wakes produce one refresh. A sleeper hands the index to an active process.
- `crates/mct-mcp-server/src/main.rs` tests: flag and environment resolution.
