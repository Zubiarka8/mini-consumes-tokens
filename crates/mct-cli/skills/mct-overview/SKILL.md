---
name: mct-overview
description: "Orientation in an unfamiliar crate, directory or file. Use first, before reading code, when you do not yet know what a path holds."
---
1. Call get_project_overview with path=<arg> (omit path for the whole project).
2. If the layout is still unclear, call get_file_tree with path=<arg>.
3. Report the modules, the key symbols and the top callers, then name the one file to look at next.
Do not read whole files.
