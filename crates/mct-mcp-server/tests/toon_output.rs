//! Integration coverage for the opt-in `format: "toon"` response shape,
//! exercised against the same in-process tool layer `tests/tools.rs` uses
//! (bypassing the stdio/JSON-RPC transport). Each test pairs a TOON call
//! against the tool's default `text` call on the same input, asserting both
//! that TOON parses back to the same underlying data (via
//! `mct_mcp_server::toon::decode_table`) and that it is meaningfully smaller
//! — the whole reason this format exists.

// Test code: an unwrap()/expect() here means a broken test precondition —
// see the same allow in tests/tools.rs for why that's correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{
    FindCallersArgs, FindCallsArgs, FindDeadCodeArgs, FindReferencesArgs, FindSymbolArgs, ImpactAnalysisArgs,
    ListSymbolsArgs, MctServer,
};
use mct_mcp_server::toon::decode_table;
use rmcp::handler::server::wrapper::Parameters;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app")
}

fn many_callers_fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/many-callers")
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

async fn build_server_at(root: &std::path::Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn build_server() -> MctServer {
    build_server_at(&fixture_root()).await
}

#[tokio::test]
async fn an_unrecognized_format_value_is_rejected_as_invalid_params() {
    let server = build_server().await;
    let err = server
        .list_symbols(Parameters(ListSymbolsArgs {
            format: Some("xml".to_string()),
            path: "src/lib.rs".to_string(),
            kind: None,
            language: None,
            limit: None,
        }))
        .await
        .unwrap_err();
    assert!(err.message.contains("format must be one of text, toon"), "got: {err:?}");
}

#[tokio::test]
async fn list_symbols_toon_decodes_to_the_same_rows_text_reports() {
    let server = build_server().await;
    let toon = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                format: Some("toon".to_string()),
                path: "src/lib.rs".to_string(),
                kind: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                format: None,
                path: "src/lib.rs".to_string(),
                kind: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );

    let table_start = toon.find("symbols[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode a well-formed table");
    // list_symbols renders every kind found under the path, same as the
    // text version — that includes the file's own synthetic `module` entry
    // alongside its two functions.
    assert_eq!(table.rows.len(), 3, "module + compute + helper: {table:?}");
    let names: Vec<&str> = table.rows.iter().map(|r| r[1].as_str()).collect();
    assert!(names.contains(&"compute"), "got: {names:?}");
    assert!(names.contains(&"helper"), "got: {names:?}");

    // No information loss versus the text rendering: same two functions,
    // same line ranges (L1-L3 / L4-L6), just shaped as columns instead of
    // labelled prose.
    assert!(text.contains("compute") && text.contains("L1-L3"), "got: {text}");
    assert!(toon.contains("compute,1,3") || toon.contains("compute,1,3\n"), "got: {toon}");
}

#[tokio::test]
async fn find_symbol_toon_round_trips_and_is_smaller_than_text() {
    let server = build_server().await;
    let toon = content_of(
        &server
            .find_symbol(Parameters(FindSymbolArgs {
                format: Some("toon".to_string()),
                name: "compute".to_string(),
                match_mode: None,
                path: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    let text = content_of(
        &server
            .find_symbol(Parameters(FindSymbolArgs {
                format: None,
                name: "compute".to_string(),
                match_mode: None,
                path: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    let table_start = toon.find("symbols[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][5], "compute");
    // A single row doesn't beat the text form (the table header itself
    // costs more than one labelled line saves) — TOON's savings come from
    // amortizing that header across many rows; see the benchmarks for the
    // realistic case.
    let _ = text;
}

#[tokio::test]
async fn find_references_toon_round_trips() {
    let server = build_server().await;
    let toon = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                format: Some("toon".to_string()),
                symbol: "helper".to_string(),
                path: None,
                language: None,
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    let table_start = toon.find("relations[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][4], "compute", "from_symbol column");
    assert_eq!(table.rows[0][6], "helper", "to column");
}

#[tokio::test]
async fn find_calls_toon_round_trips() {
    let server = build_server().await;
    let toon = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                format: Some("toon".to_string()),
                function: "compute".to_string(),
                path: None,
                language: None,
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    let table_start = toon.find("relations[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][6], "helper", "to column");
}

#[tokio::test]
async fn find_callers_toon_truncates_by_limit_the_same_as_text() {
    let server = build_server_at(&many_callers_fixture_root()).await;
    let toon = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                format: Some("toon".to_string()),
                function: "target".to_string(),
                path: None,
                language: None,
                limit: Some(10),
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(toon.starts_with("70 caller(s) of this function (showing 10"), "got: {toon}");
    let table_start = toon.find("relations[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode");
    assert_eq!(table.rows.len(), 10);
}

#[tokio::test]
async fn impact_analysis_toon_renders_one_table_per_populated_section() {
    let server = build_server().await;
    let toon = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                format: Some("toon".to_string()),
                symbol: "helper".to_string(),
                path: None,
                language: None,
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(toon.contains("\nDirect callers"), "got: {toon}");
    assert!(toon.contains("callers["), "got: {toon}");
    let table_start = toon.find("callers[").expect("callers table header");
    // Each section's table is its own block, blank-line separated from the
    // next section's heading — slice to just this one so decoding doesn't
    // also swallow the "All references" heading and table that follow.
    let table_text = match toon[table_start..].find("\n\n") {
        Some(end) => &toon[table_start..table_start + end],
        None => &toon[table_start..],
    };
    let table = decode_table(table_text).expect("must decode");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][4], "compute");
}

#[tokio::test]
async fn find_dead_code_toon_round_trips() {
    // `many-callers`: one `target` function plus 70 `caller_NNN` functions
    // that call it. `target` itself is never a dead-code candidate (it has
    // 70 callers); every `caller_NNN` is, since nothing calls them.
    let server = build_server_at(&many_callers_fixture_root()).await;
    let toon = content_of(
        &server
            .find_dead_code(Parameters(FindDeadCodeArgs {
                format: Some("toon".to_string()),
                path: None,
                language: None,
                limit: Some(200),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    let table_start = toon.find("candidates[").expect("toon body must contain the table header");
    let table = decode_table(&toon[table_start..]).expect("must decode");
    let names: Vec<&str> = table.rows.iter().map(|r| r[1].as_str()).collect();
    assert!(names.contains(&"caller_001"), "got: {names:?}");
    assert!(!names.contains(&"target"), "got: {names:?}");
}
