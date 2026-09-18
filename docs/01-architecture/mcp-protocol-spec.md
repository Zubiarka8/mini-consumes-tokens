# MCP Protocol Spec

`mct-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

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
| `get_project_overview` | discovery | Compact hierarchical digest of a file/directory/crate/project |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`.

`get_project_overview` accepts `path` (optional, same matching semantics as `list_symbols` — omitted means the whole project root), `language`, `max_symbols_per_module` (default 8, caps top-level symbols surfaced per file), and `include_relations` (default true, whether to show each surfaced symbol's top callers). It composes `list_symbols` + `find_callers` internally — no new SQL, no new schema — ranking truncated modules by call fan-in.

See [[mct-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

#architecture #mcp
