#!/usr/bin/env python3
"""PreToolUse hook: enforces CLAUDE.md's dogfooding rule.

`Grep`, or `Bash` running grep/rg/cat/find/ag/ack over `crates/`, is denied
with the equivalent mini-consumes-tokens MCP tool. If `.mcp.json` is missing or
does not register that server, Claude Code asks for confirmation and Codex
denies the call because its PreToolUse hooks do not support `ask`. `Read` is
never blocked. Same script for Claude Code and Codex.
"""
import json
import re
import sys

from _common import ROOT, emit_decision, read_payload

TEXT_SEARCH_BIN_RE = re.compile(r"\b(grep|rg|cat|find|ag|ack)\b")
CRATES_RE = re.compile(r"(^|[\s/'\"])crates/")

HINT = (
    "use the mini-consumes-tokens MCP: find_symbol (definition location), "
    "search_symbols (partial name), list_symbols (symbols in a file/crate), "
    "find_references (references), or build_context_pack (full context)."
)


def mcp_registered():
    try:
        servers = json.loads((ROOT / ".mcp.json").read_text()).get("mcpServers", {})
    except (OSError, ValueError, AttributeError):
        return False
    return isinstance(servers, dict) and "mini-consumes-tokens" in servers


def block(tool, payload):
    if mcp_registered():
        emit_decision("deny", "Dogfooding (rules.md): do not use %s over crates/; %s" % (tool, HINT))
    if payload.get("turn_id"):
        emit_decision(
            "deny",
            "The mini-consumes-tokens MCP is not registered in .mcp.json. Connect it before exploring crates/ with %s." % tool,
        )
    emit_decision(
        "ask",
        "The mini-consumes-tokens MCP is not registered in .mcp.json. Confirm whether to use %s over crates/." % tool,
    )


def main():
    payload = read_payload()
    tool_name = payload.get("tool_name", "")
    tool_input = payload.get("tool_input") or {}
    if not isinstance(tool_input, dict):
        return

    if tool_name == "Grep":
        path = tool_input.get("path") or ""
        if path == "" or "crates/" in path or path.rstrip("/").endswith("crates"):
            block("Grep", payload)
    elif tool_name == "Bash":
        command = tool_input.get("command") or ""
        if TEXT_SEARCH_BIN_RE.search(command) and CRATES_RE.search(command):
            block("Bash text search", payload)


if __name__ == "__main__":
    main()
