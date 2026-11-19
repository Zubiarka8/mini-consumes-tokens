---
name: mct-duplicates
description: "Duplicate code in a path, telling true duplicates from look-alikes. Use before merging or extracting repeated logic."
---
1. Call find_duplicate_code with path=<arg>.
2. For each group, call build_context_pack on its members to compare what they do.
3. Report true duplicates apart from code that only looks alike, and which copy to keep.
4. Same shape with a different reason to change (two parsers for different formats) is a look-alike. Merge only when one fix would belong in every copy.
Do not edit unasked.
