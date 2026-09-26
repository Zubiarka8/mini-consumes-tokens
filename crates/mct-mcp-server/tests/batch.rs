//! In-process tests for `batch` (issue #20): several read-only sub-queries
//! in one tool call. Same in-process pattern as `tests/tools.rs` — calls the
//! generated tool methods directly, bypassing the stdio/JSON-RPC transport.
//!
//! The token-reduction tests re-create the JSON-RPC exchange the transport
//! would carry (request + response envelopes around the real
//! `CallToolResult`s) for N separate calls versus one batch, and measure both
//! with the same approximate tokenizer `examples/format_benchmark.rs` uses.
//! Run with `--nocapture` to see the numbers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{BatchArgs, BatchQuery, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn content_of(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

async fn build_server(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn query(tool: &str, args: Value) -> BatchQuery {
    BatchQuery {
        tool: tool.to_string(),
        args: match args {
            Value::Object(map) => Some(map),
            Value::Null => None,
            other => panic!("batch args must be an object, got {other}"),
        },
    }
}

async fn run_batch(server: &MctServer, queries: &[(&str, Value)]) -> CallToolResult {
    let queries = queries
        .iter()
        .map(|(tool, args)| query(tool, args.clone()))
        .collect();
    server
        .batch(Parameters(BatchArgs { queries }))
        .await
        .unwrap()
}

/// A sub-query run on its own, as a direct `tools/call` would — routed
/// through a one-element batch's dispatcher so this file doesn't need a
/// per-tool match of its own, then unwrapped back to the tool's own text.
async fn run_single(server: &MctServer, tool: &str, args: &Value) -> CallToolResult {
    let text = content_of(&run_batch(server, &[(tool, args.clone())]).await);
    let body = text
        .split_once(&format!("[1] {tool}\n"))
        .map(|(_, body)| body.to_string())
        .unwrap_or_else(|| panic!("sub-query `{tool}` failed:\n{text}"));
    CallToolResult::success(vec![rmcp::model::ContentBlock::text(body)])
}

/// Same heuristic as `examples/format_benchmark.rs::approx_tokens`: a run of
/// alphanumerics/`_` is one token, every other non-whitespace char is one.
fn approx_tokens(s: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() {
                tokens += 1;
            }
        }
    }
    tokens
}

/// One `tools/call` round trip as it crosses the stdio transport.
fn exchange(id: usize, tool: &str, args: &Value, result: &CallToolResult) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args },
    });
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": serde_json::to_value(result).unwrap(),
    });
    format!("{request}\n{response}\n")
}

struct Measurement {
    separate_tokens: usize,
    batched_tokens: usize,
    separate_time: Duration,
    batched_time: Duration,
}

impl Measurement {
    fn reduction(&self) -> f64 {
        100.0 * (1.0 - self.batched_tokens as f64 / self.separate_tokens as f64)
    }
}

async fn measure(server: &MctServer, queries: &[(&str, Value)]) -> Measurement {
    // Warm SQLite's page cache first so neither side is timed cold.
    run_batch(server, queries).await;

    let start = Instant::now();
    let mut separate = String::new();
    for (i, (tool, args)) in queries.iter().enumerate() {
        let result = run_single(server, tool, args).await;
        separate.push_str(&exchange(i + 1, tool, args, &result));
    }
    let separate_time = start.elapsed();

    let start = Instant::now();
    let result = run_batch(server, queries).await;
    let batched_time = start.elapsed();
    let batch_args = json!({
        "queries": queries
            .iter()
            .map(|(tool, args)| json!({ "tool": tool, "args": args }))
            .collect::<Vec<_>>(),
    });
    let batched = exchange(1, "batch", &batch_args, &result);

    Measurement {
        separate_tokens: approx_tokens(&separate),
        batched_tokens: approx_tokens(&batched),
        separate_time,
        batched_time,
    }
}

fn report(scenario: &str, m: &Measurement) {
    println!(
        "{scenario}: separate ~{} tokens in {:?}, batch ~{} tokens in {:?} — {:.1}% fewer tokens",
        m.separate_tokens,
        m.separate_time,
        m.batched_tokens,
        m.batched_time,
        m.reduction()
    );
}

#[tokio::test]
async fn each_sub_result_is_the_tools_own_output_verbatim() {
    let server = build_server(&fixture("compute-app")).await;
    let direct = content_of(
        &server
            .find_callers(Parameters(
                serde_json::from_value(json!({ "function": "helper" })).unwrap(),
            ))
            .await
            .unwrap(),
    );
    let batched = content_of(
        &run_batch(
            &server,
            &[
                ("find_symbol", json!({ "name": "compute" })),
                ("find_callers", json!({ "function": "helper" })),
            ],
        )
        .await,
    );

    assert!(batched.starts_with("batch: 2 queries\n"), "{batched}");
    assert!(batched.contains("[1] find_symbol\n"), "{batched}");
    assert!(
        batched.contains(&format!("[2] find_callers\n{}", direct.trim_end())),
        "batched find_callers output differs from the direct call:\n{batched}\n---\n{direct}"
    );
}

#[tokio::test]
async fn a_failing_sub_query_is_reported_inline_and_the_rest_still_run() {
    let server = build_server(&fixture("compute-app")).await;
    let text = content_of(
        &run_batch(
            &server,
            &[
                ("find_symbol", json!({})),
                ("reindex", json!({ "force": true })),
                ("batch", json!({ "queries": [] })),
                ("not_a_tool", Value::Null),
                ("find_symbol", json!({ "name": "helper" })),
            ],
        )
        .await,
    );

    assert!(text.starts_with("batch: 5 queries, 4 failed\n"), "{text}");
    assert!(
        text.contains("[1] find_symbol error: invalid args for `find_symbol`"),
        "{text}"
    );
    assert!(text.contains("missing field `name`"), "{text}");
    assert!(text.contains("[2] reindex error:"), "{text}");
    assert!(text.contains("[3] batch error:"), "{text}");
    assert!(
        text.contains("[4] not_a_tool error: no tool named `not_a_tool`"),
        "{text}"
    );
    assert!(text.contains("[5] find_symbol\n"), "{text}");
    assert!(text.contains("helper"), "{text}");
}

#[tokio::test]
async fn an_argument_less_tool_runs_with_args_omitted() {
    let server = build_server(&fixture("compute-app")).await;
    let text = content_of(&run_batch(&server, &[("discover_tool_categories", Value::Null)]).await);
    assert!(text.starts_with("batch: 1 query\n"), "{text}");
    assert!(text.contains("[1] discover_tool_categories\n"), "{text}");
    assert!(text.contains("batch"), "{text}");
}

#[tokio::test]
async fn empty_and_oversized_batches_are_rejected() {
    let server = build_server(&fixture("compute-app")).await;
    let err = server
        .batch(Parameters(BatchArgs { queries: vec![] }))
        .await
        .unwrap_err();
    assert!(err.message.contains("must not be empty"));

    let queries = (0..26)
        .map(|_| query("find_symbol", json!({ "name": "compute" })))
        .collect();
    let err = server
        .batch(Parameters(BatchArgs { queries }))
        .await
        .unwrap_err();
    assert!(err.message.contains("at most 25"), "{}", err.message);
}

/// Acceptance criterion from issue #20: ≥20% fewer tokens for a batch than
/// for the same queries as separate calls. A typical "understand this
/// function" sweep over small results, where per-call envelopes dominate.
#[tokio::test]
async fn batching_an_exploration_sweep_cuts_tokens_by_at_least_20_percent() {
    let server = build_server(&fixture("compute-app")).await;
    let queries = [
        ("find_symbol", json!({ "name": "compute" })),
        ("find_callers", json!({ "function": "compute" })),
        ("find_calls", json!({ "function": "compute" })),
        ("find_symbol", json!({ "name": "helper" })),
        ("find_callers", json!({ "function": "helper" })),
        ("get_file_skeleton", json!({ "path": "src/lib.rs" })),
    ];
    let m = measure(&server, &queries).await;
    report("compute-app sweep", &m);
    assert!(
        m.reduction() >= 20.0,
        "only {:.1}% fewer tokens",
        m.reduction()
    );
}

/// Same criterion on a polyglot fixture with larger, mixed results
/// (`impact_analysis`, `list_symbols`, a skeleton), where the envelope is a
/// smaller share of each response.
#[tokio::test]
async fn batching_a_mixed_polyglot_sweep_cuts_tokens_by_at_least_20_percent() {
    let server = build_server(&fixture("omni-app")).await;
    let queries = [
        ("list_symbols", json!({ "path": "backend" })),
        ("find_symbol", json!({ "name": "NewLedger" })),
        ("find_callers", json!({ "function": "Post" })),
        ("find_calls", json!({ "function": "HandlePost" })),
        ("impact_analysis", json!({ "symbol": "Ledger" })),
        ("get_file_skeleton", json!({ "path": "backend/ledger.go" })),
    ];
    let m = measure(&server, &queries).await;
    report("omni-app sweep", &m);
    assert!(
        m.reduction() >= 20.0,
        "only {:.1}% fewer tokens",
        m.reduction()
    );
}

/// Provider prompt caching (Anthropic's, and the other major providers')
/// matches on an exact byte prefix, and the tool catalog sits at the front
/// of every request. It only caches if two server starts advertise a
/// byte-identical catalog — guard that, since a HashMap-ordered tool list or
/// a non-deterministic schema would silently break every cache hit.
#[tokio::test]
async fn the_advertised_tool_catalog_is_byte_stable_across_server_starts() {
    let first = build_server(&fixture("compute-app")).await;
    let second = build_server(&fixture("omni-app")).await;
    let first = serde_json::to_string(&first.tool_catalog()).unwrap();
    let second = serde_json::to_string(&second.tool_catalog()).unwrap();
    assert_eq!(first, second, "tool catalog differs between server starts");
}
