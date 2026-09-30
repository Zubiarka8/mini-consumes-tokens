#!/usr/bin/env python3
"""PreToolUse hook (Bash): denies `sqlite3` and any command touching
`.mct-index/index.sqlite3`; the index is only accessed via the MCP tools or mct-cli."""
import re
import sys

from _common import emit_decision, read_payload

SQLITE_RE = re.compile(r"\bsqlite3\b|\.mct-index/index\.sqlite3?\b|index\.sqlite3")


def main():
    payload = read_payload()
    if payload.get("tool_name") != "Bash":
        return
    tool_input = payload.get("tool_input") or {}
    command = tool_input.get("command") if isinstance(tool_input, dict) else ""
    if SQLITE_RE.search(command or ""):
        emit_decision(
            "deny",
            "The .mct-index/index.sqlite3 index may only be accessed through MCP tools or mct-cli, never with sqlite3.",
        )


if __name__ == "__main__":
    main()
