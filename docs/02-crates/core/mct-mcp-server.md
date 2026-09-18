# mct-mcp-server

MCP tools over stdio (`rmcp`), exposing the `mct-index` SQLite index to any MCP-capable agent.

## Depends on
`mct-core`, `mct-index`, every `mct-lang-*` production crate (registers each via `registry.rs::build_registry`).

## Responsibilities
- Auto-reindexes incrementally at startup.
- Implements the 10 tools listed in [[mcp-protocol-spec]].
- Knows no language's grammar — routes everything through `mct_core::LanguageRegistry`.

## Related
[[overview]] · [[001-sqlite-storage]]

#crate #mcp
