---
name: mct-reindex
description: "Refresh the index now. Use only when mct-status shows it stale or after large edits the background refresh has not caught up with."
---
1. Call reindex. Pass force=true only if unchanged files must be parsed again, for example after upgrading mct-cli.
2. Report the files parsed, removed and unchanged, and every issue it lists.
3. The index already refreshes itself as files settle. If results still look wrong afterwards, run mct-status and report instead of reindexing in a loop.
