# Agent 01 — reload exclusions

Follow the shared rules. Branch: `codex/audit-exclusions`. Goal: resolve F03 so an exclusion added after the server starts removes the file from the index without restarting MCP.

Reproduction: index `hidden.rs` containing `pub fn audit_hidden_symbol() {}`, start MCP, add `hidden.rs` to `.mctignore`, and call `reindex(force=true)`. `find_symbol` still returns the symbol. A new `mct-cli` process removes it.

Explore `Index::reindex`, `indexer::reindex`, `ExcludeSet`/`read_ignore_file`, `spawn_watcher`, and initialization in `main`. Scope: `crates/mct-index/src/{lib.rs,indexer.rs,exclude.rs}`, related tests, `crates/mct-mcp-server/src/{background.rs,main.rs}`, and watcher tests. You may adjust only the MCP `reindex` method in `server.rs` if necessary; do not change search, formats, or language registrations.

Reload rules consistently for the index and watcher. Detect `.mctignore` changes and `.gitignore` changes when import is enabled. A path that was excluded may produce no useful events when it becomes included, so a rule update must recover all relevant state. Do not maintain two exclusion sets that can diverge.

Acceptance: adding and removing a rule works while the server is running; `force` respects new rules; symbols/relations for excluded files disappear; ordinary changes remain incremental; index events do not create a loop. Add regression tests, including an imported `.gitignore` case. Keep MCP contracts and schema unchanged. Deliver a local commit and proposed PR.
