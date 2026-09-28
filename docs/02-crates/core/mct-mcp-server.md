# mct-mcp-server

MCP tools over stdio (`rmcp`), exposing the `mct-index` SQLite index to any MCP-capable agent.

## Depends on
`mct-core`, `mct-index`, every `mct-lang-*` production crate (registers each via `registry.rs::build_registry`).

## Responsibilities
- Auto-reindexes incrementally at startup (full walk, content-hash skip).
- While running, keeps the index fresh from filesystem events (`background.rs`, issue #25): each settled, debounced batch (`--reindex-debounce-ms`, default 1500; `0` disables the watcher) updates **only the changed paths** through `Index::reindex_paths` — changed/created files re-parsed, deleted or renamed-away files and directories dropped, directories created or renamed into place walked. Any indexed file no longer on disk is dropped even if its event was lost (macOS reports only the new path of a rename). Falls back to a full walk when the platform asks for a rescan, the watcher reports errors, or more than `MAX_INCREMENTAL_PATHS` (256) paths changed at once. A single-file update is p95 7.6 ms vs 42 ms for the previous full walk on this repo's `crates/` (`cargo run --release -p mct-mcp-server --example incremental_benchmark`); correctness against a fresh full index is checked in `crates/mct-index/tests/incremental.rs`.
- Implements the 10 tools listed in [[mcp-protocol-spec]].
- Knows no language's grammar — routes everything through `mct_core::LanguageRegistry`.

## Related
[[overview]] · [[001-sqlite-storage]]

#crate #mcp
