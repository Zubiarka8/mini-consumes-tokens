#!/usr/bin/env python3
"""Isolated CLI/index and real stdio MCP acceptance; never opens SQLite directly.

Build the candidate first. Output is TOON; JSON is used only for MCP transport.
No installed service is contacted. All source/index data belongs to TemporaryDirectory.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import queue
import statistics
import subprocess
import tempfile
import threading
import time


def stats(values):
    return (statistics.median(values), sorted(values)[math.ceil(len(values) * .95) - 1],
            statistics.pvariance(values))


class Client:
    def __init__(self, binary, root, extra_args=(), stderr=None):
        env = dict(os.environ, MCT_TOOL_ATTENTION="0")
        self.process = subprocess.Popen([str(binary), "--root", str(root), *extra_args],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL if stderr is None else stderr,
                                        text=True, env=env)
        self.messages = queue.Queue()
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        self.serial = 0
        self.calls = 0
        self.wire_bytes = 0
        self.timings = []

    def read(self):
        for line in self.process.stdout:
            self.messages.put(line)
        self.messages.put(None)

    def send(self, message):
        line = json.dumps(message, separators=(",", ":")) + "\n"
        self.wire_bytes += len(line.encode())
        self.process.stdin.write(line)
        self.process.stdin.flush()

    def request(self, method, params):
        self.serial += 1
        self.calls += 1
        start = time.perf_counter()
        self.send(dict(jsonrpc="2.0", id=self.serial, method=method, params=params))
        deadline = start + 60
        while True:
            raw = self.messages.get(timeout=max(.001, deadline - time.perf_counter()))
            if raw is None:
                raise RuntimeError("MCP exited before a response")
            self.wire_bytes += len(raw.encode())
            result = json.loads(raw)
            if result.get("id") == self.serial:
                self.timings.append((time.perf_counter() - start) * 1000)
                if "error" in result:
                    raise RuntimeError("MCP protocol error; inspect isolated fixture")
                return result["result"]
            if time.perf_counter() > deadline:
                raise TimeoutError("MCP response deadline")

    def tool(self, name, arguments):
        result = self.request("tools/call", dict(name=name, arguments=arguments))
        if result.get("isError"):
            raise RuntimeError("MCP tool failed: " + name)
        return "\n".join(c.get("text", "") for c in result.get("content", []))

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.reader.join(timeout=1)
        self.process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--files", type=int, nargs="+", default=[16, 128])
    args = parser.parse_args()
    if not 2 <= args.samples <= 50 or any(n < 2 or n > 2048 for n in args.files):
        parser.error("samples must be 2..50 and files 2..2048")
    suffix = ".exe" if os.name == "nt" else ""
    cli = (args.bin_dir / ("mct-cli" + suffix)).resolve(strict=True)
    mcp = (args.bin_dir / ("mct-mcp-server" + suffix)).resolve(strict=True)
    print("platform:", platform.system(), platform.machine())
    for path in [cli, mcp]:
        print(path.name + "_sha256:", hashlib.sha256(path.read_bytes()).hexdigest())
    print("samples:", args.samples)
    print("timing: wall_ms; variance_ms_squared; debug_or_release_as_supplied")
    print("token_estimator: ceil(utf8_wire_bytes/4); not tokenizer_or_billing")
    print("profiles{files,operation,p50_ms,p95_ms,variance,db_bytes,wal_bytes}:")
    for count in args.files:
        with tempfile.TemporaryDirectory(prefix="mct-phase9-") as temporary:
            root = Path(temporary).resolve()
            source = root / "src"
            source.mkdir()
            body = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n"
            for n in range(count):
                (source / f"f{n}.rs").write_text(f"pub fn function_{n}" + body)

            def run(command):
                start = time.perf_counter()
                result = subprocess.run([str(cli), "--root", str(root)] + command,
                                        capture_output=True, text=True, timeout=60)
                if result.returncode:
                    raise RuntimeError("CLI failed: " + command[0])
                return (time.perf_counter() - start) * 1000, result.stdout + result.stderr

            def row(name, values):
                db = root / ".mct-index/index.sqlite3"
                wal = root / ".mct-index/index.sqlite3-wal"
                median, p95, variance = stats(values)
                print(f"  {count},{name},{median:.3f},{p95:.3f},{variance:.3f},"
                      f"{db.stat().st_size},{wal.stat().st_size if wal.exists() else 0}", flush=True)

            row("initial_index", [run(["init"])[0]])
            unchanged, changed = [], []
            for n in range(args.samples):
                elapsed, output = run(["reindex"])
                if f"0 parsed, {count} unchanged" not in output:
                    raise AssertionError("unchanged reconciliation reparsed source")
                unchanged.append(elapsed)
                (source / "f0.rs").write_text(f"// revision {n}\npub fn function_0" + body)
                elapsed, output = run(["reindex"])
                if "1 parsed" not in output:
                    raise AssertionError("incremental reconciliation did not parse exactly one file")
                changed.append(elapsed)
            row("no_change_reconciliation", unchanged)
            row("one_file_incremental", changed)
            client = Client(mcp, root)
            try:
                init = client.request("initialize", dict(protocolVersion="2024-11-05",
                    capabilities={}, clientInfo=dict(name="phase9-acceptance", version="1")))
                print("negotiated_protocol:", init["protocolVersion"])
                client.send(dict(jsonrpc="2.0", method="notifications/initialized"))
                catalog = client.request("tools/list", {})
                names = [tool["name"] for tool in catalog["tools"]]
                assert "find_duplicate_code" in names and "find_dead_code" in names
                print("catalog_count:", len(names))
                catalog_bytes = len(json.dumps(catalog, separators=(",", ":")).encode())
                print("catalog_payload_estimated_tokens:", math.ceil(catalog_bytes / 4))
                duplicate_times = []
                # One unmeasured warm-up, followed by bounded real transport calls.
                for n in range(args.samples + 1):
                    output = client.tool("find_duplicate_code", dict(limit=10, max_tokens=4000))
                    assert "detected: 1" in output and "actionable: 1" in output
                    assert "source_verified" in output
                    if n:
                        duplicate_times.append(client.timings[-1])
                row("warm_stdio_duplicate", duplicate_times)
                client.tool("find_dead_code", dict(limit=10))
                # Actual multi-file update, one published generation, same client.
                (source / "f0.rs").rename(source / "renamed.rs")
                (source / "f1.rs").unlink()
                client.tool("reindex", {})
                # A directory page also contains file-module symbols; query the
                # renamed file directly rather than assuming it fits one page.
                tree = client.tool("list_symbols", dict(path="src/renamed.rs", limit=10))
                assert "function_0" in tree
                health = client.tool("get_indexing_status", {})
                assert f"{count - 1} files indexed" in health
                print("stdio_workflow_calls:", client.calls)
                print("stdio_workflow_estimated_tokens:", math.ceil(client.wire_bytes / 4))
                print("stdio_lifecycle: PASS")
            finally:
                client.close()
    try:
        import resource
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        print("children_total_user_cpu_seconds:", round(usage.ru_utime, 6))
        print("children_total_system_cpu_seconds:", round(usage.ru_stime, 6))
        unit = "bytes" if platform.system() == "Darwin" else "KiB_on_Linux"
        print("maximum_child_rss_" + unit + ":", usage.ru_maxrss)
        print("children_input_block_operations:", usage.ru_inblock)
        print("children_output_block_operations:", usage.ru_oublock)
    except ImportError:
        print("child_resource_counters: NOT_AVAILABLE")
    print("limits: no_browser_no_agent_reasoning_no_review_writes_no_per_operation_io_profile")


if __name__ == "__main__":
    main()
