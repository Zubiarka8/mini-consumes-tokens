---
name: mct-overview
description: "Orientation in an unfamiliar crate, directory or file. Use first, before reading code, when you do not yet know what a path holds."
---
Use when: you land in an unfamiliar crate, directory or file and need the map before reading anything. One capped call replaces many reads. Use mct-tree for folders only, mct-symbols for the complete list.
Returns: a header, then per file its top symbols as [kind] name and a line range such as L1-L3, ranked and capped per module.
1. Call get_project_overview with path=<arg> (omit path for the whole project). Start wide, then narrow to one crate or directory only after the first answer.
2. If the layout is still unclear, call get_file_tree with path=<arg>.
3. Report the modules, the key symbols and the top callers, then name the one file to look at next and why.
4. Next step: get_file_skeleton on that file, or mct-symbols for a whole directory.
No MCP client? Run `mct-cli --root . call --tool get_project_overview --args '{"path":"<arg>"}'` (omit --args for the whole project).
Do not read whole files.
