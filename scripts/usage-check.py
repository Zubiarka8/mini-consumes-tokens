#!/usr/bin/env python3
"""Read-only: print the Claude Code usage/billing keys cached in ~/.claude.json.

These are local cache values, not live billing data. Confirm limits and spend
with Claude Code's /usage command or the account's billing page.
"""
import json
import os
import sys

HINTS = ("overage", "extra", "billing", "usage", "credit", "subscription", "ratelimit", "rate_limit")
ACCOUNT_KEYS = {"hasExtraUsageEnabled", "billingType", "organizationRateLimitTier", "userRateLimitTier", "subscriptionCreatedAt"}
SKIP_KEYS = {"projects", "skillUsage", "pluginUsage", "accountUuid"}
PATH = os.path.expanduser("~/.claude.json")


def walk(obj, path):
    if not isinstance(obj, dict):
        return
    for key, value in obj.items():
        if key in SKIP_KEYS:  # per-project state and skill/plugin counters, not billing
            continue
        if path == "" and key == "oauthAccount":  # only billing fields, no identity
            for account_key in sorted(ACCOUNT_KEYS & set(value or {})):
                print(f"{key}.{account_key}", json.dumps(value[account_key])[:300])
            continue
        full = f"{path}.{key}" if path else key
        if any(hint in key.lower() for hint in HINTS):
            print(full, json.dumps(value)[:300])
        walk(value, full)


def main():
    try:
        with open(PATH) as f:
            data = json.load(f)
    except (OSError, ValueError) as err:
        print(f"cannot read {PATH}: {err}", file=sys.stderr)
        return 1
    walk(data, "")
    return 0


if __name__ == "__main__":
    sys.exit(main())
