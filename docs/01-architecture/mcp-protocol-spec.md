# MCP Protocol Spec

`ccm-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

## Tools

| Tool | Type | Purpose |
|---|---|---|
| `list_symbols` | discovery | List symbols under a file or directory/crate prefix |
| `find_symbol` | atomic | Exact-name symbol lookup |
| `find_references` | atomic | Every reference to a symbol |
| `find_calls` | atomic | What a function calls |
| `find_callers` | atomic | What calls a function |
| `impact_analysis` | composite | Full blast radius in one call |
| `get_file_skeleton` | discovery | Top-level shape of one file, bodies collapsed |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`.

See [[ccm-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

#architecture #mcp
