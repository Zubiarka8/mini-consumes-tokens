---
name: mct-setup
description: "Set up mct in a new project: index, MCP registration and ignore files. Use once per project."
---
Use when: the project has no index yet. Once per project; the other mct skills need the index it builds.
Returns: a built index, the MCP config of each detected agent, the skills installed for them, and ignore files.
1. Run `mct-cli --root . init` to build the first index. Add --force to parse every file again.
2. Run `mct-cli --root . mcp-register` to write or merge each agent's MCP config. Add --dry-run to preview, --agent claude,cursor to choose agents, --global to include Codex's user config.
3. Run `mct-cli --root . skills-register` to install these skills for the AI agents in use. Same --dry-run, --agent and --global; --force rewrites skills you edited.
4. Run `mct-cli --root . git-exclusions-init` so .mct-index/ stays out of git, and `mct-cli --root . init-mctignore` for an optional .mctignore (--import-gitignore, --generated).
5. Run `mct-cli --root . status` to confirm the index is healthy.
6. The programs are mct-cli and mct-mcp-server; there is no mct command. `mct-cli --help` lists every command and `mct-cli web --help` shows one command's flags.
7. Report each command's outcome. Never overwrite a file the user edited.
