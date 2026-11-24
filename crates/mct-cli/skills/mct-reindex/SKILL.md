---
name: mct-reindex
description: "Refresh the index now. Use only when mct-status shows it stale or after large edits the background refresh has not caught up with."
---
Use when: after large edits made while nothing was watching, or after upgrading mct-cli. Normal edits refresh the index by themselves.
Returns: one line with the files parsed, unchanged and removed, and the symbols written, followed by any issue found.
1. Call reindex. Pass force=true only if unchanged files must be parsed again, for example after upgrading mct-cli.
2. Report the files parsed, removed and unchanged, and every issue it lists.
3. The index already refreshes itself as files settle. If results still look wrong afterwards, run mct-status and report instead of reindexing in a loop.
No MCP client? Run `mct-cli --root . reindex`; add --force to parse every file again.
