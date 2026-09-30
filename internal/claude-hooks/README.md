# Agent hooks (Claude Code and Codex)

Same scripts for both agents. Wired in `.claude/settings.json` (gitignored, local) and `.codex/hooks.json`; each script reads the hook JSON on stdin and needs `python3`.

- `dogfood_mcp_guard.py` — PreToolUse: denies `Grep` or Bash `grep|rg|cat|find|ag|ack` over `crates/` and names the MCP tool to use; asks for confirmation in Claude Code if `.mcp.json` doesn't register `mini-consumes-tokens`, and denies in Codex because Codex PreToolUse doesn't support `ask`; `Read` is allowed.
- `sqlite_guard.py` — PreToolUse (Bash): denies `sqlite3` or anything touching `.mct-index/index.sqlite3`.
- `rustfmt_posttool.py` — PostToolUse (edits / `apply_patch`): silently runs `rustfmt --edition 2021` on `.rs` files from `git diff --name-only`.
- `stop_clippy.py` — Stop: if `.rs` files are modified and `stop_hook_active` is false, runs `scripts/unix/check.sh --only clippy`; on failure prints only the failing lines to stderr and exits 2.

`_common.py` holds the shared helpers.
