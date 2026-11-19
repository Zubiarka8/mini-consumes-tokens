---
name: mct-skeleton
description: "Shape of one file: top-level declarations with bodies collapsed. Use instead of reading a whole file when you only need its structure."
---
Use when: you need the shape of one file, meaning what it declares and with which signatures, and not its bodies. Far cheaper than reading it.
Returns: a count of top-level symbols, then the file's declarations with signatures and every body collapsed to a comment.
1. Call get_file_skeleton with path=<arg>. If you do not know the file, call list_symbols first.
2. Report the declarations in order, with their signatures.
3. If you need one collapsed body, call build_context_pack on that symbol instead of reading the file.
No MCP client? Run `mct-cli --root . call --tool get_file_skeleton --args '{"path":"<arg>"}'`.
Read the file itself only to edit it.
