import random
import re
import unittest
from pathlib import Path

from helpers import POLICY

from collect_results import gate, parse_needs

CI_YML = Path(__file__).resolve().parents[3] / ".github/workflows/ci.yml"

REQUIRED = ["fmt", "build-test", "cargo-audit"]


def needs(**results):
    return {job: {"result": result, "outputs": {}} for job, result in results.items()}


class Gate(unittest.TestCase):
    def test_all_success_passes(self):
        self.assertTrue(gate(needs(**{"fmt": "success", "build-test": "success",
                                      "cargo-audit": "success"}), REQUIRED).ok)

    def test_a_failed_test_blocks(self):
        verdict = gate(needs(fmt="success", **{"build-test": "failure", "cargo-audit": "success"}), REQUIRED)
        self.assertFalse(verdict.ok)
        self.assertIn("FAIL build-test: failure", verdict.lines)

    def test_a_missing_required_job_blocks(self):
        verdict = gate(needs(fmt="success", **{"cargo-audit": "success"}), REQUIRED)
        self.assertFalse(verdict.ok)
        self.assertTrue(any("missing" in line for line in verdict.lines))

    def test_a_cancelled_required_job_blocks(self):
        self.assertFalse(gate(needs(fmt="success", **{"build-test": "cancelled",
                                                      "cargo-audit": "success"}), REQUIRED).ok)

    def test_a_skipped_required_job_blocks_unless_the_plan_allows_it(self):
        n = needs(fmt="success", **{"build-test": "skipped", "cargo-audit": "success"})
        self.assertFalse(gate(n, REQUIRED).ok)
        self.assertTrue(gate(n, REQUIRED, {"build-test"}).ok)

    def test_a_failing_optional_job_is_reported_but_does_not_block(self):
        n = needs(fmt="success", plan="failure", **{"build-test": "success", "cargo-audit": "success"})
        verdict = gate(n, REQUIRED)
        self.assertTrue(verdict.ok)
        self.assertIn("INFO plan: failure (not required)", verdict.lines)

    def test_garbage_input_is_an_error(self):
        with self.assertRaises(ValueError):
            parse_needs("[1, 2]")
        with self.assertRaises(ValueError):
            parse_needs("{not json")
        self.assertEqual(parse_needs(""), {})

    def test_only_all_success_passes_property(self):
        rng = random.Random(5)
        results = ["success", "failure", "cancelled", "skipped", None]
        for _ in range(1000):
            n = {}
            for job in REQUIRED:
                r = rng.choice(results)
                if r is not None:
                    n[job] = {"result": r}
            expected = all(n.get(j, {}).get("result") == "success" for j in REQUIRED)
            self.assertEqual(gate(n, REQUIRED).ok, expected)


class WorkflowWiring(unittest.TestCase):
    """The policy's required jobs exist in ci.yml and are all needed by ci-gate."""

    def test_required_jobs_are_wired_into_the_gate(self):
        text = CI_YML.read_text(encoding="utf-8")
        jobs = set(re.findall(r"^  ([a-z][a-z0-9-]*):\s*$", text, re.M))
        needs = re.search(r"^  ci-gate:.*?^    needs: \[([^\]]*)\]", text, re.M | re.S)
        self.assertIsNotNone(needs)
        gate_needs = {n.strip() for n in needs.group(1).split(",")}
        for job in POLICY["gate"]["required"]:
            self.assertIn(job, jobs)
            self.assertIn(job, gate_needs)
        self.assertRegex(text, r"ci-gate:\n(?:.*\n)*?    if: always\(\)")


if __name__ == "__main__":
    unittest.main()
