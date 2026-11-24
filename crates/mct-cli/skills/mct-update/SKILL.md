---
name: mct-update
description: "Update mct-cli and mct-mcp-server to the latest release. Use only when the user asks to update."
---
Use when: only when the user asks to update. It replaces both programs where they are installed.
Returns: the release it installed. It runs the official installer next to the running mct-cli.
1. Run `mct-cli update`; `mct-cli -u` and `mct-cli --update` do the same.
2. Report the release it installed, and tell the user to reconnect the MCP server (/mcp in Claude Code) afterwards.
