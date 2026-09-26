//! In-process tests of the `search_symbols` tool (issue #58): ranked lexical
//! search over split symbol names, its `limit`/`offset` paging and clamping,
//! and the optional `snippet_lines` source excerpt.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{FindSymbolArgs, MctServer, SearchSymbolsArgs};
use rmcp::handler::server::wrapper::Parameters;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

async fn build_server_at(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn args(query: &str) -> SearchSymbolsArgs {
    SearchSymbolsArgs {
        query: query.to_string(),
        path: None,
        language: None,
        limit: None,
        offset: None,
        snippet_lines: None,
        format: None,
    }
}

async fn search(server: &MctServer, args: SearchSymbolsArgs) -> String {
    content_of(&server.search_symbols(Parameters(args)).await.unwrap())
}

#[tokio::test]
async fn split_words_find_what_find_symbol_misses_across_naming_styles() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let fuzzy = content_of(
        &server
            .find_symbol(Parameters(FindSymbolArgs {
                format: None,
                name: "create invoice".to_string(),
                match_mode: Some("fuzzy".to_string()),
                path: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(fuzzy.contains("No symbol named"), "got: {fuzzy}");

    let text = search(&server, args("create invoice")).await;
    assert!(text.contains("createInvoice"), "got: {text}");
    assert!(text.contains("HandleCreateInvoice"), "got: {text}");
}

#[tokio::test]
async fn the_exact_name_is_the_first_hit() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let text = search(&server, args("createInvoice")).await;
    let first = text.lines().nth(1).unwrap_or_default();
    assert!(
        first.contains(" createInvoice"),
        "first hit was: {first}\nfull: {text}"
    );
}

#[tokio::test]
async fn default_limit_is_ten_and_offset_pages() {
    let server = build_server_at(&fixture("many-callers")).await;
    let text = search(&server, args("caller")).await;
    assert!(text.starts_with("70 match(es)"), "got: {text}");
    assert_eq!(
        text.lines().filter(|l| l.contains(" caller_")).count(),
        10,
        "got: {text}"
    );

    let mut paged = args("caller");
    paged.offset = Some(65);
    let text = search(&server, paged).await;
    assert_eq!(
        text.lines().filter(|l| l.contains(" caller_")).count(),
        5,
        "got: {text}"
    );
    assert!(text.contains("starting at offset 65"), "got: {text}");
}

#[tokio::test]
async fn limit_is_clamped() {
    let server = build_server_at(&fixture("many-callers")).await;
    let mut huge = args("caller");
    huge.limit = Some(100_000);
    let text = search(&server, huge).await;
    assert_eq!(text.lines().filter(|l| l.contains(" caller_")).count(), 70);

    let mut zero = args("caller");
    zero.limit = Some(0);
    let text = search(&server, zero).await;
    assert_eq!(
        text.lines().filter(|l| l.contains(" caller_")).count(),
        1,
        "got: {text}"
    );
}

#[tokio::test]
async fn snippets_are_off_by_default_and_bounded_by_the_symbol_when_requested() {
    let server = build_server_at(&fixture("compute-app")).await;
    let plain = search(&server, args("compute")).await;
    assert!(!plain.contains("| "), "got: {plain}");

    let mut with_snippet = args("helper");
    with_snippet.snippet_lines = Some(50);
    let text = search(&server, with_snippet).await;
    assert!(text.contains("4| "), "got: {text}");
    // `helper` spans L4-L6: the snippet must stop there even though 50
    // lines were allowed.
    assert!(!text.contains("7| "), "got: {text}");
}

#[tokio::test]
async fn toon_output_carries_the_same_hits() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let mut toon = args("create invoice");
    toon.format = Some("toon".to_string());
    let text = search(&server, toon).await;
    assert!(text.contains("matches[2]{"), "got: {text}");
}

#[tokio::test]
async fn empty_and_unmatchable_queries_are_handled_cleanly() {
    let server = build_server_at(&fixture("compute-app")).await;
    assert!(server
        .search_symbols(Parameters(args("   ")))
        .await
        .is_err());
    let text = search(&server, args("\"*()")).await;
    assert!(text.contains("No symbol matches"), "got: {text}");
}
