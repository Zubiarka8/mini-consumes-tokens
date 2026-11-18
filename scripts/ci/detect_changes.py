"""Which files a commit range changes, from `git diff`.

The base is the merge base of the two commits, so a pull request is compared
with the point it branched from, not with whatever `main` holds now. When the
range cannot be resolved (shallow clone, a force-pushed `before`, the all-zero
SHA of a new branch) the caller gets `RangeError` and must run the full suite.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import dataclass

ZERO_SHA = "0" * 40


class RangeError(Exception):
    """The base or head commit is missing, so the impact is unknown."""


@dataclass(frozen=True)
class Change:
    status: str  # A, M, D, R, C, T (git's letters, score stripped)
    path: str
    old_path: str | None = None

    def paths(self) -> tuple[str, ...]:
        """Every path the change touches: a rename touches both names."""
        return (self.path,) if self.old_path is None else (self.old_path, self.path)


def _git(args: list[str], cwd: str | None) -> str:
    proc = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        raise RangeError(f"git {' '.join(args)}: {proc.stderr.strip()}")
    return proc.stdout


def resolve_base(base: str | None, head: str, cwd: str | None = None) -> str:
    """The commit to diff `head` against: merge-base(base, head)."""
    if not base or base == ZERO_SHA:
        raise RangeError("no base commit (new branch or missing event data)")
    for sha in (base, head):
        _git(["cat-file", "-e", f"{sha}^{{commit}}"], cwd)
    return _git(["merge-base", base, head], cwd).strip()


def parse_name_status(raw: str) -> list[Change]:
    """Parse `git diff --name-status -z` output."""
    fields = raw.split("\0")
    if fields and fields[-1] == "":
        fields.pop()
    changes: list[Change] = []
    i = 0
    while i < len(fields):
        status = fields[i][:1]
        if status in ("R", "C"):
            if i + 2 >= len(fields):
                raise RangeError("truncated rename record in git diff output")
            changes.append(Change(status, fields[i + 2], fields[i + 1]))
            i += 3
        else:
            if i + 1 >= len(fields):
                raise RangeError("truncated record in git diff output")
            changes.append(Change(status, fields[i + 1]))
            i += 2
    return changes


def changed_files(base: str, head: str, cwd: str | None = None) -> list[Change]:
    merge_base = resolve_base(base, head, cwd)
    raw = _git(
        ["diff", "--name-status", "-z", "-M", "--no-ext-diff", merge_base, head], cwd
    )
    return parse_name_status(raw)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="List files changed between the merge base of BASE and HEAD."
    )
    parser.add_argument("--base", required=True, help="base commit or ref")
    parser.add_argument("--head", default="HEAD", help="head commit or ref")
    args = parser.parse_args(argv)
    try:
        changes = changed_files(args.base, args.head)
    except RangeError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    for change in changes:
        print(change.status, *change.paths(), sep="\t")
    return 0


if __name__ == "__main__":
    sys.exit(main())
