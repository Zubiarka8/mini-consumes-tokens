---
name: mct-callers
description: "Who calls a function directly. Use to see the direct users of a function before changing its signature."
---
1. Call find_callers with function=<arg>. Use depth=2 only if the direct callers are not enough.
2. Report callers as path:line, grouped by file. Say so when the name is ambiguous instead of picking one.
Do not read whole files.
