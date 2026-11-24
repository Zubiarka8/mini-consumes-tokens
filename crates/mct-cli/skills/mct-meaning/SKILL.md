---
name: mct-meaning
description: "Search by what the code does, not by its name. Use when you can only describe the behavior, such as \"load settings from disk\"."
---
Use when: you can describe what the code does but not what it is called, such as where a failed request is retried. A quoted phrase also finds string literals, for example error or log messages.
Returns: the same rows as mct-search, ranked by a mix of word match and meaning. Without an embedding model the first line says the ranking is lexical only.
1. Call hybrid_search_like with query=<arg>. Put the phrase in double quotes for an exact phrase match.
2. Report the hits with path:line and a one-line reason each. If the first line says the semantic half is off, say that the ranking is lexical only.
3. Describe the behavior, not a guessed name: retry a failed request beats retry handler. Check a promising hit with mct-skeleton or mct-context before relying on it.
No MCP client? Run `mct-cli --root . call --tool hybrid_search_like --args '{"query":"<arg>"}'`.
Do not read whole files.
