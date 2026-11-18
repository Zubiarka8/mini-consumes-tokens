"""Load ci_policy.toml and match its path globs. Shared by every scripts/ci module.

Leaf module: imports nothing from scripts/ci, so no import cycle can start here.
"""

from __future__ import annotations

import functools
import json
import re
import sys
import tomllib
from pathlib import Path

DEFAULT_POLICY = Path(__file__).with_name("ci_policy.toml")
LEVELS = ("R0", "R1", "R2", "R3", "R4")

# Exit codes shared by every CLI in scripts/ci.
EXIT_OK = 0
EXIT_FAILED = 1  # the check ran and found a problem (gate blocked, test failed)
EXIT_USAGE = 2  # bad arguments, unreadable policy, missing git history


class PolicyError(Exception):
    """The policy file is missing, malformed or inconsistent."""


def load(path: Path | str | None = None) -> dict:
    path = Path(path) if path else DEFAULT_POLICY
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as err:
        raise PolicyError(f"cannot read policy {path}: {err}") from err
    validate(data)
    return data


def validate(data: dict) -> None:
    if data.get("unknown_level") not in LEVELS:
        raise PolicyError("unknown_level must be one of R0..R4")
    for rule in data.get("rules", []):
        if rule.get("level") not in LEVELS:
            raise PolicyError(f"rule {rule.get('pattern')!r}: bad level")
        if rule.get("scope") not in ("docs", "crate", "full"):
            raise PolicyError(f"rule {rule.get('pattern')!r}: bad scope")
    ids = [p["id"] for p in data.get("platforms", [])]
    if len(ids) != len(set(ids)):
        raise PolicyError("duplicate platform id")
    targets = [p["target"] for p in data.get("platforms", [])]
    if len(targets) != len(set(targets)):
        raise PolicyError("duplicate platform target")


def level_index(level: str) -> int:
    return LEVELS.index(level)


@functools.lru_cache(maxsize=None)
def _compile(pattern: str) -> re.Pattern[str]:
    out = []
    i = 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("".join(out) + r"\Z")


def glob_match(pattern: str, path: str) -> bool:
    """`*` never crosses `/`; `**` does; `**/` also matches zero directories."""
    return _compile(pattern).match(path) is not None


def log(event: str, level: str = "info", **fields: object) -> None:
    """One structured log line on stderr: `level=info event=x key=value`."""
    parts = [f"level={level}", f"event={event}"]
    for key, value in fields.items():
        if isinstance(value, (list, tuple, set)):
            value = ",".join(str(v) for v in sorted(value))
        text = str(value)
        if not text or any(c in text for c in ' "='):
            text = json.dumps(text)
        parts.append(f"{key}={text}")
    print(" ".join(parts), file=sys.stderr)
