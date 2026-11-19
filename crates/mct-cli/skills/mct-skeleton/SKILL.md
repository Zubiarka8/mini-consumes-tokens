---
name: mct-skeleton
description: "Shape of one file: top-level declarations with bodies collapsed. Use instead of reading a whole file when you only need its structure."
---
1. Call get_file_skeleton with path=<arg>. If you do not know the file, call list_symbols first.
2. Report the declarations in order, with their signatures.
3. If you need one collapsed body, call build_context_pack on that symbol instead of reading the file.
Read the file itself only to edit it.
