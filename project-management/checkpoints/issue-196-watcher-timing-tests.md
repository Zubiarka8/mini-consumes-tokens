# Checkpoint: issue-196-watcher-timing-tests

Branch: claude/issue-196-watcher-timing-tests. Slice of issue #196.

Added `crates/mct-mcp-server/tests/reindex_delay_timing.rs` (quiet period 2 s).
Updates are counted through a global tracing layer on the watcher's
"auto-reindex after file changes settled" event (spawn_watcher takes a concrete
`Index`, not a `Reindexer`).

| Case | Status |
| --- | --- |
| a) no update before the quiet period | passing |
| b) second save resets the period | ignored: bug |
| c) burst of saves = one update | ignored: bug |
| d) changes during a running update applied afterwards | passing (index lock held to model a running update) |
| e) .mctignore / .gitignore files never trigger an update | passing |

Bug (b, c): `notify-debouncer-full` releases events per age, not after quiet, so
a later save does not restart the period and a 1.2 s burst gave 4 updates.
Fix belongs in `src/background.rs` (trailing-edge debounce); then remove the two
`#[ignore]`s.
