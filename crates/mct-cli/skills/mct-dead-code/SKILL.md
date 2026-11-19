---
name: mct-dead-code
description: "Candidate dead code in a path, each candidate checked. Use for a cleanup review, never to delete on the first hit."
---
1. Call find_dead_code with path=<arg>.
2. For each hit, call find_references, and impact_analysis if it looks used, before calling it dead.
3. Check last the likely false positives: entry points, tests, public API, trait implementations and anything reached through macros or reflection.
4. Report confirmed candidates and the hits you ruled out with the reason. Zero indexed references is a heuristic, not proof.
Do not delete anything unasked.
