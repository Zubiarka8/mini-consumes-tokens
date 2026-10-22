// Example/bench code: a panic here means a broken benchmark precondition,
// not a path processing untrusted repo-input content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Reproducible `detail` / `max_tokens` benchmark over real repositories.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example real_repos --release -- \
//!     <repos-dir> benchmarks/real-repos/tasks.json [repeats]
//! ```
//!
//! `<repos-dir>` holds a checkout per repository of `tasks.json` (the file
//! pins the commits; `benchmarks/real-repos.md` lists the clone commands).
//! Each task has labelled evidence chosen by grep, not from MCT output. For
//! each task every strategy is run in-process through the same MCP tool path
//! the server uses, and reported:
//!
//! - calls, tokens by two *separate* heuristics (`chars/4`, `EstimatedCounter`;
//!   neither is a tokenizer and no billed figure exists here),
//! - median latency over `repeats` warm runs (default 5),
//! - whether every labelled evidence string is in the answers, whether a
//!   forbidden one is, and whether a miss was *silent* (no truncation or
//!   omission marker in any answer).
//!
//! The `ladder` strategy simulates an agent that starts at `overview` and
//! escalates `summary` then `detailed` only while evidence is missing. It is a
//! policy simulation over fixed arguments, not an adaptive server feature.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

const SIGNALS: [&str; 5] = [
    "Budget:",
    "omitted",
    "No row fits",
    "none fit",
    "more line(s)",
];
const SEARCH_BUDGET: u64 = 400;
const PACK_BUDGET: u64 = 1500;

struct Task {
    id: String,
    kind: String,
    tool: String,
    args: Value,
    evidence: Vec<String>,
    forbid: Vec<String>,
}

#[derive(Default, Clone)]
struct Outcome {
    calls: usize,
    chars4: usize,
    estimated: usize,
    micros: Vec<u128>,
    found: usize,
    total: usize,
    forbidden: bool,
    signalled: bool,
    first_hit_line: Option<usize>,
}

impl Outcome {
    fn satisfied(&self) -> bool {
        self.found == self.total && !self.forbidden
    }
    fn silent_miss(&self) -> bool {
        !self.satisfied() && !self.signalled
    }
    fn median(&self) -> u128 {
        let mut v = self.micros.clone();
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    }
}

fn arr(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn with(args: &Value, extra: Value) -> Value {
    let mut out = args.clone();
    for (k, v) in extra.as_object().unwrap() {
        out[k] = v.clone();
    }
    out
}

async fn call(server: &MctServer, tool: &str, args: &Value) -> String {
    let result = server
        .call_read_only_tool(tool, args.as_object().cloned())
        .await
        .unwrap();
    result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn next_offset(text: &str) -> Option<usize> {
    text.split("next: offset=")
        .nth(1)
        .and_then(|r| r.split(['\n', ';', ' ']).next())
        .and_then(|n| n.trim().parse::<usize>().ok())
}

/// One execution of a strategy; returns the answers in order.
async fn execute(server: &MctServer, task: &Task, strategy: &str) -> Vec<String> {
    let is_search = task.tool != "build_context_pack";
    let base = if is_search {
        with(&task.args, json!({"cache": false}))
    } else {
        task.args.clone()
    };
    let has = |answers: &[String]| {
        let all = answers.join("\n");
        task.evidence.iter().all(|e| all.contains(e))
    };
    let mut answers = Vec::new();
    match strategy {
        "default" => answers.push(call(server, &task.tool, &base).await),
        "overview" | "summary" | "detailed" => {
            let a = with(&base, json!({"detail": strategy}));
            answers.push(call(server, &task.tool, &a).await);
        }
        "detailed+budget" => {
            let budget = if is_search {
                SEARCH_BUDGET * 4
            } else {
                PACK_BUDGET
            };
            let a = with(&base, json!({"detail": "detailed", "max_tokens": budget}));
            answers.push(call(server, &task.tool, &a).await);
        }
        "overview+budget (follow next)" => {
            let mut offset = 0usize;
            for _ in 0..20 {
                let mut a = with(
                    &base,
                    json!({"detail": "overview", "max_tokens": SEARCH_BUDGET}),
                );
                if is_search {
                    a["offset"] = json!(offset);
                }
                let text = call(server, &task.tool, &a).await;
                let next = next_offset(&text);
                answers.push(text);
                if has(&answers) || !is_search {
                    break;
                }
                match next {
                    Some(n) => offset = n,
                    None => break,
                }
            }
        }
        "ladder" => {
            for level in ["overview", "summary", "detailed"] {
                let a = with(&base, json!({"detail": level}));
                answers.push(call(server, &task.tool, &a).await);
                if has(&answers) {
                    break;
                }
            }
        }
        other => panic!("unknown strategy {other}"),
    }
    answers
}

async fn measure(server: &MctServer, task: &Task, strategy: &str, repeats: usize) -> Outcome {
    // Warm-up run (page cache, statement cache) is discarded.
    let mut answers = execute(server, task, strategy).await;
    let mut out = Outcome::default();
    for _ in 0..repeats {
        let start = Instant::now();
        answers = execute(server, task, strategy).await;
        out.micros.push(start.elapsed().as_micros());
    }
    let all = answers.join("\n");
    out.calls = answers.len();
    out.chars4 = answers.iter().map(|a| a.chars().count().div_ceil(4)).sum();
    out.estimated = answers
        .iter()
        .map(|a| EstimatedCounter.count(a).value)
        .sum();
    out.total = task.evidence.len();
    out.found = task.evidence.iter().filter(|e| all.contains(*e)).count();
    out.forbidden = task.forbid.iter().any(|f| all.contains(f));
    out.signalled = answers
        .iter()
        .any(|a| SIGNALS.iter().any(|s| a.contains(s)));
    if let Some(first) = task.evidence.first() {
        out.first_hit_line = answers
            .first()
            .and_then(|a| a.lines().position(|l| l.contains(first)))
            .map(|p| p + 1);
    }
    out
}

fn dir_size_kb(p: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let md = e.metadata().unwrap();
            total += if md.is_dir() {
                dir_size_kb(&e.path())
            } else {
                md.len() / 1024
            };
        }
    }
    total
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let repos_dir = PathBuf::from(
        args.next()
            .expect("usage: real_repos <repos-dir> <tasks.json> [repeats]"),
    );
    let tasks_path = args.next().expect("tasks.json");
    let repeats: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(5);
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(tasks_path).unwrap()).unwrap();

    let strategies = [
        "default",
        "overview",
        "summary",
        "detailed",
        "detailed+budget",
        "overview+budget (follow next)",
        "ladder",
    ];
    let mut totals: Vec<(String, Outcome)> = strategies
        .iter()
        .map(|s| (s.to_string(), Outcome::default()))
        .collect();
    let mut cases_by_strategy = vec![(0usize, 0usize, 0usize); strategies.len()];

    println!("repeats per measurement: {repeats} (median, after one discarded warm-up)\n");
    for repo in spec["repos"].as_array().unwrap() {
        let name = repo["name"].as_str().unwrap();
        let root = repos_dir.join(repo["root"].as_str().unwrap());
        let registry = mct_mcp_server::registry::build_registry();
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        let t = Instant::now();
        index.reindex(&registry, false).unwrap();
        let index_ms = t.elapsed().as_millis();
        let server = MctServer::new(index, registry);
        let status = call(&server, "get_indexing_status", &json!({})).await;
        println!(
            "## {name} — {} @ {} ({} KB on disk, indexed in {index_ms} ms)\n",
            repo["language"].as_str().unwrap(),
            &repo["commit"].as_str().unwrap()[..10],
            dir_size_kb(&root)
        );
        println!("`{}`\n", status.lines().next().unwrap_or("").trim());
        println!("| task | kind | strategy | calls | ~tok chars/4 | ~tok Estimated | median µs | evidence | forbidden | signalled | silent miss | 1st hit line |");
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
        for t in repo["tasks"].as_array().unwrap() {
            let task = Task {
                id: t["id"].as_str().unwrap().into(),
                kind: t["kind"].as_str().unwrap().into(),
                tool: t["tool"].as_str().unwrap().into(),
                args: t["args"].clone(),
                evidence: arr(t, "evidence"),
                forbid: arr(t, "forbid"),
            };
            for (i, s) in strategies.iter().enumerate() {
                let o = measure(&server, &task, s, repeats).await;
                println!(
                    "| {} | {} | {s} | {} | {} | {} | {} | {}/{} | {} | {} | {} | {} |",
                    task.id,
                    task.kind,
                    o.calls,
                    o.chars4,
                    o.estimated,
                    o.median(),
                    o.found,
                    o.total,
                    if o.forbidden { "HIT" } else { "-" },
                    if o.signalled { "yes" } else { "no" },
                    if o.silent_miss() { "SILENT" } else { "-" },
                    o.first_hit_line.map_or("-".into(), |l| l.to_string()),
                );
                let (_, acc) = &mut totals[i];
                acc.calls += o.calls;
                acc.chars4 += o.chars4;
                acc.estimated += o.estimated;
                acc.micros.push(o.median());
                acc.found += o.found;
                acc.total += o.total;
                let c = &mut cases_by_strategy[i];
                c.0 += 1;
                c.1 += usize::from(o.satisfied());
                c.2 += usize::from(o.silent_miss());
            }
        }
        println!();
    }

    println!("## Totals over every task\n");
    println!("| strategy | tasks | satisfied | silent misses | calls | ~tok chars/4 | ~tok Estimated | mean median µs | evidence recall |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for (i, (s, o)) in totals.iter().enumerate() {
        let (n, sat, silent) = cases_by_strategy[i];
        let mean = o.micros.iter().sum::<u128>() / o.micros.len().max(1) as u128;
        println!(
            "| {s} | {n} | {sat} | {silent} | {} | {} | {} | {mean} | {:.2} |",
            o.calls,
            o.chars4,
            o.estimated,
            o.found as f64 / o.total.max(1) as f64
        );
    }
}
