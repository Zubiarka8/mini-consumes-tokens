---
name: mct-guide
description: Explain and use mini-consumes-tokens (MCT), including its CLI, MCP server, local index, and database. Use for MCT setup, command selection, and questions about its internals.
---

# MCT guide

Explain MCT in terms of the user's task, then give the smallest applicable commands or MCP calls. MCT indexes repository structure into a local symbol graph for code exploration; it does not execute the indexed code. The programs are `mct-cli` and `mct-mcp-server`, not `mct` or `mct server`.

## Choose the right interface

- Use MCP tools when a connected client exposes them. Select the tool for the question and consult its schema; use `batch` for known independent read-only queries.
- Use `mct-cli --root <project> call --tool <name> --args '<JSON object>'` when a client is unavailable. The tool arguments are JSON because the CLI expects that format.
- Use `mct-cli` subcommands for index administration, configuration, reports, the local dashboard, and other operational tasks. `mct-cli --root <project> status` reports index health.
- `mct-mcp-server` serves an MCP client over stdio; it is not a shell command dispatcher. Configure it in the client and reconnect when needed.

## Commands and options

Run `python3 scripts/mct_help.py <section>` from this skill directory for live help: use `cli`, `server`, any CLI subcommand such as `web`, or `all` for every section. Use `--cli` and `--server` to select binaries outside PATH. `--timeout <seconds>` limits each help command (exit 124 on timeout); `--verbose` reports commands and diagnostics on stderr. Read [the command reference](references/commands.md) as a fallback, with a section per CLI subcommand and the server. This is a snapshot, not a guarantee for every installed version: use `mct-cli --help`, `mct-cli <command> --help`, and `mct-mcp-server --help` as the authority for the binary actually in use. Explain options and compatible combinations relevant to the request; do not invent flags or enumerate meaningless combinations. Warn before commands that overwrite user edits or change global configuration, and follow the user's authorization.

## Workflow scripts

For the 22 focused scripts, usage examples, parameters and side effects, read [the workflow reference](references/workflows.md). These scripts call the installed CLI; query calls refresh the index first. Run setup and other write operations only when requested.

## Internals

For questions about indexing, persistence, freshness, and limitations, read [the index and database reference](references/internals.md). Distinguish stored facts from inferred relations, a stale or incomplete index from missing source, and a command's actual effects from assumptions. Use the relevant project's instructions and current documentation when they differ from this reference.
