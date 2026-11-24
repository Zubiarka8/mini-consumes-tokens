"""Run one MCT workflow through the installed mct-cli executable."""

import argparse
import json
import math
import shutil
import subprocess
import sys
from pathlib import Path

TOOLS = {
    "callers": ("find_callers", "function", True),
    "calls": ("find_calls", "function", True),
    "context": ("build_context_pack", "symbol", True),
    "dead-code": ("find_dead_code", "path", False),
    "deps": ("get_dependency_graph", "module", True),
    "duplicates": ("find_duplicate_code", "path", False),
    "find": ("find_symbol", "name", True),
    "impact": ("impact_analysis", "symbol", True),
    "meaning": ("hybrid_search_like", "query", True),
    "overview": ("get_project_overview", "path", False),
    "refs": ("find_references", "symbol", True),
    "search": ("search_symbols_like", "query", True),
    "skeleton": ("get_file_skeleton", "path", True),
    "symbols": ("list_symbols", "path", True),
    "tree": ("get_file_tree", "path", False),
}
COMMANDS = {"status", "reindex", "obsidian", "probe", "update", "web"}
SETUP = {"init", "mcp-register", "skills-register", "git-exclusions-init", "init-mctignore", "gitignore-init", "ignore-init", "status"}


def json_object(text):
    def unique_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def reject_constant(value):
        raise ValueError(f"invalid JSON number: {value}")

    try:
        options = json.loads(text, object_pairs_hook=unique_pairs, parse_constant=reject_constant)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid JSON at line {error.lineno}, column {error.colno}: {error.msg}") from error
    except RecursionError as error:
        raise ValueError("JSON nesting is too deep") from error
    if not isinstance(options, dict):
        raise ValueError("--args must be a JSON object")
    for key in ("depth", "limit", "offset", "max_tokens", "source_lines", "snippet_lines", "top_k"):
        if key in options and options[key] is not None:
            if isinstance(options[key], bool) or not isinstance(options[key], int) or options[key] < 0:
                raise ValueError(f"{key} must be a non-negative integer or null")
    for key in ("path", "language", "format", "detail"):
        if key in options and options[key] is not None and not isinstance(options[key], str):
            raise ValueError(f"{key} must be a string or null")
    if "cache" in options and options["cache"] is not None and not isinstance(options["cache"], bool):
        raise ValueError("cache must be a boolean or null")
    return options


def main(workflow):
    parser = argparse.ArgumentParser(description=f"Run the MCT {workflow} workflow via mct-cli.")
    parser.add_argument("--root", default=".", help="project root (default: current directory)")
    parser.add_argument("--cli", default="mct-cli", help="path to mct-cli")
    parser.add_argument("--timeout", type=float, help="maximum runtime in seconds (default: no limit)")
    if workflow not in TOOLS and workflow not in COMMANDS and workflow != "setup":
        parser.error(f"unknown workflow: {workflow}")
    if workflow in TOOLS:
        tool, key, required = TOOLS[workflow]
        parser.add_argument("value", nargs=None if required else "?", help=f"value for {key}")
        parser.add_argument("--args", default="{}", help="additional tool arguments as a JSON object")
        args = parser.parse_args()
        try:
            options = json_object(args.args)
            if args.value is not None:
                if not args.value.strip():
                    raise ValueError(f"{key} cannot be empty")
                if key in options:
                    raise ValueError(f"{key} is already supplied as a positional argument")
                options[key] = args.value
            encoded_options = json.dumps(options, allow_nan=False)
        except (ValueError, TypeError, RecursionError) as error:
            parser.error(str(error))
        command = [args.cli, "--root", args.root, "call", "--tool", tool, "--args", encoded_options]
    else:
        if workflow == "setup":
            parser.add_argument("operation", choices=sorted(SETUP), help="one setup operation to run")
        parser.add_argument("options", nargs=argparse.REMAINDER, help="arguments forwarded to mct-cli after --")
        args = parser.parse_args()
        operation = args.operation if workflow == "setup" else workflow
        options = args.options[1:] if args.options[:1] == ["--"] else args.options
        command = [args.cli, "--root", args.root, operation, *options]
    if args.timeout is not None:
        if not math.isfinite(args.timeout) or args.timeout <= 0:
            parser.error("--timeout must be a positive finite number")
    try:
        root = Path(args.root).expanduser()
        if not root.is_dir():
            parser.error(f"project root is not an accessible directory: {args.root}")
    except (OSError, RuntimeError, ValueError) as error:
        parser.error(f"invalid project root: {error}")
    command[2] = str(root)
    if not args.cli.strip() or "\0" in args.cli:
        parser.error("--cli must name an executable without NUL characters")
    try:
        executable = shutil.which(args.cli)
    except OSError as error:
        parser.error(f"cannot inspect mct-cli executable: {error}")
    if not executable:
        parser.error(f"mct-cli executable not found or not executable: {args.cli}")
    try:
        result = subprocess.run(command, check=False, timeout=args.timeout)
        if result.returncode < 0:
            return 128 - result.returncode
        return result.returncode
    except subprocess.TimeoutExpired:
        print(f"MCT command exceeded {args.timeout:g} seconds", file=sys.stderr)
        return 124
    except KeyboardInterrupt:
        print("MCT command interrupted", file=sys.stderr)
        return 130
    except OSError as error:
        print(f"Cannot run {args.cli}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    name = Path(sys.argv[0]).stem.removeprefix("mct_").replace("_", "-")
    if name not in TOOLS and name not in COMMANDS and name != "setup":
        sys.exit(f"Unknown workflow: {name}")
    sys.exit(main(name))
