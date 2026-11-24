---
name: mct-search
description: "Find symbols when the exact name is unknown: words, a partial identifier or a different casing. Use before guessing a name."
---
1. Call search_symbols_like with query=<arg>. An exact name always ranks first.
2. Report the hits with path:line and kind. If none fit, rephrase the words once, then try mct-meaning.
Do not read whole files.
