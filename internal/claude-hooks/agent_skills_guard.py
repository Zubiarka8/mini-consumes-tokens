#!/usr/bin/env python3
"""PreToolUse hook (Agent/Task): no agent or subagent starts without the project-local skills.

Two things happen on every launch:
  1. If the primary checkout has no skills (.agents/skills/*/SKILL.md), the launch is denied.
  2. The skills duty is prepended to the agent's prompt (once), so every agent is told to list
     .claude/skills, repair a missing one with the sync script, read the fitting SKILL.md files,
     report `blocked` if it cannot get them, and name the skills it used.
Worktrees created for agents get their skills from the post-checkout hook; this is the second line."""
import json
import subprocess
import sys
from pathlib import Path

from _common import emit_decision, read_payload

MARKER = "[MANDATORY LOCAL SKILLS]"


def primary_checkout(cwd):
    try:
        out = subprocess.run(
            ["git", "worktree", "list", "--porcelain"],
            cwd=cwd, capture_output=True, text=True, timeout=15,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    for line in out.splitlines():
        if line.startswith("worktree "):
            return Path(line[len("worktree "):])
    return None


def preamble(primary):
    return (
        f"{MARKER} Project rule, no exceptions. Before anything else, run `ls .claude/skills` in your working "
        f"directory. If it is missing or empty, run `{primary}/scripts/unix/sync-skills.sh \"$PWD\"` (or, if that "
        f"script is absent, copy `{primary}/.agents/skills` to `./.agents/skills` and link "
        f"`.claude/skills` -> `../.agents/skills`). Read the SKILL.md of every skill that fits your task and follow "
        "it. Use only project-local skills, never global ones. If you still cannot get the skills, stop and report "
        "status `blocked`. Your final report must list the local skills you used, or say that none fit.\n\n"
    )


def main():
    payload = read_payload()
    if payload.get("tool_name") not in ("Agent", "Task"):
        return
    tool_input = payload.get("tool_input")
    if not isinstance(tool_input, dict):
        return
    primary = primary_checkout(payload.get("cwd") or ".")
    if primary is None:
        return
    if not list((primary / ".agents" / "skills").glob("*/SKILL.md")):
        emit_decision(
            "deny",
            f"No project-local skills in {primary}/.agents/skills. Agents must not start without skills: "
            "restore them (git checkout of the skills commit, or scripts/unix/sync-skills.sh) and retry.",
        )
    prompt = tool_input.get("prompt") or ""
    if MARKER in prompt:
        return
    updated = dict(tool_input)
    updated["prompt"] = preamble(primary) + prompt
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated,
        }
    }))
    sys.exit(0)


if __name__ == "__main__":
    main()
