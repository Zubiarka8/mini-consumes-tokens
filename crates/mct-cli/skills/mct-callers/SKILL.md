---
name: mct-callers
description: "Who calls a function directly. Use to see the direct users of a function before changing its signature."
---
Use when: you are about to change a function's signature or behavior and need its direct users. Narrower than mct-refs and cheaper than mct-impact.
Returns: a count, then one row per call site: path, line, column, language, the calling symbol, the kind, depth, and a resolution column (resolved or unresolved) with its evidence and candidate count.
1. Call find_callers with function=<arg>. Use depth=2 only if the direct callers are not enough.
2. Report callers as path:line, grouped by file. Say so when the name is ambiguous instead of picking one.
3. Keep callers whose resolution is unresolved apart from the certain ones. For the full blast radius with tests, use mct-impact.
No MCP client? Run `mct-cli --root . call --tool find_callers --args '{"function":"<arg>"}'`.
Do not read whole files.
