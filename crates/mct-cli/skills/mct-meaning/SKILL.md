---
name: mct-meaning
description: "Search by what the code does, not by its name. Use when you can only describe the behavior, such as \"load settings from disk\"."
---
1. Call hybrid_search_like with query=<arg>. Put the phrase in double quotes for an exact phrase match.
2. Report the hits with path:line and a one-line reason each. If the first line says the semantic half is off, say that the ranking is lexical only.
Do not read whole files.
