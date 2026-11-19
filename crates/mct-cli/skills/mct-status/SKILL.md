---
name: mct-status
description: "Health of the index: parse failures, read failures, dependencies and staleness. Use when results look wrong or incomplete."
---
Use when: results look incomplete, a file you know exists is missing, or after upgrading. Also a cheap first check in a new session.
Returns: file and symbol totals, the last indexed time, coverage per language, and any syntax error, read failure or unsupported language.
1. Call get_indexing_status.
2. Report the file and symbol counts, any syntax or read failure with its path, and whether the index looks stale.
3. A file that fails to parse is missing from search results; say so whenever an answer depends on it.
4. Suggest mct-reindex only if it looks stale.
No MCP client? Run `mct-cli --root . status`.
