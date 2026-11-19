---
name: mct-symbols
description: "What a file, directory or crate defines. Use when you do not know a symbol's exact name."
---
1. Call list_symbols with path=<arg>. A file path matches exactly; a directory or crate path matches as a prefix. Narrow with kind or language.
2. Report the symbols grouped by file and kind.
3. Once you know the exact name, move on: mct-find for its definition, mct-context before editing it.
Do not read whole files.
