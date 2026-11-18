"""Tell infrastructure failures from code failures, and decide what may be rerun.

Only a job whose failure text matches a transient pattern from the policy
(runner never acquired, lost connection, 5xx from a service) is a rerun
candidate, and only while the run's attempt number is below
`recovery.max_attempts`. A test or compile failure is deterministic and is
never rerun: rerunning it, here or on another provider, cannot turn it green.
A plain cancellation (a newer push superseded the run, or a person cancelled
it) is not rerun either.
"""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass

OK, TRANSIENT, DETERMINISTIC, CANCELLED, PENDING = (
    "ok",
    "transient",
    "deterministic",
    "cancelled",
    "pending",
)


@dataclass(frozen=True)
class JobDiagnosis:
    job_id: int
    name: str
    conclusion: str | None
    kind: str
    evidence: str


def classify(conclusion: str | None, text: str, patterns: list[str]) -> tuple[str, str]:
    """Return (kind, evidence): the first matching pattern, if any."""
    if conclusion in ("success", "skipped", "neutral"):
        return OK, ""
    if conclusion is None:
        return PENDING, ""
    hit = next((p for p in patterns if p in text), "")
    if hit:
        return TRANSIENT, hit
    if conclusion == "cancelled":
        return CANCELLED, "cancelled with no infrastructure message"
    return DETERMINISTIC, "no infrastructure message: treat as a code failure"


def rerun_candidates(
    diagnoses: list[JobDiagnosis], run_attempt: int, max_attempts: int
) -> list[JobDiagnosis]:
    if run_attempt >= max_attempts:
        return []
    return [d for d in diagnoses if d.kind == TRANSIENT]


def _gh(args: list[str]) -> str:
    proc = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)}: {proc.stderr.strip()}")
    return proc.stdout


def diagnose_run(repo: str, run_id: int, patterns: list[str]) -> tuple[int, list[JobDiagnosis]]:
    """Fetch a run's jobs and their annotations through `gh api`."""
    run = json.loads(_gh(["api", f"repos/{repo}/actions/runs/{run_id}"]))
    jobs = json.loads(_gh(["api", f"repos/{repo}/actions/runs/{run_id}/jobs?per_page=100"]))
    out: list[JobDiagnosis] = []
    for job in jobs.get("jobs", []):
        text = ""
        if job.get("conclusion") not in ("success", "skipped", None):
            notes = json.loads(_gh(["api", f"repos/{repo}/check-runs/{job['id']}/annotations"]))
            text = "\n".join(n.get("message", "") for n in notes)
        kind, evidence = classify(job.get("conclusion"), text, patterns)
        out.append(JobDiagnosis(job["id"], job["name"], job.get("conclusion"), kind, evidence))
    return int(run.get("run_attempt", 1)), out


def rerun_job(repo: str, job_id: int) -> None:
    _gh(["api", "-X", "POST", f"repos/{repo}/actions/jobs/{job_id}/rerun"])
