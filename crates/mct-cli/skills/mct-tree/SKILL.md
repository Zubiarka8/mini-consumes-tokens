---
name: mct-tree
description: "Directory and file layout of a path, without symbols. Use to choose a file or crate before looking inside it."
---
Use when: you must choose a crate or file before looking at code and symbols do not matter yet. Cheaper than mct-overview.
Returns: an indented tree of directories and files, three levels deep unless you raise it, with no symbol data.
1. Call get_file_tree with path=<arg> (omit path for the project root). Raise depth only if the tree is too shallow; a deep tree of a large directory wastes tokens.
2. Report the layout and point at the directories that matter for the task.
3. This shows layout only. To see what the code defines, use mct-symbols or mct-overview.
No MCP client? Run `mct-cli --root . call --tool get_file_tree --args '{"path":"<arg>"}'` (omit --args for the project root).
Do not read whole files.
