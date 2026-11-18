"""Queue time, duration and outcome of CI jobs, from the GitHub API.

Fuzz jobs are grouped per OS so 18 crates read as one line. Only jobs that
ran to `success` or `failure` count toward durations and runner minutes; a
cancelled job's time is not comparable. Queue time is `started_at -
created_at` of the job, which is what a contributor waits before a runner
picks the job up.
"""

from __future__ import annotations

import json
import subprocess
import time
from collections import Counter, defaultdict
from datetime import datetime


def _gh_json(path: str, retries: int = 4) -> dict:
    for attempt in range(retries):
        proc = subprocess.run(["gh", "api", path], capture_output=True, text=True, check=False)
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"gh api {path} failed after {retries} attempts")


def _ts(text: str | None) -> datetime | None:
    return datetime.fromisoformat(text.replace("Z", "+00:00")) if text else None


def group_of(name: str) -> str:
    if name.startswith("fuzz"):
        return "fuzz (" + ("macos" if "macos" in name else "ubuntu") + ")"
    return name


def _median(values: list[float]) -> float | None:
    values = sorted(values)
    return round(values[len(values) // 2], 1) if values else None


def summarize(runs: list[dict], jobs_by_run: dict[int, list[dict]]) -> list[str]:
    outcomes: dict[str, Counter] = defaultdict(Counter)
    queue: dict[str, list[float]] = defaultdict(list)
    duration: dict[str, list[float]] = defaultdict(list)
    run_outcomes: Counter = Counter()
    minutes = 0.0
    for run in runs:
        run_outcomes[f"{run['event']}/{run['conclusion']}"] += 1
        for job in jobs_by_run.get(run["id"], []):
            group = group_of(job["name"])
            outcomes[group][job["conclusion"]] += 1
            created, started, done = (_ts(job.get(k)) for k in ("created_at", "started_at", "completed_at"))
            if started and done and job["conclusion"] in ("success", "failure"):
                queue[group].append((started - created).total_seconds() / 60)
                duration[group].append((done - started).total_seconds() / 60)
                minutes += (done - started).total_seconds() / 60
    lines = [f"runs={len(runs)} " + " ".join(f"{k}={v}" for k, v in sorted(run_outcomes.items()))]
    for group in sorted(outcomes):
        counts = " ".join(f"{k}={v}" for k, v in sorted(outcomes[group].items(), key=str))
        lines.append(
            f"job={group!r} {counts} queue_median_min={_median(queue[group])} "
            f"queue_max_min={round(max(queue[group]), 1) if queue[group] else None} "
            f"duration_median_min={_median(duration[group])}"
        )
    lines.append(f"runner_minutes_completed_jobs={round(minutes)}")
    return lines


def collect(repo: str, workflow: str, since: str, limit: int) -> list[str]:
    data = _gh_json(f"repos/{repo}/actions/runs?per_page={min(limit, 100)}&created=>={since}")
    runs = [r for r in data.get("workflow_runs", []) if r["name"] == workflow][:limit]
    jobs = {
        r["id"]: _gh_json(f"repos/{repo}/actions/runs/{r['id']}/jobs?per_page=100").get("jobs", [])
        for r in runs
    }
    return summarize(runs, jobs)
