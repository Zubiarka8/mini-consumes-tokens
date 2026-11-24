---
name: mct-symbols
description: "What a file, directory or crate defines. Use when you do not know a symbol's exact name."
---
Use when: you do not know a symbol's exact name and want everything a file, directory or crate defines. If you know the name, use mct-find instead.
Returns: a count, then one row per symbol: kind, name, path, line range and nesting level.
1. Call list_symbols with path=<arg>. A file path matches exactly; a directory or crate path matches as a prefix. Narrow with kind or language.
2. Report the symbols grouped by file and kind.
3. Once you know the exact name, move on: mct-find for its definition, mct-context before editing it.
No MCP client? Run `mct-cli --root . call --tool list_symbols --args '{"path":"<arg>"}'`.
Do not read whole files.
