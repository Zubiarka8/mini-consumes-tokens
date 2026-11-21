---
name: mct-setup
description: "Set up mct in a new project: index, MCP registration and ignore files. Use once per project."
---
1. Run `mct-cli --root . init` to build the first index.
2. Run `mct-cli --root . mcp-register` to write or merge .mcp.json.
3. Run `mct-cli --root . skills-register` to install the bundled workflows for detected agents.
4. Run `mct-cli --root . git-exclusions-init` so .mct-index/ stays out of git, and `mct-cli --root . init-mctignore` for an optional .mctignore.
5. Report each command's outcome. Never overwrite a file the user edited.
