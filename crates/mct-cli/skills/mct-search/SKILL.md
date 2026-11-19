---
name: mct-search
description: "Find symbols when the exact name is unknown: words, a partial identifier or a different casing. Use before guessing a name."
---
Use when: you only remember words or part of a name, in any case style. An exact name always ranks first. It is not exhaustive: to list everything in a place use mct-symbols.
Returns: a count, then rows best first: path, line range, language, kind, name, parent and a snippet.
1. Call search_symbols_like with query=<arg>. An exact name always ranks first.
2. Report the hits with path:line and kind. If none fit, rephrase the words once, then try mct-meaning.
3. Spellings differ by style (camel case, snake case, Pascal case), so try the other style or a shorter stem before giving up.
No MCP client? Run `mct-cli --root . call --tool search_symbols_like --args '{"query":"<arg>"}'`.
Do not read whole files.
