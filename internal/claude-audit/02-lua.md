# Agent 02 — enable Lua

Follow the shared rules. Branch: `codex/audit-lua`. Goal: resolve F04 by connecting the existing Lua parser to the CLI and MCP server.

Context: `mct-lang-lua` exists and has tests; `build_registry` in `crates/mct-cli/src/main.rs` and `crates/mct-mcp-server/src/registry.rs` omits Lua. The fixture `function greet()\n print("hello")\nend\n` does not appear in status or `find_symbol` with the distributed executables.

Scope: CLI/server language registrations, their `Cargo.toml` files, workspace `Cargo.toml`/`Cargo.lock` only if wiring requires it, end-to-end tests, and the support table in `README.md`/`internal/checklist.md`. Do not refactor the whole catalog, change the Lua grammar, or change index internals. Keep `.codex`, `.mcp.json`, and global configuration out of the commit.

Register Lua through the existing internal dependency. Add a check that uses the real production registries so a manually constructed test registry cannot hide an executable registration omission. Verify CLI/MCP parity for languages and extensions through public mechanisms.

Acceptance: `probe` accepts the Lua fixture; `init`/`status` count Lua; `find_symbol` finds `greet` with a source range; Lua calls are queryable through MCP; no extension is duplicated and no other parser is displaced. Update language counts where needed. Deliver a local commit, actual results, and a small proposed PR.
