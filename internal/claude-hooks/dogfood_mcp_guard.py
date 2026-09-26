#!/usr/bin/env python3
"""PreToolUse hook: enforces CLAUDE.md's dogfooding rule technically instead
of relying on the model remembering it.

If Claude tries to explore this repo's own `crates/` source via `Grep` or a
`Bash` text-search command (grep/rg/cat/find/ls -R) instead of the
`mini-consumes-tokens` MCP tools, this turns the call into an explicit
per-instance user confirmation ("ask") rather than a silent pass-through —
mirroring CLAUDE.md's "ask the user... never fall back automatically, and
never treat one approval as a standing exception."

This script is tracked in git so it ships with the repo, but Claude Code
only runs it if wired into a local (gitignored) `.claude/settings.json` /
`.claude/settings.local.json` — see internal/claude-hooks/README.md.

Reads the standard Claude Code PreToolUse hook JSON on stdin, prints a
hookSpecificOutput decision on stdout.
"""
import json
import re
import sys

TEXT_SEARCH_BIN_RE = re.compile(r"\b(grep|rg|cat|find|ag|ack)\b")
CRATES_RE = re.compile(r"(^|[\s/'\"])crates/")

REASON = (
    "CLAUDE.md's dogfooding rule: exploring this repo's own crates/ source "
    "must go through the mini-consumes-tokens MCP tools (find_symbol, "
    "list_symbols, find_references, get_file_skeleton, ...) or mct-cli, not "
    "Grep/Bash text search. Confirm only if the MCP tools are genuinely "
    "unavailable or this falls outside their coverage (e.g. checking a "
    "doc-comment's wording)."
)


def ask(reason: str) -> None:
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "ask",
            "permissionDecisionReason": reason,
        }
    }))
    sys.exit(0)


def allow() -> None:
    sys.exit(0)


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        allow()
        return

    tool_name = payload.get("tool_name", "")
    tool_input = payload.get("tool_input", {}) or {}

    if tool_name == "Grep":
        path = tool_input.get("path") or ""
        # No explicit path means the default search root (the repo, which
        # includes crates/); an explicit path is only exempt if it clearly
        # steers away from crates/.
        if path == "" or "crates/" in path or path.rstrip("/").endswith("crates"):
            ask(REASON)
            return
        allow()
        return

    if tool_name == "Bash":
        command = tool_input.get("command", "") or ""
        if TEXT_SEARCH_BIN_RE.search(command) and CRATES_RE.search(command):
            ask(REASON)
            return
        allow()
        return

    allow()


if __name__ == "__main__":
    main()
