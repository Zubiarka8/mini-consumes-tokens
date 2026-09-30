#!/usr/bin/env python3
"""Stop hook: if `.rs` files are modified (and this isn't a hook re-run), runs
`scripts/unix/check.sh --only clippy`; on failure prints only the failing
lines to stderr and exits 2."""
import subprocess
import sys

from _common import ROOT, modified_rs_files, read_payload

MAX_LINES = 15


def main():
    payload = read_payload()
    if payload.get("stop_hook_active") or not modified_rs_files():
        return
    try:
        res = subprocess.run(
            ["scripts/unix/check.sh", "--only", "clippy"],
            cwd=str(ROOT), capture_output=True, text=True, timeout=900,
        )
    except (OSError, subprocess.SubprocessError):
        return
    if res.returncode == 0:
        return
    lines = [l for l in (res.stdout + res.stderr).splitlines()
             if l.startswith("  ") or "FAILED" in l]
    if len(lines) > MAX_LINES:
        lines = lines[:MAX_LINES] + ["  … more in target/script-logs/check-clippy.log"]
    sys.stderr.write("\n".join(lines or ["clippy FAILED"]) + "\n")
    sys.exit(2)


if __name__ == "__main__":
    main()
