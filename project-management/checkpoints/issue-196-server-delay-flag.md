# Checkpoint: issue-196-server-delay-flag

Branch: claude/issue-196-server-delay-flag (from origin/main 562633a)

## Done
- New `crates/mct-mcp-server/src/reindex_delay.rs`: pure `parse_reindex_delay(&str) -> Result<Option<Duration>, String>` (None = off) plus clap adapter `parse_reindex_delay_arg` / `ReindexDelay`.
- `main.rs`: `--reindex-delay <VALUE>` (15s, 30s, 45s, 1m, `<N>s|m|ms`, `off`); `--reindex-debounce-ms` is now `Option<u64>`; the two conflict via clap; default stays 1500 ms; both resolve to the Duration passed to `background::spawn_watcher`.
- Zero, negative, non-numeric, unknown units, and overflow are rejected; zero error points to `off`.

## Remaining
- Docs, integration tests, CHANGELOG, mct-cli (out of this slice's scope).
