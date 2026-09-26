# Claude Code hooks

Scripts here are tracked so they ship with the repo, but none of them run on
their own — Claude Code only invokes a hook that's wired into a local
`.claude/settings.json` (or `.claude/settings.local.json`). That file is
gitignored (session/machine-local, like `.mcp.json`), so each contributor
who wants a hook active enables it themselves, once, on their own machine.

## dogfood_mcp_guard.py

Turns the dogfooding rule in `../../CLAUDE.md` ("explore this repo's own
`crates/` source through the `mini-consumes-tokens` MCP tools or `mct-cli`,
not `Grep`/`Bash` text search") from a rule Claude has to remember into one
enforced by Claude Code itself: a `PreToolUse` hook that turns a `Grep` call
or a `Bash` command running `grep`/`rg`/`cat`/`find`/`ag`/`ack` against
`crates/` into a one-off user confirmation instead of letting it through
silently. Approving it never creates a standing exception — the next
matching call asks again, same as CLAUDE.md's own carve-out for cases the
MCP tools don't cover.

To enable it, add to `.claude/settings.json` (or `.claude/settings.local.json`)
at the project root:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Grep",
        "hooks": [
          { "type": "command", "command": "python3 internal/claude-hooks/dogfood_mcp_guard.py" }
        ]
      },
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "python3 internal/claude-hooks/dogfood_mcp_guard.py" }
        ]
      }
    ]
  }
}
```

Requires `python3` on `PATH`. Only Claude Code reads this hook config today
— it's not portable to other coding agents (Codex, Cursor, Gemini, ...),
each of which would need its own equivalent mechanism, if it has one.
