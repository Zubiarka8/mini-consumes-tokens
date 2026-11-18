"""End-to-end through main.py: the scenario table of the CI design doc."""

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path

from helpers import GitRepo, metadata

import cache_manager
import main
import run_tests
from classify_risk import full_plan


def run_main(*argv, env=None):
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
        old = dict(os.environ)
        os.environ.pop("GITHUB_OUTPUT", None)
        os.environ.pop("GITHUB_STEP_SUMMARY", None)
        os.environ.update(env or {})
        try:
            code = main.main(list(argv))
        finally:
            os.environ.clear()
            os.environ.update(old)
    return code, out.getvalue()


class Scenarios(unittest.TestCase):
    def setUp(self):
        self.repo = GitRepo()
        root = self.repo.path
        for name in ("mct-core", "mct-lang-a", "mct-index", "mct-cli"):
            self.repo.write(f"crates/{name}/src/lib.rs")
        self.repo.write("docs/a.md")
        self.base = self.repo.commit("base")
        self._meta_dir = tempfile.TemporaryDirectory()
        self.meta = Path(self._meta_dir.name, "meta.json")
        self.meta.write_text(json.dumps(metadata(root)))
        self.cwd = os.getcwd()
        os.chdir(root)

    def tearDown(self):
        os.chdir(self.cwd)
        self.repo.close()
        self._meta_dir.cleanup()

    def plan(self, *paths, event="pull_request"):
        for p in paths:
            self.repo.write(p, "changed\n")
        head = self.repo.commit()
        code, out = run_main("plan", "--event", event, "--base", self.base, "--head", head,
                             "--metadata", str(self.meta))
        self.assertEqual(code, 0, out)
        return dict(line.split("=", 1) for line in out.splitlines() if "=" in line and not line.startswith("reason"))

    def test_docs_change(self):
        self.assertEqual(self.plan("docs/a.md")["level"], "R0")

    def test_parser_change(self):
        result = self.plan("crates/mct-lang-a/src/lib.rs")
        self.assertEqual(result["level"], "R2")
        self.assertIn("mct-lang-a", result["test_crates"])

    def test_core_change(self):
        self.assertEqual(self.plan("crates/mct-core/src/lib.rs")["full"], "true")

    def test_lockfile_change(self):
        result = self.plan("Cargo.lock")
        self.assertEqual((result["level"], result["full"]), ("R4", "true"))

    def test_ci_change(self):
        self.assertEqual(self.plan(".github/workflows/ci.yml")["full"], "true")

    def test_unknown_dependency(self):
        self.assertEqual(self.plan("crates/mct-new/src/lib.rs")["full"], "true")

    def test_deleted_file(self):
        self.repo.git("rm", "-q", "crates/mct-lang-a/src/lib.rs")
        head = self.repo.commit()
        code, out = run_main("plan", "--base", self.base, "--head", head, "--metadata", str(self.meta))
        self.assertEqual(code, 0)
        self.assertIn("reason[R2] crates/mct-lang-a/src/lib.rs", out)

    def test_incomplete_history_falls_back_to_full(self):
        code, out = run_main("plan", "--base", "f" * 40, "--head", "HEAD", "--metadata", str(self.meta))
        self.assertEqual(code, 0)
        self.assertIn("full=true", out)

    def test_incomplete_history_is_an_error_in_strict_mode(self):
        code, _ = run_main("plan", "--strict", "--base", "f" * 40, "--metadata", str(self.meta))
        self.assertEqual(code, 2)

    def test_unreadable_metadata_falls_back_to_full(self):
        self.repo.write("crates/mct-lang-a/src/lib.rs", "y\n")
        head = self.repo.commit()
        code, out = run_main("plan", "--base", self.base, "--head", head, "--metadata", "/nonexistent")
        self.assertEqual(code, 0)
        self.assertIn("full=true", out)

    def test_schedule_and_dispatch_are_full(self):
        for event in ("schedule", "workflow_dispatch"):
            code, out = run_main("plan", "--event", event)
            self.assertIn("full=true", out)

    def test_github_output_carries_a_valid_matrix(self):
        with tempfile.NamedTemporaryFile("r", suffix=".out", delete=False) as handle:
            path = handle.name
        try:
            run_main("plan", "--event", "schedule", env={"GITHUB_OUTPUT": path})
            lines = dict(l.split("=", 1) for l in Path(path).read_text().splitlines())
            matrix = json.loads(lines["matrix"])
            self.assertEqual(len(matrix["include"]), 3)
        finally:
            os.unlink(path)

    def test_dry_run_prints_without_running(self):
        code, out = run_main("run", "--event", "schedule", "--dry-run")
        self.assertEqual(code, 0)
        self.assertIn("would run: cargo test --locked --workspace", out)


class GateCli(unittest.TestCase):
    def test_gate_blocks_a_missing_job(self):
        code, out = run_main("gate", env={"NEEDS_JSON": json.dumps({"fmt": {"result": "success"}})})
        self.assertEqual(code, 1)
        self.assertIn("gate=fail", out)

    def test_gate_passes_when_all_required_succeed(self):
        needs = {job: {"result": "success"} for job in main.pol.load()["gate"]["required"]}
        code, _ = run_main("gate", env={"NEEDS_JSON": json.dumps(needs)})
        self.assertEqual(code, 0)


class CacheKey(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "Cargo.lock").write_text("v1")
        (self.root / "Cargo.toml").write_text("[workspace]")

    def tearDown(self):
        self.tmp.cleanup()

    def key(self, **kw):
        args = dict(target="x86_64-unknown-linux-gnu", runner_os="Linux", runner_arch="X64", env={})
        args.update(kw)
        return cache_manager.key(self.root, **args)

    def test_lockfile_change_invalidates(self):
        before = self.key()
        (self.root / "Cargo.lock").write_text("v2")
        self.assertNotEqual(before, self.key())

    def test_x64_and_arm64_never_share(self):
        self.assertNotEqual(self.key(), self.key(target="aarch64-unknown-linux-gnu", runner_arch="ARM64"))

    def test_rustflags_and_profile_invalidate(self):
        self.assertNotEqual(self.key(), self.key(env={"RUSTFLAGS": "-Cdebuginfo=0"}))
        self.assertNotEqual(self.key(), self.key(profile="release"))

    def test_wrong_runner_arch_fails(self):
        with self.assertRaises(cache_manager.ArchMismatch):
            cache_manager.verify_arch("aarch64-apple-darwin", "X64")
        cache_manager.verify_arch("aarch64-unknown-linux-gnu", "X64", mode="cross")

    def test_cli_rejects_wrong_arch(self):
        code, _ = run_main("cache-key", "--target", "aarch64-pc-windows-msvc", "--root", str(self.root),
                           env={"RUNNER_ARCH": "X64", "RUNNER_OS": "Windows"})
        self.assertEqual(code, 1)


class RunTests(unittest.TestCase):
    def test_full_plan_uses_the_ci_flags(self):
        cmds = run_tests.commands(full_plan("t"))
        self.assertEqual(cmds[1], ["cargo", "test", "--locked", "--workspace"])
        self.assertIn("clippy::unwrap_used", cmds[2])

    def test_first_failure_stops_and_propagates(self):
        self.assertNotEqual(run_tests.run([["git", "--no-such-flag"], ["git", "--version"]]), 0)


if __name__ == "__main__":
    unittest.main()
