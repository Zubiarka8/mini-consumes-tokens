import copy
import re
import unittest
from pathlib import Path

from helpers import POLICY, changes, graph

from classify_risk import classify, full_plan
from generate_matrix import MatrixError, build

RELEASE_WORKFLOW_TARGETS = {
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
}


def ids(matrix):
    return [e["id"] for e in matrix.include]


class Matrix(unittest.TestCase):
    def test_docs_only_runs_linux_for_the_docs_consumers(self):
        plan = classify(changes("docs/a.md"), graph(), POLICY)
        self.assertEqual(ids(build(plan, POLICY)), ["linux-x64"])

    def test_nothing_selected_means_no_rust_job(self):
        plan = classify([], graph(), POLICY)
        matrix = build(plan, POLICY)
        self.assertEqual(matrix.include, [])
        self.assertTrue(all(why for _, why in matrix.skipped))

    def test_os_sensitive_dependents_run_on_every_pr_platform(self):
        plan = classify(changes("crates/mct-lang-a/src/lib.rs"), graph(), POLICY)
        self.assertEqual(ids(build(plan, POLICY)), ["linux-x64", "macos-arm64", "windows-x64"])

    def test_a_pure_crate_runs_linux_only_and_says_why_not_others(self):
        plan = classify(changes("crates/mct-lang-b/tests/t.rs"), graph(), POLICY)
        matrix = build(plan, POLICY)
        self.assertEqual(ids(matrix), ["linux-x64"])
        self.assertIn("macos-arm64", {pid for pid, _ in matrix.skipped})

    def test_full_scope_all_covers_the_release_targets(self):
        matrix = build(full_plan("t"), POLICY, scope="all")
        self.assertEqual({e["target"] for e in matrix.include}, RELEASE_WORKFLOW_TARGETS)

    def test_extra_scope_is_every_platform_a_pull_request_does_not_test(self):
        extra = {e["id"] for e in build(full_plan("t"), POLICY, scope="extra").include}
        pr = {e["id"] for e in build(full_plan("t"), POLICY, scope="pr").include}
        self.assertFalse(extra & pr)
        self.assertEqual(extra | pr, {p["id"] for p in POLICY["platforms"]})

    def test_unknown_scope_is_rejected(self):
        with self.assertRaises(MatrixError):
            build(full_plan("t"), POLICY, scope="everything")

    def test_release_workflow_targets_match_the_policy(self):
        text = (Path(__file__).resolve().parents[3] / ".github/workflows/release.yml").read_text()
        found = set(re.findall(r"target:\s*([a-z0-9_]+-[a-z0-9_-]+)", text))
        self.assertEqual(found, RELEASE_WORKFLOW_TARGETS)

    def test_no_duplicates_and_every_platform_accounted_for(self):
        for plan in (full_plan("t"), classify(changes("docs/a.md"), graph(), POLICY)):
            for scope in ("pr", "extra", "all"):
                matrix = build(plan, POLICY, scope)
                listed = ids(matrix) + [pid for pid, _ in matrix.skipped]
                self.assertEqual(sorted(listed), sorted(p["id"] for p in POLICY["platforms"]))
                self.assertEqual(len(set(ids(matrix))), len(ids(matrix)))

    def test_invalid_target_is_rejected(self):
        bad = copy.deepcopy(POLICY)
        bad["platforms"][0]["target"] = "x86_64-unknown-linux-gnux"
        with self.assertRaises(MatrixError):
            build(full_plan("t"), bad)

    def test_arch_must_match_target(self):
        bad = copy.deepcopy(POLICY)
        bad["platforms"][0]["arch"] = "ARM64"
        with self.assertRaises(MatrixError):
            build(full_plan("t"), bad)

    def test_a_cross_build_is_never_marked_as_tested(self):
        cross = copy.deepcopy(POLICY)
        cross["platforms"][3]["mode"] = "cross"
        entry = next(e for e in build(full_plan("t"), cross, "all").include
                     if e["id"] == cross["platforms"][3]["id"])
        self.assertFalse(entry["run_tests"])


if __name__ == "__main__":
    unittest.main()
