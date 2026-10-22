// Example/bench code: a panic here means a broken benchmark precondition,
// not a path processing untrusted repo-input content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cost of review decisions for `find_dead_code`, measured in two parts that
//! are never merged:
//!
//! - **query response**: latency, bytes and estimated tokens of a default
//!   `find_dead_code` call (TOON and text), plus a SHA-256 of each response
//!   so two runs on different commits prove the output is byte-identical;
//! - **catalog**: the `find_dead_code` tool entry and the whole `tools/list`
//!   catalog, with their SHA-256, which every client that loads all schemas
//!   pays once per session.
//!
//! ```sh
//! cargo run --release -p mct-mcp-server --example review_overhead -- \
//!     --root <indexed project> [--iterations 50]
//! ```
//!
//! `--root` must already be indexed (`mct-cli --root <project> init`); the
//! index is opened read-only and never modified. Tokens are the project's
//! `EstimatedCounter` heuristic, not a model tokenizer.

use std::path::PathBuf;
use std::time::Instant;

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::{FindDeadCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

struct Options {
    root: PathBuf,
    iterations: usize,
}

fn options() -> Options {
    let mut root = None;
    let mut iterations = 50;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--iterations" => iterations = args.next().unwrap().parse().unwrap(),
            other => panic!("unknown argument {other}"),
        }
    }
    Options {
        root: root.expect("--root <indexed project> is required"),
        iterations,
    }
}

fn sha256(text: &str) -> String {
    hmac_sha256::Hash::hash(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// p50 and p95 of `samples` in microseconds.
fn percentiles(samples: &mut [u128]) -> (u128, u128) {
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    (at(0.50), at(0.95))
}

async fn dead_code_call(server: &MctServer, format: &str) -> String {
    let result = server
        .find_dead_code(Parameters(FindDeadCodeArgs {
            path: None,
            language: None,
            limit: None,
            offset: None,
            format: Some(format.to_string()),
        }))
        .await
        .unwrap();
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let options = options();
    let root = options.root.canonicalize().unwrap();
    let database = root.join(".mct-index/index.sqlite3");
    let index =
        Index::open_existing_read_only(&root, &database, ExcludeSet::for_project(&root)).unwrap();
    let files = index.file_hashes().unwrap().len();
    let candidates = mct_index::find_dead_code_candidates(&index, None, None)
        .unwrap()
        .len();
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());

    println!("# Review overhead\n");
    println!(
        "Project: {} indexed files, {candidates} dead-code candidates; {} iterations.\n",
        files, options.iterations
    );

    println!("## Query response: default `find_dead_code`\n");
    println!("| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |");
    println!("|---|---|---|---|---|---|");
    for format in ["toon", "text"] {
        let response = dead_code_call(&server, format).await;
        let mut samples = Vec::with_capacity(options.iterations);
        for _ in 0..options.iterations {
            let started = Instant::now();
            let again = dead_code_call(&server, format).await;
            samples.push(started.elapsed().as_micros());
            assert_eq!(again, response, "the response must be deterministic");
        }
        let (p50, p95) = percentiles(&mut samples);
        println!(
            "| {format} | {p50} | {p95} | {} | {} | `{}` |",
            response.len(),
            EstimatedCounter.count(&response).value,
            sha256(&response)
        );
    }

    println!("\n## Catalog\n");
    println!("| entry | bytes | EstimatedCounter | SHA-256 |");
    println!("|---|---|---|---|");
    let tools = server.tool_catalog();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "find_dead_code")
        .expect("find_dead_code is registered");
    for (name, json) in [
        (
            "find_dead_code".to_string(),
            serde_json::to_string(tool).unwrap(),
        ),
        (
            format!("all {} tools", tools.len()),
            serde_json::to_string(&tools).unwrap(),
        ),
    ] {
        println!(
            "| {name} | {} | {} | `{}` |",
            json.len(),
            EstimatedCounter.count(&json).value,
            sha256(&json)
        );
    }
}
