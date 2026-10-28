#!/usr/bin/env python3
"""Run only an isolated dashboard from a native terminal; keep its URL private."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    suffix = ".exe" if os.name == "nt" else ""
    binary = (args.bin_dir / ("mct-cli" + suffix)).resolve(strict=True)
    print("Candidate SHA-256:", hashlib.sha256(binary.read_bytes()).hexdigest(), flush=True)
    with tempfile.TemporaryDirectory(prefix="mct-phase9-browser-") as directory:
        root = Path(directory).resolve()
        body = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n"
        (root / "copies.rs").write_text("pub fn alpha" + body + "pub fn beta" + body)
        subprocess.run([str(binary), "--root", str(root), "init"], check=True)
        print("Disposable project:", root, flush=True)
        print("Keep the private URL below secret. Ctrl-C stops ONLY this test instance.", flush=True)
        print("The disposable project is removed when this launcher exits.", flush=True)
        child = subprocess.Popen([str(binary), "--root", str(root), "web",
                                  "--port", "0", "--no-obsidian"])
        try:
            code = child.wait()
        except KeyboardInterrupt:
            code = 130
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        raise SystemExit(code)


if __name__ == "__main__":
    main()
