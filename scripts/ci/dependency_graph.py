"""The workspace crate graph from `cargo metadata`, and the transitive impact of a change.

Only edges between workspace members matter for test selection: a registry
dependency changes only through Cargo.lock, which the policy already sends to
the full suite. Every dependency kind counts (normal, dev, build) and so does
every platform-specific dependency, because any of them can break a
dependent's build or tests.

This module also holds what the plan calls `affected_crates`: owner lookup
(path -> crate) and the reverse transitive closure. They share one data
structure, so splitting them would only add an import.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import deque
from dataclasses import dataclass, field
from pathlib import PurePosixPath


class MetadataError(Exception):
    """`cargo metadata` failed or returned something unexpected."""


@dataclass
class CrateGraph:
    # crate name -> directory relative to the workspace root, POSIX separators
    dirs: dict[str, str]
    # crate name -> workspace crates it depends on (any kind, any target)
    deps: dict[str, set[str]]
    # crate name -> workspace crates that depend on it
    rdeps: dict[str, set[str]] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self.rdeps = {name: set() for name in self.dirs}
        for name, targets in self.deps.items():
            for dep in targets:
                if dep not in self.dirs:
                    raise MetadataError(f"{name} depends on unknown crate {dep}")
                self.rdeps[dep].add(name)

    def owner_of(self, path: str) -> str | None:
        """The crate whose directory holds `path` (longest prefix), or None."""
        best: tuple[int, str] | None = None
        for name, directory in self.dirs.items():
            if directory and (path == directory or path.startswith(directory + "/")):
                if best is None or len(directory) > best[0]:
                    best = (len(directory), name)
        return best[1] if best else None

    def dependents(self, crates: set[str]) -> set[str]:
        """`crates` plus every crate that depends on one of them, transitively."""
        seen = set(crates)
        queue = deque(crates)
        while queue:
            for parent in self.rdeps.get(queue.popleft(), ()):
                if parent not in seen:
                    seen.add(parent)
                    queue.append(parent)
        return seen

    def dependencies(self, crates: set[str]) -> set[str]:
        """`crates` plus everything they depend on inside the workspace, transitively."""
        seen = set(crates)
        queue = deque(crates)
        while queue:
            for dep in self.deps.get(queue.popleft(), ()):
                if dep not in seen:
                    seen.add(dep)
                    queue.append(dep)
        return seen


def from_metadata(metadata: dict) -> CrateGraph:
    root = PurePosixPath(_posix(metadata["workspace_root"]))
    members = set(metadata.get("workspace_members", []))
    dirs: dict[str, str] = {}
    manifests: dict[str, str] = {}
    for package in metadata["packages"]:
        if members and package["id"] not in members:
            continue
        manifest = PurePosixPath(_posix(package["manifest_path"]))
        try:
            rel = manifest.parent.relative_to(root)
        except ValueError as err:
            raise MetadataError(f"{package['name']} is outside the workspace") from err
        dirs[package["name"]] = "" if str(rel) == "." else str(rel)
        manifests[package["name"]] = package
    deps: dict[str, set[str]] = {}
    for name, package in manifests.items():
        deps[name] = {
            d["name"]  # the package name, also when the dependency is renamed
            for d in package.get("dependencies", [])
            if d["name"] in dirs and d.get("path")
        }
    return CrateGraph(dirs, deps)


def _posix(path: str) -> str:
    return path.replace("\\", "/")


def load(cwd: str | None = None) -> CrateGraph:
    """Run `cargo metadata --no-deps --locked`: workspace only, no registry access."""
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise MetadataError(proc.stderr.strip() or "cargo metadata failed")
    try:
        return from_metadata(json.loads(proc.stdout))
    except (KeyError, json.JSONDecodeError) as err:
        raise MetadataError(f"unexpected cargo metadata output: {err}") from err


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Print the workspace crates that depend on CRATE, transitively."
    )
    parser.add_argument("crates", nargs="*", help="crate names; none lists the graph")
    args = parser.parse_args(argv)
    try:
        graph = load()
    except MetadataError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    if not args.crates:
        for name in sorted(graph.dirs):
            print(f"{name}: {','.join(sorted(graph.deps[name]))}")
        return 0
    unknown = [c for c in args.crates if c not in graph.dirs]
    if unknown:
        print(f"error: unknown crates: {', '.join(unknown)}", file=sys.stderr)
        return 2
    for name in sorted(graph.dependents(set(args.crates))):
        print(name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
