---
name: mct-context
description: "Everything needed before editing a symbol: definition, callers, callees, dependencies and tests, in one call. Use before you start working on it."
---
1. Call build_context_pack with symbol=<arg>. Use path or language if the name is ambiguous, and source_lines to see more of the body.
2. Report the definition, then the callers, callees and tests it lists.
Read the file only to edit it.
