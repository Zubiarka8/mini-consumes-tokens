# Agent hooks (Claude Code and Codex)

Same scripts for both agents. Wired in `.claude/settings.json` (gitignored, local) and `.codex/hooks.json`; each script reads the hook JSON on stdin and needs `python3`.

- `dogfood_mcp_guard.py` — retired, not registered in active settings. Its entry point is a no-op so clients with a cached registration cannot block tools.
- `sqlite_guard.py` — PreToolUse (Bash): denies `sqlite3` or anything touching `.mct-index/index.sqlite3`.
- `rustfmt_posttool.py` — PostToolUse (edits / `apply_patch`): silently runs `rustfmt --edition 2021` on `.rs` files from `git diff --name-only`.
- `stop_clippy.py` — Stop: if `.rs` files are modified and `stop_hook_active` is false, runs `scripts/unix/check.sh --only clippy`; on failure prints only the failing lines to stderr and exits 2.

`_common.py` holds the shared helpers.
