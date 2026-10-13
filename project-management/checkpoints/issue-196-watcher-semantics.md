# Issue 196 watcher semantics audit

Scope: `crates/mct-mcp-server/src/background.rs` (`spawn_watcher`, `BatchState`, `Reindexer`, `changed_paths`) against the five acceptance criteria.

## Findings

1. Quiet period resets on every relevant change: CONFIRMED GAP, fixed.
   `notify_debouncer_full` settles each file independently (`event.time` of that file's own last event vs. the timeout, `lib.rs` ~L221). With delay D, a change to A at t=0 and to B at t=0.6D released A at D while B was still changing. The delay was effectively per-file, not project-wide.
   Fix: the debouncer now only coalesces noise (`min(D, 500 ms)`); the watcher thread holds merged pending changes and waits `D - coalesce` after the last relevant batch, restarting on each one. Total latency after the last change stays about D.
2. Burst grouped into one update: CONFIRMED GAP (same cause), fixed by `merge_changed` (union, dedup, rescan absorbs all, cap -> rescan).
3. Changes during an update: HOLDS. One worker thread runs updates serially and takes `index.blocking_lock()`, so updates (and the `reindex` tool) never overlap; events queue in the unbounded mpsc channel and are read after the update. Unchanged.
4. Exclusions: HOLDS. `changed_paths` filters via `ExcludeSet` and returns `Changed::Nothing`; the new loop only restarts the deadline when the batch is not `Nothing`, so excluded or access-only events neither trigger nor extend the wait. Rules-file edits still reload and rescan.
5. Long delays: one confirmed issue, fixed. `MAX_RETRY_DELAY` (60 s) capped the failure retry below a longer D (e.g. 5 min), retrying sooner than the quiet period. Now `min(max(60 s, D))`. Debouncer tick is timeout/4, which is no longer a concern because the debouncer timeout is at most 500 ms. No other short-debounce assumptions found (no timeouts besides these).

## Intentional / unchanged

- Debouncer errors still force a full rescan, and now also participate in the quiet period (they are relevant).
- With `D <= 500 ms` the quiet wait is zero: behavior equals the previous one (each debounced batch flushes immediately), so existing timing tests are unaffected.
- A failed update's owed-rescan retry only runs while no changes are pending.

## Verification

Inline unit tests: `merge_changed` (burst dedup, Nothing/Rescan, cap) and long-delay `retry_delay`. See commit and final report for check results.
