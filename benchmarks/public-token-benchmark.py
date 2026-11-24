#!/usr/bin/env python3
"""Measure MCP navigation and text-search baselines on three pinned repositories."""

import argparse
import json
import subprocess
from pathlib import Path

import tiktoken

CASES = (
    ("ripgrep", "4649aa9700619f94cf9c66876e9549d83420e16c", "rust", (
        ("Searcher", "crates/searcher/src/searcher/mod.rs"),
        ("RegexMatcher", "crates/regex/src/matcher.rs"),
        ("Standard", "crates/printer/src/standard.rs"),
    )),
    ("requests", "b25c87d7cb8d6a18a37fa12442b5f883f9e41741", "python", (
        ("Session", "src/requests/sessions.py"),
        ("PreparedRequest", "src/requests/models.py"),
        ("Response", "src/requests/models.py"),
    )),
    ("preact", "e42b82b17b3bf2ef5ff1f26328582254d7348278", "javascript_typescript", (
        ("render", "src/render.js"),
        ("createElement", "src/create-element.js"),
        ("hydrate", "src/render.js"),
    )),
)


def run(command, *, cwd=None):
    return subprocess.run(command, cwd=cwd, check=True, stdout=subprocess.PIPE, text=True).stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True, help="Path to mct-cli")
    parser.add_argument("--repos", type=Path, required=True, help="Directory containing ripgrep, requests and preact")
    args = parser.parse_args()
    cli = str(args.cli.resolve())
    encoding = tiktoken.get_encoding("cl100k_base")
    count = lambda text: len(encoding.encode(text, disallowed_special=()))
    print("| Repository | Symbol | MCP TOON | MCP JSON envelope | grep | full matched files | grep + read |")
    print("|---|---|---:|---:|---:|---:|---:|")
    for name, revision, language, cases in CASES:
        root = (args.repos / name).resolve()
        if run(["git", "rev-parse", "HEAD"], cwd=root).strip() != revision:
            raise SystemExit(f"{name}: expected revision {revision}")
        run([cli, "--root", str(root), "init", "--no-obsidian"])
        for symbol, path in cases:
            search_params = json.dumps({"query": symbol, "path": path, "language": language, "format": "toon"})
            search_command = [cli, "--root", str(root), "call", "--tool", "search_symbols_like", "--args", search_params]
            params = json.dumps({"symbol": symbol, "path": path, "language": language, "format": "toon"})
            command = [cli, "--root", str(root), "call", "--tool", "build_context_pack", "--args", params]
            search = run(search_command)
            context = run(command)
            if "Context pack for" not in context or path not in context or symbol not in search:
                raise SystemExit(f"{name}: no usable context pack or search for {symbol}: {context}")
            toon = search + context
            envelope = run(search_command + ["--json"]) + run(command + ["--json"])
            grep = run(["rg", "--line-number", "--fixed-strings", "--glob", "!.mct-index/**", symbol, "."], cwd=root)
            files = run(["rg", "--files-with-matches", "--fixed-strings", "--glob", "!.mct-index/**", symbol, "."], cwd=root)
            bodies = "".join((root / line).read_text(encoding="utf-8", errors="replace") for line in files.splitlines())
            print(f"| {name} | {symbol} | {count(toon)} | {count(envelope)} | {count(grep)} | {count(bodies)} | {count(grep + bodies)} |")


if __name__ == "__main__":
    main()
