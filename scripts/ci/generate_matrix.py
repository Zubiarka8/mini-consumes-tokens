"""GitHub Actions matrices from a plan and the policy's platform table.

A matrix only ever lists distinct, valid targets. Every platform left out is
listed with the reason, so a skipped OS or architecture is visible in the
run summary instead of silently missing.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from classify_risk import Plan

_VALID_TARGET = re.compile(
    r"^(x86_64|aarch64)-(unknown-linux-(gnu|musl)|apple-darwin|pc-windows-msvc)$"
)
_TARGET_ARCH = {"x86_64": "X64", "aarch64": "ARM64"}


class MatrixError(Exception):
    """The platform table is inconsistent."""


@dataclass(frozen=True)
class Matrix:
    include: list[dict]
    skipped: list[tuple[str, str]]  # (platform id, reason)

    def for_actions(self) -> dict:
        """The shape `strategy.matrix: ${{ fromJSON(...) }}` expects."""
        return {"include": self.include}


def check_platform(platform: dict) -> None:
    target = platform["target"]
    if not _VALID_TARGET.match(target):
        raise MatrixError(f"{platform['id']}: invalid target {target}")
    arch = _TARGET_ARCH[target.split("-", 1)[0]]
    if platform["arch"] != arch:
        raise MatrixError(f"{platform['id']}: arch {platform['arch']} but target is {arch}")
    if platform["mode"] not in ("native", "cross"):
        raise MatrixError(f"{platform['id']}: bad mode {platform['mode']}")


def build(plan: Plan, policy: dict, scope: str = "pr") -> Matrix:
    """`scope`: `pr` (pull request platforms), `extra` (the others) or `all`."""
    if scope not in ("pr", "extra", "all"):
        raise MatrixError(f"unknown scope {scope}")
    platforms = policy.get("platforms", [])
    for platform in platforms:
        check_platform(platform)
    sensitive = set(policy.get("platform_sensitive_crates", []))

    include: list[dict] = []
    skipped: list[tuple[str, str]] = []
    for platform in platforms:
        if scope == "pr" and not platform.get("pr", False):
            skipped.append((platform["id"], "not a pull request platform (full/release only)"))
            continue
        if scope == "extra" and platform.get("pr", False):
            skipped.append((platform["id"], "already tested on every pull request"))
            continue
        if not plan.full and not plan.test_crates:
            skipped.append((platform["id"], f"{plan.level}: no Rust tests selected"))
            continue
        primary = platform["id"] == "linux-x64"
        if not (plan.full or primary or plan.test_crates & sensitive):
            skipped.append(
                (platform["id"], f"{plan.level}: selected crates are not OS-sensitive")
            )
            continue
        include.append(
            {
                "id": platform["id"],
                "os": platform["runner"],
                "target": platform["target"],
                "arch": platform["arch"],
                "mode": platform["mode"],
                "run_tests": platform["mode"] == "native",
            }
        )
    ids = [entry["id"] for entry in include]
    targets = [entry["target"] for entry in include]
    if len(ids) != len(set(ids)) or len(targets) != len(set(targets)):
        raise MatrixError("duplicate platform in matrix")
    return Matrix(include, skipped)
