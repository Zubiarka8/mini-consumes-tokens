//! In-process tests for `build_context_pack` (issue #27): one call bundling a
//! symbol's definition, doc comment, capped source, callers/callees,
//! dependencies and related tests, each related symbol listed once. Same
//! in-process pattern as `tests/batch.rs`.
//!
//! The token-reduction test re-creates the JSON-RPC exchanges an agent would
//! otherwise make by hand — the individual lookups plus reading every file
//! they point at — and compares them to the one pack, measured with the same
//! approximate tokenizer `examples/format_benchmark.rs` uses. Run with
//! `--nocapture` to see the numbers and the pack itself.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn content_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn build_server(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn call(server: &MctServer, tool: &str, args: &Value) -> CallToolResult {
    let args = match args {
        Value::Object(map) => Some(map.clone()),
        _ => None,
    };
    server
        .call_read_only_tool(tool, args)
        .await
        .unwrap_or_else(|err| panic!("`{tool}` failed: {}", err.message))
}

async fn pack(server: &MctServer, args: Value) -> String {
    content_of(&call(server, "build_context_pack", &args).await)
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
fn exchange(id: usize, tool: &str, args: &Value, text: &str) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args },
    });
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "content": [{ "type": "text", "text": text }] },
    });
    format!("{request}\n{response}\n")
}

/// A whole file as a file-reading tool hands it to an agent: every line
/// prefixed with its number.
fn numbered(source: &str) -> String {
    source
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{line}\n", i + 1))
        .collect()
}

#[tokio::test]
async fn packs_definition_doc_comment_and_every_related_symbol_once() {
    let server = build_server(&fixture("context-pack-app")).await;
    let text = pack(&server, json!({ "symbol": "place_order" })).await;
    println!("{text}");

    assert!(
        text.starts_with("Context pack for `place_order` (depth 1): 1 definition(s)"),
        "{text}"
    );
    // The definition, with the doc comment above it and its body.
    assert!(text.contains("src/orders.rs:L35-L49 [rust] function place_order"), "{text}");
    assert!(text.contains("31| /// Places `order`: checks it is well formed"), "{text}");
    assert!(text.contains("40|     validate_order(order)?;"), "{text}");
    // Only the definition, never the rest of its file.
    assert!(!text.contains("fn cancel_order"), "{text}");

    // Callees resolved to their definitions, with a signature.
    assert!(text.contains("callee validate_order  src/orders.rs:L52-L62 function"), "{text}");
    assert!(
        text.contains("| pub fn apply_discount(cents: u64, coupon: Option<&str>) -> u64 {"),
        "{text}"
    );
    // A caller that is also a test is one row with both roles.
    assert!(text.contains("caller,test test_place_order_stores_the_discounted_total"), "{text}");
    assert!(text.contains("caller place_orders  src/orders.rs:L82-L91 function"), "{text}");
    // A file-level `use` is named once, not listed as a whole-file row.
    assert!(
        text.contains("Referenced at file level (use/import) by: tests/orders.rs"),
        "{text}"
    );
    assert!(!text.contains(" module  |"), "{text}");
    // Names the index can't resolve are summarised, not listed as rows.
    assert!(
        text.contains("Called but not defined in the index (std/third-party): Ok, as_deref, len"),
        "{text}"
    );

    // Deduplication: every related row names a distinct symbol.
    let rows: Vec<&str> = text
        .lines()
        .skip_while(|l| !l.starts_with("Related symbols"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .collect();
    let names: BTreeSet<&str> = rows
        .iter()
        .filter_map(|row| row.split_whitespace().nth(1))
        .collect();
    assert_eq!(names.len(), rows.len(), "a symbol is listed twice:\n{text}");
}

/// Acceptance criterion from issue #27: ≥60% fewer tokens than gathering the
/// same context by hand. The manual route is what an agent does today to get
/// ready to change `place_order`: locate it, list its callees, its callers
/// and its likely tests, then read every file those point at to see the code
/// and signatures — each as its own tool round trip.
#[tokio::test]
async fn one_pack_cuts_tokens_by_at_least_60_percent_versus_manual_aggregation() {
    let root = fixture("context-pack-app");
    let server = build_server(&root).await;

    let lookups = [
        ("find_symbol", json!({ "name": "place_order" })),
        ("find_calls", json!({ "function": "place_order" })),
        ("find_callers", json!({ "function": "place_order" })),
        ("impact_analysis", json!({ "symbol": "place_order" })),
    ];
    // Every file holding the definition or a related symbol in the pack.
    let files = ["src/orders.rs", "src/pricing.rs", "src/store.rs", "tests/orders.rs"];

    let mut tools_only = String::new();
    for (i, (tool, args)) in lookups.iter().enumerate() {
        let text = content_of(&call(&server, tool, args).await);
        tools_only.push_str(&exchange(i + 1, tool, args, &text));
    }
    let mut manual = tools_only.clone();
    for (i, file) in files.iter().enumerate() {
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        let args = json!({ "file_path": file });
        manual.push_str(&exchange(lookups.len() + i + 1, "Read", &args, &numbered(&source)));
    }

    let args = json!({ "symbol": "place_order" });
    let packed = exchange(1, "build_context_pack", &args, &pack(&server, args.clone()).await);

    let (manual, tools_only, packed) =
        (approx_tokens(&manual), approx_tokens(&tools_only), approx_tokens(&packed));
    let reduction = 100.0 * (1.0 - packed as f64 / manual as f64);
    println!(
        "place_order: manual ~{manual} tokens (lookups alone ~{tools_only}, no source), \
         build_context_pack ~{packed} tokens — {reduction:.1}% fewer"
    );
    assert!(reduction >= 60.0, "only {reduction:.1}% fewer tokens");
}

#[tokio::test]
async fn toon_renders_the_related_symbols_as_one_table() {
    let server = build_server(&fixture("context-pack-app")).await;
    let text = pack(&server, json!({ "symbol": "place_order", "format": "toon" })).await;
    assert!(
        text.contains("related[8]{name,roles,hop,path,lines,kind,signature}:"),
        "{text}"
    );
    assert!(
        text.contains("  test_place_order_stores_the_discounted_total,caller test,1,tests/orders.rs,L18-L25,function,"),
        "{text}"
    );
    // The definition's source is not tabular and stays as numbered lines.
    assert!(text.contains("40|     validate_order(order)?;"), "{text}");
}

#[tokio::test]
async fn depth_two_adds_callees_of_callees_tagged_with_their_hop() {
    let server = build_server(&fixture("context-pack-app")).await;
    let direct = pack(&server, json!({ "symbol": "place_order" })).await;
    let deep = pack(&server, json!({ "symbol": "place_order", "depth": 2 })).await;
    println!("{deep}");
    assert!(!direct.contains("callee reserve "), "{direct}");
    assert!(
        deep.contains("callee reserve [depth 2]  src/inventory.rs:L29-L37 method"),
        "{deep}"
    );
    assert!(deep.starts_with("Context pack for `place_order` (depth 2)"), "{deep}");
}

#[tokio::test]
async fn an_ambiguous_name_packs_each_definition_and_path_narrows_it() {
    let server = build_server(&fixture("context-pack-app")).await;
    let both = pack(&server, json!({ "symbol": "new" })).await;
    assert!(both.contains("2 definition(s)"), "{both}");
    assert!(both.contains("src/inventory.rs:L13-L15 [rust] method new"), "{both}");
    assert!(both.contains("src/store.rs:L22-L24 [rust] method new"), "{both}");

    let one = pack(&server, json!({ "symbol": "new", "path": "src/store.rs" })).await;
    assert!(one.contains("1 definition(s)"), "{one}");
    assert!(!one.contains("src/inventory.rs:L13-L15 [rust] method new"), "{one}");
}

#[tokio::test]
async fn source_lines_caps_the_body_and_says_how_much_is_hidden() {
    let server = build_server(&fixture("context-pack-app")).await;
    let text = pack(&server, json!({ "symbol": "place_order", "source_lines": 1 })).await;
    assert!(text.contains("35| pub fn place_order("), "{text}");
    assert!(!text.contains("36|"), "{text}");
    assert!(text.contains("… 14 more line(s) — raise `source_lines`"), "{text}");
}

#[tokio::test]
async fn an_unknown_symbol_says_so_and_an_empty_name_is_rejected() {
    let server = build_server(&fixture("context-pack-app")).await;
    let text = pack(&server, json!({ "symbol": "no_such_symbol" })).await;
    assert!(text.starts_with("No symbol named `no_such_symbol`"), "{text}");

    let err = server
        .call_read_only_tool(
            "build_context_pack",
            json!({ "symbol": "  " }).as_object().cloned(),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("must not be empty"), "{}", err.message);
}
