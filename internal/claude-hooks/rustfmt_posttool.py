#!/usr/bin/env python3
"""PostToolUse hook: silently runs `rustfmt --edition 2021` on modified `.rs` files."""
import subprocess

from _common import ROOT, modified_rs_files, read_payload


def main():
    read_payload()
    files = modified_rs_files()
    if not files:
        return
    try:
        subprocess.run(
            ["rustfmt", "--edition", "2021"] + files,
            cwd=str(ROOT), capture_output=True, timeout=60,
        )
    except (OSError, subprocess.SubprocessError):
        pass


if __name__ == "__main__":
    main()
