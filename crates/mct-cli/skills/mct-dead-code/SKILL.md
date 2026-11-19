---
name: mct-dead-code
description: "Candidate dead code in a path, each candidate checked. Use for a cleanup review, never to delete on the first hit."
---
Use when: you are reviewing for cleanup. The list is a heuristic starting point, so every hit needs checking before anything is removed.
Returns: a status line, counts (detected, actionable, suppressed), paging fields, then a candidates table: kind, name, path, line range, id and review state.
1. Call find_dead_code with path=<arg>.
2. For each hit, call find_references, and impact_analysis if it looks used, before calling it dead.
3. Check last the likely false positives: entry points, tests, public API, trait implementations and anything reached through macros or reflection.
4. Report confirmed candidates and the hits you ruled out with the reason. Zero indexed references is a heuristic, not proof.
No MCP client? Run `mct-cli --root . dead-code --path <arg>` (omit --path for the whole project). It writes a CSV to .mct-index/dead-code-report.csv; --output picks another file and --language keeps one language.
Do not delete anything unasked.
