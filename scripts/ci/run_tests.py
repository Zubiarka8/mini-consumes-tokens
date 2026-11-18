"""The cargo commands a plan selects, and running them in order.

The flags match the required CI jobs exactly (`--locked`, `--all-targets`,
the clippy deny list), so a selected run checks the same things as the full
one, only on fewer crates. `cargo test` stays (not nextest) because it also
runs doctests.
"""

from __future__ import annotations

import subprocess

from classify_risk import Plan

CLIPPY_DENY = [
    "-D", "warnings",
    "-D", "clippy::unwrap_used",
    "-D", "clippy::expect_used",
    "-D", "clippy::panic",
]


def commands(plan: Plan) -> list[list[str]]:
    if plan.full:
        scope = ["--workspace"]
    elif plan.test_crates:
        scope = [arg for crate in sorted(plan.test_crates) for arg in ("-p", crate)]
    else:
        return []
    return [
        ["cargo", "build", "--locked", *scope, "--all-targets"],
        ["cargo", "test", "--locked", *scope],
        ["cargo", "clippy", "--locked", *scope, "--all-targets", "--all-features", "--", *CLIPPY_DENY],
    ]


def run(cmds: list[list[str]]) -> int:
    """Run each command; stop at the first failure and return its exit code."""
    for cmd in cmds:
        code = subprocess.run(cmd, check=False).returncode
        if code != 0:
            return code
    return 0
