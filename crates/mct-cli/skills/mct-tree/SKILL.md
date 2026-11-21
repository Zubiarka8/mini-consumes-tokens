---
name: mct-tree
description: "Directory and file layout of a path, without symbols. Use to choose a file or crate before looking inside it."
---
1. Call get_file_tree with path=<arg> (omit path for the project root). Raise depth only if the tree is too shallow.
2. Report the layout and point at the directories that matter for the task.
Do not read whole files.
