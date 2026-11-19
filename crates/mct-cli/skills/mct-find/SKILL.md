---
name: mct-find
description: "Go to the definition of a symbol by its exact name. Use when you know the name and want where it is defined."
---
Use when: you know the exact name and want where it is defined. Matches exact, prefix or fuzzy, in any language. For the users of a symbol use mct-callers or mct-refs.
Returns: a count, then one row per definition: path, line, column, language, kind, name, parent and signature.
1. Call find_symbol with name=<arg>.
2. Report each definition as path:line with its kind and signature. If several match, list all of them and use path or language to narrow.
3. If nothing matches, the name is probably inexact: use mct-search for words or mct-meaning for behavior. Do not guess a name.
No MCP client? Run `mct-cli --root . call --tool find_symbol --args '{"name":"<arg>"}'`.
Do not read whole files.
