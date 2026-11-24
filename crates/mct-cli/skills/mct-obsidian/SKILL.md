---
name: mct-obsidian
description: "Rebuild the Obsidian vault in .mct-index/obsidian/. Use when the user wants the code atlas as notes."
---
Use when: the user wants the code atlas as linked notes in Obsidian. It rebuilds from the existing index and does not parse source files.
Returns: the vault folder .mct-index/obsidian/. init, reindex and web also write it unless given --no-obsidian.
1. If the index may be stale, call reindex first.
2. Run `mct-cli --root . obsidian`.
3. Report the vault folder it prints; open it with "Open folder as vault".
