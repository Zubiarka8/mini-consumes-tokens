#!/usr/bin/env python3
"""CI planner entry point: plan, run, gate, cache-key, recovery, metrics.

Python standard library only, no project build needed, so CI can plan and
diagnose itself even when the Rust workspace does not compile.

Exit codes: 0 ok, 1 the check ran and failed (gate blocked, tests failed,
wrong runner arch), 2 usage or environment error.

Examples:
  python3 scripts/ci/main.py plan --event pull_request --base origin/main --head HEAD
  python3 scripts/ci/main.py run --event pull_request --base origin/main --dry-run
  python3 scripts/ci/main.py gate --needs-file needs.json
  python3 scripts/ci/main.py recovery --repo OWNER/REPO --run 123 [--apply]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import cache_manager  # noqa: E402
import classify_risk  # noqa: E402
import collect_results  # noqa: E402
import dependency_graph  # noqa: E402
import detect_changes  # noqa: E402
import generate_matrix  # noqa: E402
import metrics  # noqa: E402
import policy as pol  # noqa: E402
import recovery  # noqa: E402
import run_tests  # noqa: E402

RANGE_EVENTS = ("pull_request", "pull_request_target", "push", "merge_group")


def _write_github_file(env_key: str, text: str) -> None:
    path = os.environ.get(env_key)
    if path:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(text)


def make_plan(args: argparse.Namespace, policy: dict) -> classify_risk.Plan:
    if args.event not in RANGE_EVENTS:
        return classify_risk.full_plan(f"event {args.event}: full validation by design")
    try:
        changes = detect_changes.changed_files(args.base, args.head)
    except detect_changes.RangeError as err:
        if args.strict:
            raise
        pol.log("range_unresolved", level="warn", error=str(err))
        return classify_risk.full_plan(f"git range unresolved: {err}")
    try:
        if args.metadata:
            graph = dependency_graph.from_metadata(
                json.loads(Path(args.metadata).read_text(encoding="utf-8"))
            )
        else:
            graph = dependency_graph.load()
    except (OSError, ValueError, KeyError, dependency_graph.MetadataError) as err:
        if args.strict:
            raise
        pol.log("graph_unavailable", level="warn", error=str(err))
        return classify_risk.full_plan(f"crate graph unavailable: {err}")
    pol.log("changes", count=len(changes))
    return classify_risk.classify(changes, graph, policy)


def cmd_plan(args: argparse.Namespace, policy: dict) -> int:
    plan = make_plan(args, policy)
    matrix = generate_matrix.build(plan, policy, scope=args.scope)
    for line in plan.as_lines():
        print(line)
    for entry in matrix.include:
        print(f"platform {entry['id']} {entry['os']} {entry['target']} tests={entry['run_tests']}")
    for pid, why in matrix.skipped:
        print(f"skipped_platform {pid}: {why}")
    crates = "*" if plan.full else ",".join(sorted(plan.test_crates))
    _write_github_file(
        "GITHUB_OUTPUT",
        f"level={plan.level}\nfull={str(plan.full).lower()}\ntest_crates={crates}\n"
        f"matrix={json.dumps(matrix.for_actions(), separators=(',', ':'))}\n",
    )
    summary = [f"### CI plan: {plan.level}{' (full suite)' if plan.full else ''}", ""]
    summary += [f"- `{r.path}` {r.level}: {r.why}" for r in plan.reasons[:200]]
    summary += ["", "Platforms: " + (", ".join(e["id"] for e in matrix.include) or "none")]
    summary += [f"- skipped {pid}: {why}" for pid, why in matrix.skipped]
    _write_github_file("GITHUB_STEP_SUMMARY", "\n".join(summary) + "\n")
    return pol.EXIT_OK


def cmd_run(args: argparse.Namespace, policy: dict) -> int:
    plan = make_plan(args, policy)
    cmds = run_tests.commands(plan)
    for cmd in cmds:
        print(("would run: " if args.dry_run else "run: ") + " ".join(cmd))
    if not cmds:
        print(f"nothing to run for {plan.level}")
    if args.dry_run:
        return pol.EXIT_OK
    return pol.EXIT_OK if run_tests.run(cmds) == 0 else pol.EXIT_FAILED


def cmd_gate(args: argparse.Namespace, policy: dict) -> int:
    text = Path(args.needs_file).read_text(encoding="utf-8") if args.needs_file else os.environ.get("NEEDS_JSON", "")
    needs = collect_results.parse_needs(text)
    required = args.required.split(",") if args.required else policy["gate"]["required"]
    allowed = set(filter(None, (args.allow_skip or "").split(",")))
    verdict = collect_results.gate(needs, required, allowed)
    for line in verdict.lines:
        print(line)
    print("gate=" + ("pass" if verdict.ok else "fail"))
    _write_github_file("GITHUB_STEP_SUMMARY", "### CI gate\n\n" + "\n".join(f"- {l}" for l in verdict.lines) + "\n")
    return pol.EXIT_OK if verdict.ok else pol.EXIT_FAILED


def cmd_cache_key(args: argparse.Namespace, policy: dict) -> int:
    runner_os = os.environ.get("RUNNER_OS", sys.platform)
    runner_arch = os.environ.get("RUNNER_ARCH", "")
    if runner_arch:
        try:
            cache_manager.verify_arch(args.target, runner_arch, args.mode)
        except cache_manager.ArchMismatch as err:
            print(f"error: {err}", file=sys.stderr)
            return pol.EXIT_FAILED
    key = cache_manager.key(Path(args.root), args.target, runner_os, runner_arch or "unknown", args.profile)
    print(f"key={key}")
    _write_github_file("GITHUB_OUTPUT", f"key={key}\n")
    return pol.EXIT_OK


def cmd_recovery(args: argparse.Namespace, policy: dict) -> int:
    rec = policy.get("recovery", {})
    attempt, diagnoses = recovery.diagnose_run(args.repo, args.run, rec.get("transient_patterns", []))
    for d in diagnoses:
        if d.kind != recovery.OK:
            print(f"{d.kind:13} {d.conclusion or 'running':10} {d.name}  {d.evidence}")
    rerun = recovery.rerun_candidates(diagnoses, attempt, rec.get("max_attempts", 2))
    blocking = [d for d in diagnoses if d.kind == recovery.DETERMINISTIC]
    print(f"attempt={attempt} transient={len(rerun)} deterministic={len(blocking)}")
    if args.apply:
        for d in rerun:
            recovery.rerun_job(args.repo, d.job_id)
            print(f"rerun requested: {d.name}")
    elif rerun:
        print("dry run: pass --apply to rerun the transient jobs")
    return pol.EXIT_FAILED if blocking else pol.EXIT_OK


def cmd_metrics(args: argparse.Namespace, policy: dict) -> int:
    for line in metrics.collect(args.repo, args.workflow, args.since, args.limit):
        print(line)
    return pol.EXIT_OK


def _add_range_args(sub: argparse.ArgumentParser) -> None:
    sub.add_argument("--event", default="pull_request", help="GitHub event name")
    sub.add_argument("--base", help="base commit (PR base SHA or push `before`)")
    sub.add_argument("--head", default="HEAD", help="head commit")
    sub.add_argument("--metadata", help="cargo metadata JSON file instead of running cargo")
    sub.add_argument("--strict", action="store_true", help="error instead of falling back to the full suite")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--policy", help="policy file (default scripts/ci/ci_policy.toml)")
    subs = parser.add_subparsers(dest="command", required=True)

    sub = subs.add_parser("plan", help="risk level, selected crates and platform matrix")
    _add_range_args(sub)
    sub.add_argument("--scope", choices=("pr", "extra", "all"), default="pr")
    sub.set_defaults(func=cmd_plan)

    sub = subs.add_parser("run", help="run the selected cargo build, test and clippy")
    _add_range_args(sub)
    sub.add_argument("--dry-run", action="store_true", help="print the commands only")
    sub.set_defaults(func=cmd_run)

    sub = subs.add_parser("gate", help="fail unless every required job succeeded")
    sub.add_argument("--needs-file", help="file with toJSON(needs); default $NEEDS_JSON")
    sub.add_argument("--required", help="comma list; default [gate].required")
    sub.add_argument("--allow-skip", help="comma list of required jobs the plan may skip")
    sub.set_defaults(func=cmd_gate)

    sub = subs.add_parser("cache-key", help="cache key for a target; checks runner arch")
    sub.add_argument("--target", required=True)
    sub.add_argument("--mode", choices=("native", "cross"), default="native")
    sub.add_argument("--profile", default="dev")
    sub.add_argument("--root", default=".")
    sub.set_defaults(func=cmd_cache_key)

    sub = subs.add_parser("recovery", help="classify a run's failures; rerun transient ones")
    sub.add_argument("--repo", required=True)
    sub.add_argument("--run", type=int, required=True)
    sub.add_argument("--apply", action="store_true", help="really rerun (default: dry run)")
    sub.set_defaults(func=cmd_recovery)

    sub = subs.add_parser("metrics", help="queue time, duration and outcome per job")
    sub.add_argument("--repo", required=True)
    sub.add_argument("--workflow", default="CI")
    sub.add_argument("--since", required=True, help="YYYY-MM-DD")
    sub.add_argument("--limit", type=int, default=100)
    sub.set_defaults(func=cmd_metrics)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        policy = pol.load(args.policy)
        return args.func(args, policy)
    except (pol.PolicyError, detect_changes.RangeError, dependency_graph.MetadataError,
            generate_matrix.MatrixError, ValueError, RuntimeError, OSError) as err:
        pol.log("error", level="error", command=args.command, error=str(err))
        return pol.EXIT_USAGE


if __name__ == "__main__":
    sys.exit(main())
