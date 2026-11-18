"""The final gate: one stable check that passes only if every required job passed.

Input is the `needs` context of the gate job (`${{ toJSON(needs) }}`, JSON
because GitHub produces it). A required job passes only with `success`.
`skipped`, `cancelled`, `failure` or a job missing from `needs` all block:
a skipped required job must never count as a pass, which is what GitHub's
branch protection does on its own when a required job is skipped.
"""

from __future__ import annotations

import json
from dataclasses import dataclass


@dataclass(frozen=True)
class Verdict:
    ok: bool
    lines: list[str]


def gate(needs: dict, required: list[str], allowed_skips: set[str] | None = None) -> Verdict:
    allowed_skips = allowed_skips or set()
    lines: list[str] = []
    ok = True
    for job in required:
        result = needs.get(job, {}).get("result") if isinstance(needs.get(job), dict) else None
        if result is None:
            ok = False
            lines.append(f"FAIL {job}: missing from the run (not in needs)")
        elif result == "success":
            lines.append(f"PASS {job}")
        elif result == "skipped" and job in allowed_skips:
            lines.append(f"SKIPPED {job}: skip allowed by the plan")
        else:
            ok = False
            lines.append(f"FAIL {job}: {result}")
    for job in sorted(set(needs) - set(required)):
        result = needs[job].get("result") if isinstance(needs[job], dict) else None
        lines.append(f"INFO {job}: {result} (not required)")
    return Verdict(ok, lines)


def parse_needs(text: str) -> dict:
    try:
        data = json.loads(text or "{}")
    except json.JSONDecodeError as err:
        raise ValueError(f"needs is not valid JSON: {err}") from err
    if not isinstance(data, dict):
        raise ValueError("needs must be a JSON object")
    return data
