"""Shared helpers for the hooks in this directory (Claude Code and Codex share
the same stdin JSON shape for the fields used here)."""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read_payload():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def emit_decision(decision, reason):
    """PreToolUse decision in the hookSpecificOutput format both agents read."""
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": reason,
        }
    }))
    sys.exit(0)


def modified_rs_files():
    """Modified `.rs` files per `git diff --name-only`; [] if git is unavailable."""
    try:
        out = subprocess.run(
            ["git", "diff", "--name-only", "--relative"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=30,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    files = [ln.strip() for ln in out.splitlines() if ln.strip().endswith(".rs")]
    return [f for f in files if (ROOT / f).is_file()]
