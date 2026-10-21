# Agent hooks (Claude Code and Codex)

Hook scripts are shared when a client opts into them. Codex registers only the SQLite CLI guard in `.codex/hooks.json`. Claude Code may have separate local registrations in the gitignored `.claude/settings.json`; this file does not change those registrations. Each script reads hook JSON on stdin and needs `python3`.

- `dogfood_mcp_guard.py` — retired, not registered in active settings. Its entry point is a no-op so clients with a cached registration cannot block tools.
- `sqlite_guard.py` — PreToolUse (Bash): denies direct invocation of the `sqlite3` CLI. Text searches mentioning the database path remain allowed.
- `rustfmt_posttool.py` — PostToolUse (edits / `apply_patch`): silently runs `rustfmt --edition 2021` on `.rs` files from `git diff --name-only`.
- `stop_clippy.py` — retired no-op. Stale client registrations cannot run Clippy; invoke project checks explicitly when wanted.
- `agent_skills_guard.py` — optional PreToolUse (Agent/Task) hook. It can deny launches and prepend instructions; Codex no longer registers it. Claude Code users with a local registration should remove that entry if it is disruptive.

`_common.py` holds the shared helpers.
