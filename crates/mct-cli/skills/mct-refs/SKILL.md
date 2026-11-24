---
name: mct-refs
description: "Every use of a symbol: calls, imports, extends, implements and overrides. Use for a type or constant, not only a function."
---
Use when: the symbol is a type, constant or trait, or you need every kind of use and not only calls. For a plain function, mct-callers is cheaper.
Returns: a count, then one row per use with its kind: calls, imports, extends, implements or overrides, plus resolution and evidence as in mct-callers.
1. Call find_references with symbol=<arg>. Page with offset if the list is cut.
2. Report the references grouped by kind and file.
3. Prefer this for types, constants and traits, where callers alone miss imports and implementations.
No MCP client? Run `mct-cli --root . call --tool find_references --args '{"symbol":"<arg>"}'`.
Do not read whole files.
