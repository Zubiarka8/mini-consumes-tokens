#!/usr/bin/env python3
"""Show help from the installed MCT binaries without starting the server."""

import argparse
import math
import re
import shlex
import shutil
import subprocess
import sys


def run_help(executable, *args, timeout=None, verbose=False):
    command = [executable, *args, "--help"]
    label = shlex.join(command)
    if not executable.strip() or "\0" in executable:
        raise RuntimeError(f"Cannot run {label}: executable must be nonempty and contain no NUL characters")
    try:
        if not shutil.which(executable):
            raise RuntimeError(f"Cannot run {label}: executable not found or not executable. Check PATH or pass --cli/--server with its path.")
    except OSError as error:
        raise RuntimeError(f"Cannot inspect {executable}: {error}") from error
    try:
        if verbose:
            print(f"Running: {label}", file=sys.stderr)
        result = subprocess.run(
            command, text=True, capture_output=True,
            check=False, timeout=timeout
        )
    except subprocess.TimeoutExpired as error:
        raise TimeoutError(f"Command timed out after {timeout:g} seconds: {label}") from error
    except UnicodeError as error:
        raise RuntimeError(f"Cannot decode output from {label}: {error}") from error
    except OSError as error:
        raise RuntimeError(f"Cannot run {label}: {error}") from error
    if result.returncode:
        details = [f"Command failed: {label}", f"Exit code: {result.returncode}"]
        if result.stderr.strip():
            details.append(f"Standard error:\n{result.stderr.strip()}")
        if result.stdout.strip():
            details.append(f"Standard output:\n{result.stdout.strip()}")
        if len(details) == 2:
            details.append("The command returned no diagnostic output.")
        raise RuntimeError("\n".join(details))
    if not result.stdout.strip():
        diagnostic = f"\nStandard error:\n{result.stderr.strip()}" if result.stderr.strip() else ""
        raise RuntimeError(f"Command returned empty help: {label}{diagnostic}")
    if verbose:
        print(f"Succeeded: {label} (exit code 0)", file=sys.stderr)
        if result.stderr.strip():
            print(f"Standard error:\n{result.stderr.strip()}", file=sys.stderr)
    return result.stdout


def subcommands(help_text):
    match = re.search(r"(?ms)^Commands:\s*\n(.*?)(?=^[^ \t\n]|\Z)", help_text)
    if not match:
        raise RuntimeError("Cannot find the Commands section in mct-cli --help")
    return [
        command
        for command in re.findall(r"(?m)^  ([a-z][a-z0-9-]*)(?:\s|,)", match.group(1))
        if command != "help"
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("section", nargs="?", default="cli", help="cli, server, all, or an mct-cli subcommand")
    parser.add_argument("--cli", default="mct-cli", help="path to the mct-cli executable")
    parser.add_argument("--server", default="mct-mcp-server", help="path to the mct-mcp-server executable")
    parser.add_argument("--timeout", type=float, help="maximum seconds per help command (default: no limit)")
    parser.add_argument("--verbose", action="store_true", help="show each command and its diagnostic output on stderr")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z][a-z0-9-]*", args.section):
        parser.error("section must be cli, server, all, or a lowercase subcommand name")
    if args.timeout is not None and (not math.isfinite(args.timeout) or args.timeout <= 0):
        parser.error("--timeout must be a positive finite number")
    try:
        if args.section == "server":
            print(run_help(args.server, timeout=args.timeout, verbose=args.verbose), end="")
        elif args.section == "all":
            cli_help = run_help(args.cli, timeout=args.timeout, verbose=args.verbose)
            commands = subcommands(cli_help)
            if not commands:
                raise RuntimeError("No CLI subcommands found")
            print("# mct-cli\n", cli_help, sep="", end="\n")
            for command in commands:
                print(f"# mct-cli {command}\n", run_help(args.cli, command, timeout=args.timeout, verbose=args.verbose), sep="", end="\n")
            print("# mct-mcp-server\n", run_help(args.server, timeout=args.timeout, verbose=args.verbose), sep="", end="")
        elif args.section == "cli":
            print(run_help(args.cli, timeout=args.timeout, verbose=args.verbose), end="")
        else:
            print(run_help(args.cli, args.section, timeout=args.timeout, verbose=args.verbose), end="")
    except TimeoutError as error:
        print(error, file=sys.stderr)
        return 124
    except KeyboardInterrupt:
        print("Help command interrupted", file=sys.stderr)
        return 130
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
