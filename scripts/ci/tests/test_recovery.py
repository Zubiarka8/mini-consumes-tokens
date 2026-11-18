import unittest

from helpers import POLICY

import recovery as rec

PATTERNS = POLICY["recovery"]["transient_patterns"]
MAX = POLICY["recovery"]["max_attempts"]
# Real annotation of job "fuzz smoke test (mct-lang-rust, macos-latest)", run 44104590502.
NOT_ACQUIRED = "The job was not acquired by Runner of type hosted even after multiple attempts"


def job(kind_text, conclusion, job_id=1, name="j"):
    kind, evidence = rec.classify(conclusion, kind_text, PATTERNS)
    return rec.JobDiagnosis(job_id, name, conclusion, kind, evidence)


class Classify(unittest.TestCase):
    def test_runner_never_acquired_is_transient(self):
        self.assertEqual(rec.classify("cancelled", NOT_ACQUIRED, PATTERNS)[0], rec.TRANSIENT)

    def test_timeout_with_lost_runner_is_transient(self):
        text = "The self-hosted runner lost communication with the server."
        self.assertEqual(rec.classify("failure", text, PATTERNS)[0], rec.TRANSIENT)

    def test_test_failure_is_deterministic(self):
        text = "Process completed with exit code 101."
        self.assertEqual(rec.classify("failure", text, PATTERNS)[0], rec.DETERMINISTIC)

    def test_a_plain_cancel_is_not_rerun(self):
        self.assertEqual(rec.classify("cancelled", "", PATTERNS)[0], rec.CANCELLED)

    def test_success_and_running(self):
        self.assertEqual(rec.classify("success", NOT_ACQUIRED, PATTERNS)[0], rec.OK)
        self.assertEqual(rec.classify(None, "", PATTERNS)[0], rec.PENDING)


class Reruns(unittest.TestCase):
    def setUp(self):
        self.jobs = [
            job(NOT_ACQUIRED, "cancelled", 1, "macos"),
            job("exit code 101", "failure", 2, "tests"),
            job("", "cancelled", 3, "superseded"),
            job("", "success", 4, "fmt"),
        ]

    def test_only_transient_jobs_are_rerun(self):
        self.assertEqual([d.job_id for d in rec.rerun_candidates(self.jobs, 1, MAX)], [1])

    def test_rerun_budget_is_bounded(self):
        self.assertEqual(rec.rerun_candidates(self.jobs, MAX, MAX), [])
        self.assertEqual(rec.rerun_candidates(self.jobs, MAX + 5, MAX), [])

    def test_a_deterministic_failure_cannot_become_green_by_rerunning(self):
        for attempt in range(1, MAX + 3):
            picked = rec.rerun_candidates(self.jobs, attempt, MAX)
            self.assertNotIn(2, [d.job_id for d in picked])


if __name__ == "__main__":
    unittest.main()
