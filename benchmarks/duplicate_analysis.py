#!/usr/bin/env python3
"""Unix benchmark using only mct-cli; never opens the index itself.

Run with a release binary, optionally a second binary for paired measurements.
Temporary fixture projects are generated statically; no fixture code is executed.
Unix wait4 reports each child's RSS independently, unlike cumulative RUSAGE_CHILDREN.
"""

import argparse
import os
import shlex
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


def measured(command):
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        start = time.perf_counter()
        process = subprocess.Popen(command, stdout=output, stderr=errors)
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter() - start
        if process.returncode:
            errors.seek(0)
            raise RuntimeError(errors.read().decode(errors="replace"))
        size = output.tell()
        output.seek(0)
        text = output.read().decode()
        rss = usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024
        return elapsed, rss, size, text


def fixture(root, scenario, count):
    files = count if scenario == "many-small" else 4
    buckets = [[] for _ in range(files)]
    for index in range(count):
        # Equal-length distinct operator sequences, without quadratic-size functions.
        if scenario == "unique":
            shape = index
        elif scenario in ("many-small", "few-large"):
            shape = index % 10
        else:
            shape = 0
        bits = f"{shape:020b}"
        expression = "x"
        for bit in bits:
            operator = "*" if bit == "1" else "+"
            expression = f"({expression} {operator} x)"
        code = f"def copy_{index}(x):\n    return {expression}\n"
        buckets[index % files].append(code)
    for index, functions in enumerate(buckets):
        (root / f"sample_{index}.py").write_text("\n".join(functions))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="+", type=Path)
    parser.add_argument("--functions", type=int, default=1000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--extra-args", default="",
                        help="extra duplicates flags for every binary, e.g. --partial-blocks")
    args = parser.parse_args()
    if args.functions < 4 or args.functions > 1_000_000 or args.runs < 1:
        parser.error("functions must be 4..1000000 and runs must be positive")
    binaries = [str(binary.resolve(strict=True)) for binary in args.binaries]
    print("scope: synthetic Python fixtures; init excluded; warm median; per-process peak RSS")
    print(f"functions: {args.functions}\nruns: {args.runs}")
    if args.extra_args:
        print(f"extra_args: {args.extra_args}")
    columns = "scenario,binary,seconds,rss_bytes,output_bytes,groups"
    if args.extra_args:
        columns += ",partial_blocks,budget_exhausted"
    print(f"measurements[{len(binaries) * 4}]{{{columns}}}:")
    for scenario in ["many-small", "few-large", "unique", "large-group"]:
        with tempfile.TemporaryDirectory(prefix="mct-duplicate-benchmark-") as temporary:
            root = Path(temporary)
            fixture(root, scenario, args.functions)
            measured([binaries[0], "--root", str(root), "init"])
            baseline_members = None
            for position, binary in enumerate(binaries):
                command = [binary, "--root", str(root), "duplicates", "--min-lines", "1", "--min-tokens", "1"]
                command += shlex.split(args.extra_args)
                # Large-group output measures member rendering, not merely analysis.
                measured(command)
                results = [measured(command) for _ in range(args.runs)]
                # Generated paths/names never contain CSV delimiters. Ignore
                # excerpts and added header fields, compare membership in order.
                members = [tuple(line.strip().split(",")[:7]) for line in results[0][3].splitlines() if line.startswith("      sample_")]
                if baseline_members is None:
                    baseline_members = members
                elif members != baseline_members:
                    raise RuntimeError(f"{scenario}: group membership/order/counts changed")
                groups = next(line.split(": ")[1] for line in results[0][3].splitlines() if line.startswith("total_groups:"))
                extra = ""
                if args.extra_args:
                    header = dict(line.split(": ", 1) for line in results[0][3].splitlines()
                                  if line.startswith(("partial_block_count:", "  budget_exhausted:")))
                    extra = (f",{header.get('partial_block_count', '-')}"
                             f",{header.get('  budget_exhausted', '-')}")
                print(f"  {scenario},{position},{statistics.median(row[0] for row in results):.6f},"
                      f"{int(statistics.median(row[1] for row in results))},{results[0][2]},{groups}{extra}", flush=True)


if __name__ == "__main__":
    main()
