#!/usr/bin/env python3
"""End-to-end agent benchmark: Claude Code with and without mct-mcp-server.

Runs each task of `tasks.json` in headless Claude Code (`claude -p`) once per
scenario and repetition, each run in its own fresh snapshot of the repository,
and reads token usage, cost, turns and tool calls from Claude Code's own
`stream-json` output (never from byte counts). See ../agent-benchmark.md.

    agent_bench.py selftest                      # no model calls
    agent_bench.py run --dry-run                 # set up and print, no model calls
    agent_bench.py run --reps 5 --max-budget-usd 1.0
    agent_bench.py report <run dir>              # Markdown summary

Stdlib only.
"""
import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
SERVER = "mct"
# Files that tell an agent how to use this repo's MCP tools. Removed from every
# snapshot so scenario A is not steered towards tools it does not have.
AGENT_FILES = ["CLAUDE.md", "AGENTS.md", "rules.md", "LOCAL_INSTRUCTIONS.md", ".mcp.json",
               ".claude", ".codex", ".agents", "skills-lock.json"]
BUILTIN_TOOLS = "Read,Grep,Glob,Edit,Write,Bash"
ALLOWED = ["Read", "Grep", "Glob", "Edit", "Write", "Bash(cargo:*)", "Bash(git:*)", "Bash(cd:*)",
           "Bash(ls:*)", "Bash(grep:*)", "Bash(rg:*)", "Bash(find:*)", "Bash(cat:*)",
           "Bash(head:*)", "Bash(tail:*)", "Bash(sed -n:*)", "Bash(wc:*)"]
# ENABLE_TOOL_SEARCH is pinned in every scenario so Claude Code's default
# ("auto") cannot silently differ between them. Deferral needs the ToolSearch
# tool, which `--tools` drops unless it is listed: C adds it.
SCENARIOS = {
    "A-no-mcp": {"mcp": False, "env": {"ENABLE_TOOL_SEARCH": "false"}},
    "B-mcp": {"mcp": True, "env": {"ENABLE_TOOL_SEARCH": "false"}},
    "C-mcp-deferred-catalog": {"mcp": True, "env": {"ENABLE_TOOL_SEARCH": "true"}},
}
# Credentials that would bill an API account instead of the logged-in
# subscription. Removed from every run's environment.
API_ENV = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK",
           "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY"]
PATH_ITEM = re.compile(r"[\w./-]+\.rs(?::\d+)?")


def load_tasks():
    return json.loads((HERE / "tasks.json").read_text())


def snapshot(commit, task, dest):
    """Fresh copy of `commit` with the task's bug applied, as its own git repo."""
    dest.mkdir(parents=True)
    archive = subprocess.run(["git", "-C", str(REPO), "archive", commit],
                             check=True, capture_output=True).stdout
    subprocess.run(["tar", "-x", "-C", str(dest)], input=archive, check=True)
    for name in AGENT_FILES:
        path = dest / name
        if path.is_dir():
            shutil.rmtree(path)
        elif path.exists():
            path.unlink()
    for edit in task.get("setup", []):
        path = dest / edit["file"]
        text = path.read_text()
        if text.count(edit["old"]) != 1:
            raise SystemExit(f"{task['id']}: setup text not found exactly once in {edit['file']}")
        path.write_text(text.replace(edit["old"], edit["new"]))
    git = ["git", "-C", str(dest), "-c", "user.name=bench", "-c", "user.email=bench@localhost"]
    subprocess.run(git + ["init", "-q"], check=True)
    subprocess.run(git + ["add", "-A"], check=True)
    subprocess.run(git + ["commit", "-qm", "snapshot"], check=True)


def answer_line(text):
    lines = [l for l in (text or "").splitlines() if l.strip().startswith("ANSWER:")]
    return lines[-1].split("ANSWER:", 1)[1].strip() if lines else ""


def grade(task, final_text, workdir):
    """(score 0..1, success, details). Mirrors mct-eval: expected patterns
    found, zeroed by any `absent` pattern (a wrong hit)."""
    details = {}
    if "check" in task:
        check = task["check"]
        proc = subprocess.run(check["command"], cwd=workdir, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        missing = [s for s in check.get("stdout_contains", []) if s not in out]
        tests_ok = True
        if "tests_unchanged" in check:
            tests_ok = tests_block(workdir / check["tests_unchanged"]) == tests_block(
                workdir.parent / "orig" / check["tests_unchanged"])
        ok = proc.returncode == 0 and not missing and tests_ok
        details.update(exit=proc.returncode, missing=missing, tests_unchanged=tests_ok)
        return (1.0 if ok else 0.0), ok, details
    answer = answer_line(final_text)
    found = [p for p in task["expect"] if re.search(p, answer)]
    wrong = [p for p in task.get("absent", []) if re.search(p, answer)]
    if task.get("only_expected_paths"):  # a listed path no pattern accounts for is a false positive
        wrong += [i for i in PATH_ITEM.findall(answer)
                  if not any(re.fullmatch(p, i) for p in task["expect"])]
    edited = [f for f in task.get("unchanged", [])
              if (workdir / f).read_text() != (workdir.parent / "orig" / f).read_text()]
    score = 0.0 if wrong or edited else len(found) / len(task["expect"])
    details.update(answer=answer, missing=[p for p in task["expect"] if p not in found],
                   unexpected=wrong, edited=edited)
    return score, score == 1.0, details


def tests_block(path):
    text = path.read_text()
    return text[text.find("#[cfg(test)]"):]


def keep_originals(task, workdir):
    """Copies of files the grader compares against, kept next to (not inside)
    the snapshot so the agent never sees them."""
    files = list(task.get("unchanged", []))
    if "check" in task and "tests_unchanged" in task["check"]:
        files.append(task["check"]["tests_unchanged"])
    for f in files:
        dst = workdir.parent / "orig" / f
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(workdir / f, dst)
    (workdir / ".git" / "info" / "exclude").write_text(".mct-index/\ntarget/\n")


def deferred(scenario):
    return SCENARIOS[scenario]["env"]["ENABLE_TOOL_SEARCH"] == "true"


def claude_cmd(args, prompt, scenario, workdir, server):
    cmd = ["claude", "-p", prompt, "--output-format", "stream-json", "--verbose",
           "--include-hook-events", "--no-session-persistence",
           "--model", args.model, "--effort", args.effort,
           "--setting-sources", "project", "--strict-mcp-config",
           "--tools", BUILTIN_TOOLS + (",ToolSearch" if deferred(scenario) else ""),
           "--permission-mode", "acceptEdits",
           "--max-turns", str(args.max_turns)]
    if args.max_budget_usd:
        cmd += ["--max-budget-usd", str(args.max_budget_usd)]
    allowed = list(ALLOWED)
    servers = {}
    if SCENARIOS[scenario]["mcp"]:
        servers[SERVER] = {"type": "stdio", "command": str(server),
                           "args": ["--root", str(workdir), "--reindex-debounce-ms", "0"]}
        allowed.append(f"mcp__{SERVER}__*")
    cmd += ["--mcp-config", json.dumps({"mcpServers": servers}), "--allowedTools", *allowed]
    return cmd


def parse_stream(lines):
    """Metrics from Claude Code's stream-json events."""
    m = {"tool_calls": {}, "tool_errors": 0, "api_retries": 0, "hook_events": 0,
         "init": None, "result": None, "rate_limit": None, "first_request_input": None}
    for line in lines:
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind, sub = ev.get("type"), ev.get("subtype")
        if kind == "system" and sub == "init":
            m["init"] = {k: ev.get(k) for k in ("model", "tools", "mcp_servers", "plugins",
                                                "permissionMode", "apiKeySource")}
        elif kind == "system" and sub == "api_retry":
            m["api_retries"] += 1
        elif kind == "system" and sub and sub.startswith("hook"):
            m["hook_events"] += 1
        elif kind == "assistant":
            u = ev.get("message", {}).get("usage") or {}
            if m["first_request_input"] is None and u:  # system prompt + tool catalog + task, cache-neutral
                m["first_request_input"] = sum(u.get(k) or 0 for k in (
                    "input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
            for block in ev.get("message", {}).get("content", []):
                if block.get("type") == "tool_use":
                    m["tool_calls"][block["name"]] = m["tool_calls"].get(block["name"], 0) + 1
        elif kind == "user":
            content = ev.get("message", {}).get("content", [])
            if isinstance(content, list):
                m["tool_errors"] += sum(1 for b in content
                                        if b.get("type") == "tool_result" and b.get("is_error"))
        elif kind == "rate_limit_event":
            m["rate_limit"] = ev.get("rate_limit_info")
        elif kind == "result":
            m["result"] = ev
    return m


def contamination(scenario, init):
    """Reasons the run's environment is not the scenario it claims to be."""
    if not init:
        return ["no init event"]
    problems = []
    servers = {s.get("name"): s.get("status") for s in init.get("mcp_servers") or []}
    mct_tools = [t for t in init.get("tools") or [] if t.startswith(f"mcp__{SERVER}__")]
    if set(servers) - {SERVER}:
        problems.append(f"unexpected MCP servers {sorted(set(servers) - {SERVER})}")
    if SCENARIOS[scenario]["mcp"]:
        if servers.get(SERVER) != "connected":
            problems.append(f"mct server status {servers.get(SERVER)}")
        if not mct_tools:
            problems.append("mct tools missing from the tool list")
        if deferred(scenario) != ("ToolSearch" in (init.get("tools") or [])):
            problems.append("ToolSearch availability does not match the scenario")
    elif mct_tools:
        problems.append("mct tools present without MCP")
    # Built-in plugins ship with Claude Code and load in every scenario alike.
    extra = [p.get("source") for p in init.get("plugins") or [] if not str(p.get("source")).endswith("@builtin")]
    if extra:
        problems.append(f"plugins loaded {extra}")
    return problems


def billing_check(metrics):
    """Why this run may have been billed beyond the subscription, if it was."""
    init, limit = metrics["init"] or {}, metrics["rate_limit"] or {}
    problems = []
    if init.get("apiKeySource") not in (None, "none"):
        problems.append(f"apiKeySource {init['apiKeySource']}")
    if limit.get("isUsingOverage") or limit.get("overageStatus") in ("allowed", "allowed_warning"):
        problems.append(f"overage {limit}")
    return problems


def run(args):
    spec = load_tasks()
    account = json.loads((Path.home() / ".claude.json").read_text()).get("oauthAccount") or {}
    if account.get("hasExtraUsageEnabled") is not False and not args.dry_run:
        raise SystemExit("extra usage is not known to be off for this account: refusing to run")
    commit = args.commit or spec["commit"]
    tasks = [t for t in spec["tasks"] if not args.task or t["id"] in args.task]
    scenarios = args.scenario or list(SCENARIOS)
    server = Path(args.server).resolve()
    if not server.exists() and not args.dry_run:
        raise SystemExit(f"{server} not found: cargo build --release -p mct-mcp-server --locked")
    out = Path(args.out or REPO / "target" / "agent-bench" / time.strftime("%Y%m%d-%H%M%S"))
    out.mkdir(parents=True, exist_ok=True)
    (out / "config.json").write_text(json.dumps({
        "commit": commit, "model": args.model, "effort": args.effort, "reps": args.reps,
        "max_turns": args.max_turns, "max_budget_usd": args.max_budget_usd,
        "server": str(server), "scenarios": {s: SCENARIOS[s] for s in scenarios},
        "claude_version": subprocess.run(["claude", "--version"], capture_output=True,
                                         text=True).stdout.strip()}, indent=2))
    total = args.reps * len(tasks) * len(scenarios)
    print(f"{total} runs -> {out}", file=sys.stderr)
    n = 0
    # Repetition-major, scenarios rotated per task, so time-of-day drift and
    # the account-wide prompt cache do not systematically favor one scenario.
    for rep in range(args.reps):
        for ti, task in enumerate(tasks):
            order = scenarios[(rep + ti) % len(scenarios):] + scenarios[:(rep + ti) % len(scenarios)]
            for scenario in order:
                n += 1
                run_dir = out / scenario / task["id"] / f"rep{rep}"
                run_dir.mkdir(parents=True, exist_ok=True)
                with tempfile.TemporaryDirectory(prefix="mct-bench-") as tmp:
                    workdir = Path(tmp) / "repo"
                    snapshot(commit, task, workdir)
                    keep_originals(task, workdir)
                    prompt = f"{task['prompt']}\n\n{spec['answer_rule']}" if "check" not in task else task["prompt"]
                    cmd = claude_cmd(args, prompt, scenario, workdir, server)
                    if args.dry_run:
                        print(f"[{n}/{total}] {scenario} {task['id']} rep{rep}\n  cwd={workdir}\n  "
                              + " ".join(map(json.dumps, cmd)), file=sys.stderr)
                        continue
                    env = {k: v for k, v in os.environ.items() if k not in API_ENV}
                    env.update(SCENARIOS[scenario]["env"])
                    start = time.monotonic()
                    proc = subprocess.run(cmd, cwd=workdir, env=env, capture_output=True,
                                          text=True, stdin=subprocess.DEVNULL)
                    wall = time.monotonic() - start
                    (run_dir / "stream.jsonl").write_text(proc.stdout)
                    (run_dir / "stderr.txt").write_text(proc.stderr)
                    metrics = parse_stream(proc.stdout.splitlines())
                    result = metrics["result"] or {}
                    score, success, details = grade(task, result.get("result", ""), workdir)
                    record = {
                        "scenario": scenario, "task": task["id"], "category": task["category"],
                        "rep": rep, "exit": proc.returncode, "wall_s": round(wall, 1),
                        "score": score, "success": success, "grading": details,
                        "contamination": contamination(scenario, metrics["init"]),
                        "result_subtype": result.get("subtype"), "is_error": result.get("is_error"),
                        "usage": result.get("usage"), "model_usage": result.get("modelUsage"),
                        "cost_usd": result.get("total_cost_usd"), "num_turns": result.get("num_turns"),
                        "duration_ms": result.get("duration_ms"),
                        "permission_denials": len(result.get("permission_denials") or []),
                        **{k: metrics[k] for k in ("tool_calls", "tool_errors", "api_retries",
                                                   "hook_events", "init", "rate_limit",
                                                   "first_request_input")},
                        "billing": billing_check(metrics),
                    }
                    (run_dir / "run.json").write_text(json.dumps(record, indent=2))
                    print(f"[{n}/{total}] {scenario} {task['id']} rep{rep}: success={success} "
                          f"cost={record['cost_usd']} turns={record['num_turns']} "
                          f"contamination={record['contamination']}", file=sys.stderr)
                    if record["billing"]:
                        raise SystemExit(f"stopping: possible billing beyond the subscription: {record['billing']}")
    if not args.dry_run:
        print(report(out))


def stat(values):
    values = [v for v in values if v is not None]
    if not values:
        return "n/a"
    mean = statistics.mean(values)
    sd = statistics.stdev(values) if len(values) > 1 else 0.0
    return f"{mean:,.4g} ± {sd:,.2g}"


def report(out):
    out = Path(out)
    runs = [json.loads(p.read_text()) for p in sorted(out.glob("*/*/*/run.json"))]
    if not runs:
        return f"No runs under {out}."
    config = json.loads((out / "config.json").read_text())
    scenarios = sorted({r["scenario"] for r in runs})
    tasks = list(dict.fromkeys(r["task"] for r in runs))

    def tokens(r, key):
        return (r.get("usage") or {}).get(key)

    def total_in(r):
        u = r.get("usage") or {}
        return sum(u.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens",
                                           "cache_read_input_tokens")) if u else None

    def mct_calls(r):
        return sum(n for name, n in r["tool_calls"].items() if name.startswith(f"mcp__{SERVER}__"))

    def cost_per_success(rs):
        wins = sum(r["success"] for r in rs)
        cost = sum(r["cost_usd"] or 0 for r in rs)
        return f"{cost / wins:.4f}" if wins else "∞ (0 successes)"

    lines = [f"# Agent benchmark — {out.name}", "",
             f"Commit `{config['commit']}`, model `{config['model']}`, effort `{config['effort']}`, "
             f"{config['claude_version']}, {config['reps']} repetition(s). "
             "Values are mean ± sample standard deviation over runs; tokens and cost are "
             "Claude Code's own `result` event. Cost is Claude Code's `total_cost_usd` estimate, "
             "which is notional on a subscription and depends on prompt-cache hits, so on run "
             "order; compare token totals first. First request input is the system prompt, "
             "tool catalog and task of the first API call.", "",
             "## Per scenario", "",
             "| Scenario | Runs | Success | Cost per successful task (USD) | Cost/run (USD) | "
             "Input | Cache write | Cache read | Total input | First request input | Output | Turns | "
             "Tool calls | mct calls | Tool errors | API retries | Duration (s) | Contaminated |",
             "|" + "---|" * 18]
    for s in scenarios:
        rs = [r for r in runs if r["scenario"] == s]
        lines.append(" | ".join([
            f"| {s}", str(len(rs)), f"{sum(r['success'] for r in rs)}/{len(rs)}", cost_per_success(rs),
            stat([r["cost_usd"] for r in rs]), stat([tokens(r, "input_tokens") for r in rs]),
            stat([tokens(r, "cache_creation_input_tokens") for r in rs]),
            stat([tokens(r, "cache_read_input_tokens") for r in rs]), stat([total_in(r) for r in rs]),
            stat([r.get("first_request_input") for r in rs]),
            stat([tokens(r, "output_tokens") for r in rs]), stat([r["num_turns"] for r in rs]),
            stat([sum(r["tool_calls"].values()) for r in rs]), stat([mct_calls(r) for r in rs]),
            stat([r["tool_errors"] + r["permission_denials"] for r in rs]),
            stat([r["api_retries"] for r in rs]),
            stat([(r["duration_ms"] or 0) / 1000 for r in rs]),
            str(sum(bool(r["contamination"]) for r in rs)) + " |"]))
    lines += ["", "## Per task", "",
              "| Task | Scenario | Success | Mean score | Cost per success (USD) | Total input | "
              "Output | Turns | mct calls | Tools used |", "|" + "---|" * 10]
    for t in tasks:
        for s in scenarios:
            rs = [r for r in runs if r["task"] == t and r["scenario"] == s]
            if not rs:
                continue
            used = {}
            for r in rs:
                for name, n in r["tool_calls"].items():
                    used[name] = used.get(name, 0) + n
            lines.append(" | ".join([
                f"| {t}", s, f"{sum(r['success'] for r in rs)}/{len(rs)}",
                f"{statistics.mean(r['score'] for r in rs):.2f}", cost_per_success(rs),
                stat([total_in(r) for r in rs]), stat([tokens(r, "output_tokens") for r in rs]),
                stat([r["num_turns"] for r in rs]), stat([mct_calls(r) for r in rs]),
                ", ".join(f"{k.removeprefix(f'mcp__{SERVER}__')}×{v}" for k, v in
                          sorted(used.items(), key=lambda kv: -kv[1])) + " |"]))
    bad = [r for r in runs if r["contamination"]]
    if bad:
        lines += ["", "## Contaminated runs (exclude or rerun)", ""]
        lines += [f"- {r['scenario']} / {r['task']} / rep{r['rep']}: {r['contamination']}" for r in bad]
    unused = [r for r in runs if SCENARIOS.get(r["scenario"], {}).get("mcp") and not mct_calls(r)]
    if unused:
        lines += ["", f"MCP connected but never called in {len(unused)} run(s): "
                  + ", ".join(sorted({f"{r['scenario']}/{r['task']}" for r in unused})) + "."]
    return "\n".join(lines)


def selftest():
    """Checks the setup and the graders without any model call."""
    spec = load_tasks()
    with tempfile.TemporaryDirectory(prefix="mct-bench-selftest-") as tmp:
        for task in spec["tasks"]:
            workdir = Path(tmp) / task["id"] / "repo"
            snapshot(spec["commit"], task, workdir)
            keep_originals(task, workdir)
            assert not (workdir / "CLAUDE.md").exists() and not (workdir / ".mcp.json").exists()
            if "check" in task:
                assert not grade(task, "", workdir)[1], f"{task['id']}: passes before any fix"
                for edit in task["setup"]:  # undo the injected bug = the reference fix
                    p = workdir / edit["file"]
                    p.write_text(p.read_text().replace(edit["new"], edit["old"]))
                ok, details = grade(task, "", workdir)[1:]
                assert ok, f"{task['id']}: reference fix does not pass: {details}"
                print(f"ok {task['id']}", file=sys.stderr)
                continue
            plain = lambda p: re.sub(r"\\b|\(\?i\)", "", p).replace("\\", "")
            perfect = "ANSWER: " + task.get("reference_answer", ", ".join(
                plain(p).replace("[5-7]", "6") for p in task["expect"]))
            score, ok, details = grade(task, "prose\n" + perfect, workdir)
            assert ok, f"{task['id']}: reference answer fails: {details}"
            assert grade(task, "no answer line", workdir)[0] == 0.0
            if task.get("only_expected_paths"):
                assert grade(task, perfect + ", crates/x/src/y.rs:1", workdir)[0] == 0.0
            for p in task.get("absent", []):
                assert grade(task, perfect + ", " + plain(p), workdir)[0] == 0.0
            for f in task.get("unchanged", []):
                (workdir / f).write_text("edited")
                assert not grade(task, perfect, workdir)[1], f"{task['id']}: edit not detected"
            print(f"ok {task['id']}", file=sys.stderr)
    sample = ['{"type":"system","subtype":"init","model":"m","tools":["Read","mcp__mct__find_symbol"],'
              '"mcp_servers":[{"name":"mct","status":"connected"}],"plugins":[]}',
              '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__mct__find_symbol"}]}}',
              '{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}',
              '{"type":"result","subtype":"success","total_cost_usd":0.1,"usage":{"input_tokens":5}}']
    m = parse_stream(sample)
    assert billing_check(m) == []
    over = parse_stream(sample + ['{"type":"rate_limit_event","rate_limit_info":{"isUsingOverage":true}}'])
    assert billing_check(over)
    assert m["tool_calls"] == {"mcp__mct__find_symbol": 1} and m["tool_errors"] == 1
    assert contamination("B-mcp", m["init"]) == []
    assert contamination("A-no-mcp", m["init"]) == ["mct tools present without MCP"]
    assert contamination("C-mcp-deferred-catalog", m["init"]) == [
        "ToolSearch availability does not match the scenario"]
    assert m["first_request_input"] is None
    print("selftest ok", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--reps", type=int, default=1)
    r.add_argument("--model", default="sonnet")
    r.add_argument("--effort", default="medium")
    r.add_argument("--max-turns", type=int, default=40)
    r.add_argument("--max-budget-usd", type=float, help="per-run cap passed to claude")
    r.add_argument("--scenario", action="append", choices=list(SCENARIOS))
    r.add_argument("--task", action="append", help="task id; repeatable; default all")
    r.add_argument("--commit", help="override tasks.json's commit")
    r.add_argument("--server", default=str(REPO / "target" / "release" / "mct-mcp-server"))
    r.add_argument("--out")
    r.add_argument("--dry-run", action="store_true", help="set up and print commands, no model calls")
    rp = sub.add_parser("report")
    rp.add_argument("dir")
    sub.add_parser("selftest")
    args = parser.parse_args()
    if args.cmd == "run":
        run(args)
    elif args.cmd == "report":
        print(report(args.dir))
    else:
        selftest()


if __name__ == "__main__":
    main()
