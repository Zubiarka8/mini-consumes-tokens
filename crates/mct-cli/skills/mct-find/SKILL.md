---
name: mct-find
description: "Go to the definition of a symbol by its exact name. Use when you know the name and want where it is defined."
---
1. Call find_symbol with name=<arg>.
2. Report each definition as path:line with its kind and signature. If several match, list all of them and use path or language to narrow.
Do not read whole files.
