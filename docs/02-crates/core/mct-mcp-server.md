# mct-mcp-server

MCP tools over stdio (`rmcp`), exposing the `mct-index` SQLite index to any MCP-capable agent.

## Depends on
`mct-core`, `mct-index`, every `mct-lang-*` production crate (registers each via `registry.rs::build_registry`).

## Responsibilities
- Auto-reindexes incrementally at startup (full walk, content-hash skip).
- While running, keeps the index fresh from filesystem events (`background.rs`, issue #25): each settled, debounced batch (`--reindex-debounce-ms`, default 1500; `0` disables the watcher) updates **only the changed paths** through `Index::reindex_paths` — changed/created files re-parsed, deleted or renamed-away files and directories dropped, directories created or renamed into place walked. A file that no longer parses loses its previous symbols (only the syntax error stays) until it parses again. macOS reports only the new path of a rename, so for each newly indexed path the likely old ends — indexed files with the same content hash, and indexed paths in the same directory — are checked and dropped if gone; an edit or a deletion checks nothing else, so no update costs anything per indexed file. A case-only rename on a case-insensitive filesystem (macOS, Windows) re-indexes under the new spelling; symlinked directories are never walked. Falls back to a full walk when the platform asks for a rescan, the watcher reports errors, more than `MAX_INCREMENTAL_PATHS` (256) paths changed at once, or an earlier update failed (retried with backoff even without a new event, so a failed batch is never lost). Single-file update p95 on this repo's `crates/`: 14 ms edit, <2 ms create/rename/delete, vs 79 ms for a full walk; on a synthetic 50 000-file tree (`-- --synthetic 50000`): 6.8 ms edit, 19 ms create, 16 ms rename, 0.8 ms delete, vs 6.1 s (`cargo run --release -p mct-mcp-server --example incremental_benchmark`). Correctness against a fresh full index is checked in `crates/mct-index/tests/incremental.rs`. Known gap: a file moved to another directory *and* edited within one debounce window, or moved into an excluded directory, when only the new path is reported, keeps its old rows until the next full reindex (at startup, the `reindex` tool, or `mct-cli reindex`).
- Implements the 10 tools listed in [[mcp-protocol-spec]].
- Knows no language's grammar — routes everything through `mct_core::LanguageRegistry`.

## Related
[[overview]] · [[001-sqlite-storage]]

#crate #mcp
