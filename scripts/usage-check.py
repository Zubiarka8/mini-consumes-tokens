#!/usr/bin/env python3
"""Read-only: print the Claude Code usage/billing keys cached in ~/.claude.json, as TOON.

These are local cache values, not live billing data. Confirm limits and spend
with Claude Code's /usage command or the account's billing page.

TOON subset: `key: value`, nested objects by indentation, primitive arrays inline
as `key[N]: a,b`, other arrays as `key[N]:` with `- ` items.
"""
import json
import os
import re
import sys

HINTS = ("overage", "extra", "billing", "usage", "credit", "subscription", "ratelimit", "rate_limit")
ACCOUNT_KEYS = {"hasExtraUsageEnabled", "billingType", "organizationRateLimitTier", "userRateLimitTier", "subscriptionCreatedAt"}
SKIP_KEYS = {"projects", "skillUsage", "pluginUsage", "accountUuid"}
PATH = os.path.expanduser("~/.claude.json")
NUMBER_RE = re.compile(r"^-?\d+(\.\d+)?([eE][+-]?\d+)?$")


def prune(obj):
    if isinstance(obj, dict):
        return {k: prune(v) for k, v in obj.items() if k not in SKIP_KEYS}
    if isinstance(obj, list):
        return [prune(v) for v in obj]
    return obj


def select(obj, top=True):
    """Keep keys that match HINTS (whole value), recurse into the rest, drop the rest."""
    out = {}
    for key, value in obj.items():
        if key in SKIP_KEYS:
            continue
        if top and key == "oauthAccount" and isinstance(value, dict):  # billing fields only, no identity
            out[key] = {a: value[a] for a in sorted(ACCOUNT_KEYS) if a in value}
        elif any(hint in key.lower() for hint in HINTS):
            out[key] = prune(value)
        elif isinstance(value, dict):
            sub = select(value, top=False)
            if sub:
                out[key] = sub
    return out


def needs_quote(s):
    return (
        s == ""
        or s != s.strip()
        or any(c in s for c in ',:"\\[]{}\n\r\t')
        or s in ("true", "false", "null")
        or s.startswith("- ")
        or NUMBER_RE.match(s) is not None
    )


def quote(s):
    escaped = s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
    return f'"{escaped}"'


def scalar(v):
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return json.dumps(v)
    return quote(v) if needs_quote(v) else v


def encode(obj, depth=0):
    pad = "  " * depth
    lines = []
    for key, value in obj.items():
        k = quote(key) if needs_quote(key) else key
        if isinstance(value, dict):
            lines.append(f"{pad}{k}:")
            lines.extend(encode(value, depth + 1))
        elif isinstance(value, list):
            lines.append(f"{pad}{k}[{len(value)}]:" if any(isinstance(x, (dict, list)) for x in value) or not value
                         else f"{pad}{k}[{len(value)}]: " + ",".join(scalar(x) for x in value))
            if value and any(isinstance(x, (dict, list)) for x in value):
                for item in value:
                    if isinstance(item, dict):
                        sub = encode(item, depth + 2)
                        if sub:
                            sub[0] = f"{pad}  - " + sub[0].lstrip()
                            lines.extend(sub)
                        else:
                            lines.append(f"{pad}  - {{}}")
                    else:
                        lines.append(f"{pad}  - {scalar(item)}")
        else:
            lines.append(f"{pad}{k}: {scalar(value)}")
    return lines


def main():
    try:
        with open(PATH) as f:
            data = json.load(f)
    except (OSError, ValueError) as err:
        print(f"cannot read {PATH}: {err}", file=sys.stderr)
        return 1
    print("\n".join(encode(select(data))))
    return 0


if __name__ == "__main__":
    sys.exit(main())
