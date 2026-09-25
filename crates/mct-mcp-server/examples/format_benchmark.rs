// Example/bench code: a panic here means a broken benchmark precondition
// (e.g. the local synthetic struct failing to serialize), not a path
// processing untrusted repo-input content — see the same allow's rationale
// in crates/mct-mcp-server/tests/*.rs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! TOON-vs-JSON size benchmark for this server's tool responses.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p mct-mcp-server --example format_benchmark --release
//! ```
//!
//! Measures byte size and an approximate token count (a `bytes / 4`-ish
//! heuristic scan — no tokenizer dependency is added just for this, per the
//! workspace's no-unnecessary-dependencies rule) for three representative
//! payload shapes against a JSON baseline. The JSON baseline is a local
//! `#[derive(Serialize)]` struct mirroring each row's fields — this server
//! itself never serializes to JSON (see `crate::format`'s module doc), so
//! this is what "the JSON this data would cost if serialized naively" looks
//! like, which is the comparison the project's TOON-adoption decision is
//! actually about.

use mct_index::{RelationHit, SymbolListEntry};
use mct_mcp_server::{format, toon::encode_table};
use serde::Serialize;

#[derive(Serialize)]
struct SymbolRow {
    path: String,
    line: u32,
    column: u32,
    language: String,
    kind: String,
    name: String,
    parent: String,
}

#[derive(Serialize)]
struct RelationRow {
    path: String,
    line: u32,
    column: u32,
    language: String,
    from: String,
    kind: String,
    to: String,
    depth: u32,
}

/// A crude token-count approximation: every run of alphanumeric/`_`
/// characters counts as one token, and every other non-whitespace character
/// (punctuation — `{`, `"`, `,`, `:`, `[`, `]`, `-`, `>` ...) counts as its
/// own token. This mirrors how BPE tokenizers tend to split JSON/TOON
/// punctuation into single-character tokens while keeping identifiers
/// mostly whole, without pulling in a real tokenizer dependency.
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

fn report(scenario: &str, toon: &str, json: &str) {
    let toon_bytes = toon.len();
    let json_bytes = json.len();
    let toon_tokens = approx_tokens(toon);
    let json_tokens = approx_tokens(json);
    let byte_reduction = 100.0 * (1.0 - toon_bytes as f64 / json_bytes as f64);
    let token_reduction = 100.0 * (1.0 - toon_tokens as f64 / json_tokens as f64);

    println!("== {scenario} ==");
    println!("  JSON: {json_bytes:>6} bytes, ~{json_tokens:>5} tokens");
    println!("  TOON: {toon_bytes:>6} bytes, ~{toon_tokens:>5} tokens");
    println!("  reduction vs JSON: {byte_reduction:.1}% bytes, {token_reduction:.1}% tokens");
}

/// This server never actually emitted JSON (see `format`'s module doc) —
/// its existing plain-text renderer is the real baseline TOON has to beat
/// for this project to adopt it. Printed alongside the JSON comparison
/// above so the PR reports both: the headline number the task asked for
/// (vs. naive JSON, a fair proxy for "a typical MCP server's tool output"),
/// and the honest one (vs. what this server already ships).
fn report_vs_existing_text(scenario: &str, toon: &str, text: &str) {
    let toon_bytes = toon.len();
    let text_bytes = text.len();
    let toon_tokens = approx_tokens(toon);
    let text_tokens = approx_tokens(text);
    let byte_reduction = 100.0 * (1.0 - toon_bytes as f64 / text_bytes as f64);
    let token_reduction = 100.0 * (1.0 - toon_tokens as f64 / text_tokens as f64);
    println!("  existing text: {text_bytes:>6} bytes, ~{text_tokens:>5} tokens");
    println!("  reduction vs existing text ({scenario}): {byte_reduction:.1}% bytes, {token_reduction:.1}% tokens");
    println!();
}

/// `list_symbols`-shaped: symbol rows spanning a realistic directory listing
/// (mirrors what `list_symbols` on a mid-size crate's `src/` returns).
fn symbol_rows(count: usize) -> Vec<SymbolRow> {
    let kinds = ["function", "struct", "method", "enum", "trait"];
    (0..count)
        .map(|i| SymbolRow {
            path: format!("crates/mct-lang-go/src/parser_{:02}.rs", i % 12),
            line: 10 + (i as u32) * 7,
            column: 1,
            language: "rust".to_string(),
            kind: kinds[i % kinds.len()].to_string(),
            name: format!("handle_node_kind_{i:03}"),
            parent: if i % 3 == 0 {
                String::new()
            } else {
                format!("Parser{}", i % 5)
            },
        })
        .collect()
}

/// `find_references`/`find_calls`/`find_callers`-shaped.
fn relation_rows(count: usize) -> Vec<RelationRow> {
    (0..count)
        .map(|i| RelationRow {
            path: format!("crates/mct-index/src/query_{:02}.rs", i % 9),
            line: 20 + (i as u32) * 3,
            column: 5,
            language: "rust".to_string(),
            from: format!("caller_function_{i:03}"),
            kind: "calls".to_string(),
            to: "find_symbol_matching_scoped".to_string(),
            depth: 1,
        })
        .collect()
}

fn main() {
    // Scenario 1: `list_symbols` on a real directory — ~80 symbols is a
    // realistic mid-size crate's `src/`.
    let symbols = symbol_rows(80);
    let symbol_json: Vec<String> = symbols
        .iter()
        .map(|r| serde_json::to_string(r).expect("local struct always serializes"))
        .collect();
    let symbol_json_array = format!("[{}]", symbol_json.join(","));
    let symbol_rows_owned: Vec<Vec<String>> = symbols
        .iter()
        .map(|r| {
            vec![
                r.path.clone(),
                r.line.to_string(),
                r.column.to_string(),
                r.language.clone(),
                r.kind.clone(),
                r.name.clone(),
                r.parent.clone(),
            ]
        })
        .collect();
    let symbol_toon = encode_table(
        "symbols",
        &["path", "line", "column", "language", "kind", "name", "parent"],
        &symbol_rows_owned,
    );
    report("list_symbols (80 rows)", &symbol_toon, &symbol_json_array);
    let symbol_entries: Vec<SymbolListEntry> = symbols
        .iter()
        .map(|r| SymbolListEntry {
            name: r.name.clone(),
            kind: r.kind.clone(),
            language: r.language.clone(),
            relative_path: r.path.clone(),
            line: r.line,
            end_line: Some(r.line + 6),
            parent: (!r.parent.is_empty()).then(|| r.parent.clone()),
            level: None,
        })
        .collect();
    let symbol_text = format::list_symbols("crates/mct-lang-go/src", false, &symbol_entries, 200);
    report_vs_existing_text("list_symbols", &symbol_toon, &symbol_text);

    // Scenario 2: `find_references` with several dozen hits.
    let relations = relation_rows(40);
    let relation_json: Vec<String> = relations
        .iter()
        .map(|r| serde_json::to_string(r).expect("local struct always serializes"))
        .collect();
    let relation_json_array = format!("[{}]", relation_json.join(","));
    let relation_rows_owned: Vec<Vec<String>> = relations
        .iter()
        .map(|r| {
            vec![
                r.path.clone(),
                r.line.to_string(),
                r.column.to_string(),
                r.language.clone(),
                r.from.clone(),
                r.kind.clone(),
                r.to.clone(),
                r.depth.to_string(),
            ]
        })
        .collect();
    let relation_toon = encode_table(
        "relations",
        &["path", "line", "column", "language", "from", "kind", "to", "depth"],
        &relation_rows_owned,
    );
    report("find_references (40 rows)", &relation_toon, &relation_json_array);
    let relation_hits: Vec<RelationHit> = relations
        .iter()
        .map(|r| RelationHit {
            kind: r.kind.clone(),
            from_symbol: r.from.clone(),
            to_name: r.to.clone(),
            language: r.language.clone(),
            relative_path: r.path.clone(),
            line: r.line,
            column: r.column,
            depth: r.depth,
        })
        .collect();
    let relation_text = format::relation_hits("find_symbol_matching_scoped", "reference(s)", &relation_hits, 0, 200);
    report_vs_existing_text("find_references", &relation_toon, &relation_text);

    // Scenario 3: `find_symbol` with a handful of hits — the small-N case,
    // where TOON's fixed header overhead matters most.
    let small = symbol_rows(5);
    let small_json: Vec<String> = small
        .iter()
        .map(|r| serde_json::to_string(r).expect("local struct always serializes"))
        .collect();
    let small_json_array = format!("[{}]", small_json.join(","));
    let small_rows_owned: Vec<Vec<String>> = small
        .iter()
        .map(|r| {
            vec![
                r.path.clone(),
                r.line.to_string(),
                r.column.to_string(),
                r.language.clone(),
                r.kind.clone(),
                r.name.clone(),
                r.parent.clone(),
            ]
        })
        .collect();
    let small_toon = encode_table(
        "symbols",
        &["path", "line", "column", "language", "kind", "name", "parent"],
        &small_rows_owned,
    );
    report("find_symbol (5 rows)", &small_toon, &small_json_array);
    let small_entries: Vec<SymbolListEntry> = small
        .iter()
        .map(|r| SymbolListEntry {
            name: r.name.clone(),
            kind: r.kind.clone(),
            language: r.language.clone(),
            relative_path: r.path.clone(),
            line: r.line,
            end_line: Some(r.line + 6),
            parent: (!r.parent.is_empty()).then(|| r.parent.clone()),
            level: None,
        })
        .collect();
    // `find_symbol`'s text renderer is `symbol_hits`, not `list_symbols` —
    // same per-row shape (path/line/column/language/kind/name/parent), just
    // framed as "definitions of `name`" rather than "symbols under `path`".
    let small_hits: Vec<mct_index::SymbolHit> = small_entries
        .iter()
        .map(|e| mct_index::SymbolHit {
            name: e.name.clone(),
            kind: e.kind.clone(),
            language: e.language.clone(),
            relative_path: e.relative_path.clone(),
            line: e.line,
            column: 1,
            parent: e.parent.clone(),
            end_line: e.end_line,
            level: None,
        })
        .collect();
    let small_text = format::symbol_hits("handle_node_kind_000", &small_hits, 200);
    report_vs_existing_text("find_symbol", &small_toon, &small_text);
}
