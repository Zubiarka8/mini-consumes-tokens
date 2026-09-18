# ccm-mcp-server

MCP tools over stdio (`rmcp`), exposing the `ccm-index` SQLite index to any MCP-capable agent.

## Depends on
`ccm-core`, `ccm-index`, every `ccm-lang-*` production crate (registers each via `registry.rs::build_registry`).

## Responsibilities
- Auto-reindexes incrementally at startup.
- Implements the 10 tools listed in [[mcp-protocol-spec]].
- Knows no language's grammar — routes everything through `ccm_core::LanguageRegistry`.

## Related
[[overview]] · [[001-sqlite-storage]]

#crate #mcp
