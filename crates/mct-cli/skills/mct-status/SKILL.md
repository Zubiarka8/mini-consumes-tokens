---
name: mct-status
description: "Health of the index: parse failures, read failures, dependencies and staleness. Use when results look wrong or incomplete."
---
1. Call get_indexing_status.
2. Report the file and symbol counts, any syntax or read failure with its path, and whether the index looks stale.
3. Suggest mct-reindex only if it looks stale.
