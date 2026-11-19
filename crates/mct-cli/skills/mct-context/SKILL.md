---
name: mct-context
description: "Everything needed before editing a symbol: definition, callers, callees, dependencies and tests, in one call. Use before you start working on it."
---
Use when: you are about to edit one symbol and want everything around it at once. One call replaces a definition lookup, callers, callees and reading files.
Returns: a header with counts, the definition with its source (line numbered, capped), then a table of related symbols: name, roles such as caller or callee, hop, path, lines, kind and signature.
1. Call build_context_pack with symbol=<arg>. Use path or language if the name is ambiguous, and source_lines to see more of the body.
2. Report the definition, then the callers, callees and tests it lists.
3. Edit from this pack. If it shows the symbol is widely used, run mct-impact before changing its signature.
No MCP client? Run `mct-cli --root . call --tool build_context_pack --args '{"symbol":"<arg>"}'`.
Read the file only to edit it.
