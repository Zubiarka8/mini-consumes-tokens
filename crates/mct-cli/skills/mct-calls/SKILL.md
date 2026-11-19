---
name: mct-calls
description: "What a function calls. Use to understand what a function depends on without reading its body."
---
Use when: you want to know what a function depends on without reading its body. The reverse of mct-callers.
Returns: a count, then one row per call it makes, in the same columns as mct-callers: the callee, its resolution (resolved or unresolved) and the evidence.
1. Call find_calls with function=<arg>.
2. Report the callees as path:line. List unresolved or ambiguous callees separately and do not guess them.
3. To follow one callee, run mct-calls on it; to see who else uses it, mct-callers. Stop after two hops unless asked for more.
No MCP client? Run `mct-cli --root . call --tool find_calls --args '{"function":"<arg>"}'`.
Do not read whole files.
