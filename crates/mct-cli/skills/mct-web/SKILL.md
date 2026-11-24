---
name: mct-web
description: "Open the local dashboard for findings, duplicates and dead code. Use when the user wants to review results visually."
---
Use when: a person should review findings, duplicates, dead code, dependencies or notes in a browser, or decide which duplicate groups are real. Run it only when the user asks; an agent cannot use the page.
Returns: the address of a local dashboard. It keeps running until stopped and keeps the index fresh meanwhile.
1. Run `mct-cli --root . web`. If it says no index was found, run `mct-cli --root . init` first.
2. Options: --port <PORT> uses exactly that port (0 for any free one), otherwise the project's remembered port is reused (8787 on the first run). --idle-timeout <MINUTES> stops it after that long unused (0 never; 30 unless the dashboard Settings say otherwise). --no-obsidian skips the vault.
3. Report the address it prints. The server runs until stopped, so say how to stop it (Ctrl-C).
