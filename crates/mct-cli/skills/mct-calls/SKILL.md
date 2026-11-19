---
name: mct-calls
description: "What a function calls. Use to understand what a function depends on without reading its body."
---
1. Call find_calls with function=<arg>.
2. Report the callees as path:line. List unresolved or ambiguous callees separately and do not guess them.
3. To follow one callee, run mct-calls on it; to see who else uses it, mct-callers. Stop after two hops unless asked for more.
Do not read whole files.
