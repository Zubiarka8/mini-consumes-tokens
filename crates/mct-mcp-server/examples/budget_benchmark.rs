// Example/bench code: a panic here means a broken benchmark precondition,
// not a path processing untrusted repo-input content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `detail` / `max_tokens` benchmark over this repository's own code and ADRs.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example budget_benchmark --release -- <repo-root>
//! ```
//!
//! Copies `crates/mct-index/src` and `docs/01-architecture/adrs` of
//! `<repo-root>` into a temporary project, indexes it, and runs the same
//! tasks the old way (default arguments) and the new way. Reported per task:
//! MCP calls, response characters, tokens by two *separate* methods
//! (`chars/4` and `EstimatedCounter`, both heuristics, never exact), latency,
//! and whether the expected evidence (`path:line` or text) is in the answers.
//! Tokens billed by a provider and tool-catalog schema tokens are not
//! measured here.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Run {
    calls: usize,
    chars: usize,
    chars4: usize,
    estimated: usize,
    micros: u128,
    found: bool,
}

async fn run(server: &MctServer, calls: &[(&str, Value)], expect: &[&str]) -> Run {
    let start = Instant::now();
    let mut all = String::new();
    let mut chars = 0;
    let mut chars4 = 0;
    let mut estimated = 0;
    for (tool, args) in calls {
        let result = server
            .call_read_only_tool(tool, args.as_object().cloned())
            .await
            .unwrap();
        let text = result
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        if std::env::var_os("MCT_BENCH_DUMP").is_some() {
            eprintln!("--- {tool} {args}\n{text}");
        }
        chars += text.chars().count();
        chars4 += text.chars().count().div_ceil(4);
        estimated += EstimatedCounter.count(&text).value;
        all.push_str(&text);
    }
    Run {
        calls: calls.len(),
        chars,
        chars4,
        estimated,
        micros: start.elapsed().as_micros(),
        found: expect.iter().all(|needle| all.contains(needle)),
    }
}

fn row(task: &str, mode: &str, r: &Run) {
    println!(
        "| {task} | {mode} | {} | {} | {} | {} | {} | {} |",
        r.calls,
        r.chars,
        r.chars4,
        r.estimated,
        r.micros,
        if r.found { "yes" } else { "NO" }
    );
}

#[tokio::main]
async fn main() {
    let repo = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: budget_benchmark <repo-root>"),
    );
    let project = std::env::temp_dir().join(format!("mct-budget-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    copy_dir(
        &repo.join("crates/mct-index/src"),
        &project.join("crates/mct-index/src"),
    );
    copy_dir(
        &repo.join("docs/01-architecture/adrs"),
        &project.join("docs/adrs"),
    );

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&project, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let server = MctServer::new(index, registry);

    println!("| task | mode | calls | chars | ~tokens (chars/4) | ~tokens (EstimatedCounter) | latency µs | evidence found |");
    println!("|---|---|---|---|---|---|---|---|");

    let sem = "crates/mct-index/src/semantic.rs";
    let t = "locate a function";
    let base = json!({"query": "hybrid search", "cache": false});
    let r = run(
        &server,
        &[("search_symbols_like", base.clone())],
        &["hybrid_search"],
    )
    .await;
    row(t, "default", &r);
    let r = run(
        &server,
        &[(
            "search_symbols_like",
            json!({"query": "hybrid search", "cache": false, "detail": "overview"}),
        )],
        &["hybrid_search"],
    )
    .await;
    row(t, "overview", &r);

    let t = "context pack of a function";
    for (mode, extra) in [
        ("default", json!({})),
        ("detailed", json!({"detail": "detailed"})),
        ("summary", json!({"detail": "summary"})),
        ("overview", json!({"detail": "overview"})),
        ("detailed, max_tokens=600", json!({"max_tokens": 600})),
    ] {
        let mut args = json!({"symbol": "hybrid_search", "path": sem});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let r = run(&server, &[("build_context_pack", args)], &["semantic.rs:L"]).await;
        row(t, mode, &r);
    }

    let t = "markdown section (ADR)";
    for (mode, extra) in [
        ("default", json!({})),
        ("summary", json!({"detail": "summary"})),
        ("overview", json!({"detail": "overview"})),
    ] {
        let mut args =
            json!({"symbol": "Decision", "path": "docs/adrs/013-obsidian-vault-location.md"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let r = run(
            &server,
            &[("build_context_pack", args)],
            &["013-obsidian-vault-location.md:L"],
        )
        .await;
        row(t, mode, &r);
    }

    let t = "find a decision in the ADRs";
    let r = run(
        &server,
        &[(
            "search_symbols_like",
            json!({"query": "alternatives considered", "cache": false, "snippet_lines": 8}),
        )],
        &["013-obsidian"],
    )
    .await;
    row(t, "default (snippet_lines=8)", &r);
    let r = run(&server, &[("search_symbols_like", json!({"query": "alternatives considered", "cache": false, "detail": "overview", "max_tokens": 300}))], &["013-obsidian"]).await;
    row(t, "overview, max_tokens=300", &r);

    // Escalation: look cheap first, then ask for the full pack of the one hit.
    let t = "escalate overview -> detailed";
    let one = run(
        &server,
        &[(
            "build_context_pack",
            json!({"symbol": "hybrid_search", "path": sem}),
        )],
        &["semantic.rs:L"],
    )
    .await;
    row(t, "one default call", &one);
    let two = run(
        &server,
        &[
            (
                "build_context_pack",
                json!({"symbol": "hybrid_search", "path": sem, "detail": "overview"}),
            ),
            (
                "build_context_pack",
                json!({"symbol": "hybrid_search", "path": sem, "detail": "detailed"}),
            ),
        ],
        &["semantic.rs:L"],
    )
    .await;
    row(t, "overview then detailed", &two);

    // Scenarios added after the first run (no "before" row exists for them):
    // the same tasks resolved by following the continuation / in one call.
    let t = "find a decision in the ADRs (follow-up)";
    let (r, pages) = follow(
        &server,
        json!({"query": "alternatives considered", "cache": false, "detail": "overview", "max_tokens": 300}),
        &["013-obsidian"],
    )
    .await;
    row(
        t,
        &format!("overview, max_tokens=300, {pages} page(s) via next: offset"),
        &r,
    );
    let t = "escalate overview -> detailed (single call)";
    let r = run(
        &server,
        &[(
            "build_context_pack",
            json!({"symbol": "hybrid_search", "path": sem, "detail": "detailed", "max_tokens": 1200}),
        )],
        &["semantic.rs:L"],
    )
    .await;
    row(t, "detailed, max_tokens=1200", &r);

    std::fs::remove_dir_all(&project).unwrap();
}

/// Calls `search_symbols_like` and follows `next: offset=N` until every
/// needle has been seen or the results end; returns the summed run and the
/// number of pages.
async fn follow(server: &MctServer, base: Value, expect: &[&str]) -> (Run, usize) {
    let start = Instant::now();
    let mut total = Run {
        calls: 0,
        chars: 0,
        chars4: 0,
        estimated: 0,
        micros: 0,
        found: false,
    };
    let mut seen = String::new();
    let mut offset = 0usize;
    for _ in 0..50 {
        let mut args = base.clone();
        args["offset"] = json!(offset);
        let result = server
            .call_read_only_tool("search_symbols_like", args.as_object().cloned())
            .await
            .unwrap();
        let text = result
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        total.calls += 1;
        total.chars += text.chars().count();
        total.chars4 += text.chars().count().div_ceil(4);
        total.estimated += EstimatedCounter.count(&text).value;
        seen.push_str(&text);
        if expect.iter().all(|n| seen.contains(n)) {
            break;
        }
        let next = text
            .split("next: offset=")
            .nth(1)
            .and_then(|r| r.split(['\n', ';']).next())
            .and_then(|n| n.trim().parse::<usize>().ok());
        match next {
            Some(n) => offset = n,
            None => break,
        }
    }
    total.micros = start.elapsed().as_micros();
    total.found = expect.iter().all(|n| seen.contains(n));
    let pages = total.calls;
    (total, pages)
}
