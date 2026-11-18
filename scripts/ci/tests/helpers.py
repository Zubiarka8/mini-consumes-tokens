"""Shared fixtures: a small crate graph shaped like the real workspace, and temp git repos."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

CI_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(CI_DIR))

import policy as pol  # noqa: E402
from dependency_graph import CrateGraph  # noqa: E402
from detect_changes import Change  # noqa: E402

POLICY = pol.load()

# core <- tree-sitter <- lang-a, lang-b <- languages <- cli, mcp, eval
# index <- cli, mcp ; corpus is a dev-dependency of lang-a
CRATES = {
    "mct-core": [],
    "mct-tree-sitter": ["mct-core"],
    "mct-corpus": ["mct-core"],
    "mct-lang-a": ["mct-core", "mct-tree-sitter", "mct-corpus"],
    "mct-lang-b": ["mct-core", "mct-tree-sitter"],
    "mct-lang-md": ["mct-core", "mct-tree-sitter"],
    "mct-languages": ["mct-lang-a", "mct-lang-b", "mct-lang-md"],
    "mct-index": ["mct-core"],
    "mct-cli": ["mct-languages", "mct-index"],
    "mct-mcp-server": ["mct-languages", "mct-index"],
    "mct-eval": ["mct-languages", "mct-index"],
    "mct-leaf": [],
    "mct-leaf-user": ["mct-leaf"],
}


def graph() -> CrateGraph:
    return CrateGraph(
        {name: f"crates/{name}" for name in CRATES},
        {name: set(deps) for name, deps in CRATES.items()},
    )


def changes(*paths: str) -> list[Change]:
    return [Change("M", p) for p in paths]


def metadata(root: str = "/w") -> dict:
    """A `cargo metadata --no-deps` document for CRATES."""
    packages = []
    for name, deps in CRATES.items():
        packages.append({
            "name": name,
            "id": f"path+file://{root}/crates/{name}#0.1.0",
            "manifest_path": f"{root}/crates/{name}/Cargo.toml",
            "dependencies": [
                {"name": d, "kind": "dev" if d == "mct-corpus" else None,
                 "path": f"{root}/crates/{d}", "target": None}
                for d in deps
            ] + [{"name": "serde", "kind": None, "path": None, "target": None}],
        })
    return {
        "workspace_root": root,
        "workspace_members": [p["id"] for p in packages],
        "packages": packages,
    }


class GitRepo:
    """A throwaway git repository for detect_changes tests."""

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.path = self._tmp.name
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "ci@example.invalid")
        self.git("config", "user.name", "ci")
        self.git("config", "commit.gpgsign", "false")

    def git(self, *args: str) -> str:
        env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"}
        return subprocess.run(["git", *args], cwd=self.path, check=True,
                              capture_output=True, text=True, env=env).stdout.strip()

    def write(self, rel: str, text: str = "x\n") -> None:
        path = Path(self.path, rel)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self, message: str = "c") -> str:
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", message)
        return self.git("rev-parse", "HEAD")

    def close(self) -> None:
        self._tmp.cleanup()
