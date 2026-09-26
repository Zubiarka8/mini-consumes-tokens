//! In-process tests of the `hybrid_search` tool (issue #61) that need no
//! embedding model: the lexical-only paths (`alpha = 0`, and a build without
//! the `semantic` feature) must return exactly `search_symbols`' ranking,
//! say which mode ran, and clamp/page like `search_symbols`. Retrieval
//! quality with the real model is measured by `hybrid_benchmark.rs`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{HybridSearchArgs, MctServer, SearchSymbolsArgs};
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

fn args(query: &str, alpha: Option<f64>) -> HybridSearchArgs {
    HybridSearchArgs {
        query: query.to_string(),
        alpha,
        path: None,
        language: None,
        top_k: None,
        offset: None,
        snippet_lines: None,
        format: None,
    }
}

async fn hybrid(server: &MctServer, args: HybridSearchArgs) -> String {
    content_of(&server.hybrid_search(Parameters(args)).await.unwrap())
}

async fn lexical(server: &MctServer, query: &str) -> String {
    content_of(
        &server
            .search_symbols(Parameters(SearchSymbolsArgs {
                query: query.to_string(),
                path: None,
                language: None,
                limit: None,
                offset: None,
                snippet_lines: None,
                format: None,
            }))
            .await
            .unwrap(),
    )
}

/// Everything after `hybrid_search`'s first (mode) line.
fn body(text: &str) -> &str {
    text.split_once('\n')
        .map(|(_, rest)| rest)
        .unwrap_or_default()
}

#[tokio::test]
async fn alpha_zero_is_search_symbols_verbatim_and_says_so() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    for query in ["create invoice", "createInvoice", "zzz"] {
        let text = hybrid(&server, args(query, Some(0.0))).await;
        assert!(
            text.starts_with("hybrid: alpha 0, lexical ranking only\n"),
            "got: {text}"
        );
        assert_eq!(body(&text), lexical(&server, query).await, "{query}");
    }
}

#[cfg(not(feature = "semantic"))]
#[tokio::test]
async fn without_a_model_it_degrades_to_lexical_and_names_the_reason() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let text = hybrid(&server, args("create invoice", None)).await;
    let (note, rest) = text.split_once('\n').unwrap();
    assert!(
        note.starts_with("hybrid: lexical ranking only"),
        "got: {note}"
    );
    assert!(note.contains("--features semantic"), "got: {note}");
    assert_eq!(rest, lexical(&server, "create invoice").await);
}

#[tokio::test]
async fn top_k_defaults_to_ten_and_is_clamped() {
    let server = build_server_at(&fixture("many-callers")).await;
    let count = |t: &str| t.lines().filter(|l| l.contains(" caller_")).count();

    let text = hybrid(&server, args("caller", Some(0.0))).await;
    assert_eq!(count(&text), 10, "got: {text}");

    let mut huge = args("caller", Some(0.0));
    huge.top_k = Some(100_000);
    assert_eq!(count(&hybrid(&server, huge).await), 70);

    let mut zero = args("caller", Some(0.0));
    zero.top_k = Some(0);
    assert_eq!(count(&hybrid(&server, zero).await), 1);

    let mut paged = args("caller", Some(0.0));
    paged.offset = Some(65);
    assert_eq!(count(&hybrid(&server, paged).await), 5);
}

#[tokio::test]
async fn out_of_range_alpha_is_clamped_not_rejected() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let text = hybrid(&server, args("create invoice", Some(-3.0))).await;
    assert!(text.starts_with("hybrid: alpha 0,"), "got: {text}");
    // NaN can't arrive through JSON, but a non-finite value falls back to the
    // default rather than poisoning the ranking.
    assert!(server
        .hybrid_search(Parameters(args("create invoice", Some(f64::INFINITY))))
        .await
        .is_ok());
}

#[tokio::test]
async fn toon_output_keeps_the_mode_line_first() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    let mut toon = args("create invoice", Some(0.0));
    toon.format = Some("toon".to_string());
    let text = hybrid(&server, toon).await;
    assert!(text.starts_with("hybrid: "), "got: {text}");
    assert!(text.contains("matches[2]{"), "got: {text}");
}

#[tokio::test]
async fn an_empty_query_is_rejected() {
    let server = build_server_at(&fixture("compute-app")).await;
    assert!(server
        .hybrid_search(Parameters(args("  ", None)))
        .await
        .is_err());
}

#[tokio::test]
async fn a_quoted_query_is_lexical_only_whatever_alpha() {
    let server = build_server_at(&fixture("polyglot-app")).await;
    for alpha in [None, Some(1.0)] {
        let text = hybrid(&server, args("\"create invoice\"", alpha)).await;
        assert!(
            text.starts_with("hybrid: alpha 0 (exact phrase), lexical ranking only\n"),
            "got: {text}"
        );
        let first = body(&text).lines().nth(1).unwrap_or_default();
        assert!(first.ends_with("function createInvoice"), "got: {text}");
    }
}

/// A throwaway project with one Rust file holding an error message.
fn literal_project(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mct-hybrid-literals-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/db.rs"),
        "pub fn connect(url: &str) -> Result<(), String> {\n    \
         if url.is_empty() {\n        \
         return Err(format!(\"Error de conexión con la BD: {}\", url));\n    \
         }\n    Ok(())\n}\n",
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

#[tokio::test]
async fn a_quoted_phrase_finds_the_string_literal_holding_it() {
    let server = build_server_at(&literal_project("found")).await;
    let text = hybrid(&server, args("\"Error de conexion con la BD\"", None)).await;
    assert!(
        text.contains(
            "1 string literal(s) holding \"Error de conexion con la BD\":\n\
             src/db.rs:3 in function connect \"Error de conexión con la BD:\"\n"
        ),
        "got: {text}"
    );

    let toon = hybrid(
        &server,
        HybridSearchArgs {
            format: Some("toon".to_string()),
            ..args("\"conexión con la BD\"", None)
        },
    )
    .await;
    assert!(toon.contains("literals[1]{path,line,language,kind,symbol,text}"), "got: {toon}");
}

#[tokio::test]
async fn literals_are_searched_only_for_an_exact_phrase() {
    let server = build_server_at(&literal_project("unquoted")).await;
    for query in ["Error de conexion con la BD", "\"conexion la BD\""] {
        let text = hybrid(&server, args(query, Some(0.0))).await;
        assert!(!text.contains("string literal(s)"), "{query}: {text}");
    }
}
