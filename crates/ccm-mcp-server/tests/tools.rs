//! In-process test of the tool layer (bypassing the stdio/JSON-RPC
//! transport): calls the generated tool methods directly against a small
//! polyglot (Rust + Python) fixture versioned at `tests/fixtures/compute-app/`,
//! so a regression here is caught without standing up a full MCP client.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use ccm_index::{ExcludeSet, Index};
use ccm_mcp_server::server::{
    CcmServer, FindCallersArgs, FindCallsArgs, FindReferencesArgs, FindSymbolArgs,
    ImpactAnalysisArgs,
};
use rmcp::handler::server::wrapper::Parameters;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app")
}

/// `tests/fixtures/many-callers/`: one `target` function plus 70 callers —
/// deliberately more than `DEFAULT_RESULT_LIMIT` (50), used only to prove
/// result truncation actually triggers and is reported.
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

async fn build_server() -> CcmServer {
    build_server_at(&fixture_root()).await
}

async fn build_server_at(root: &std::path::Path) -> CcmServer {
    let registry = ccm_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    CcmServer::new(index, registry)
}

#[tokio::test]
async fn find_symbol_locates_rust_function() {
    let server = build_server().await;
    let result = server
        .find_symbol(Parameters(FindSymbolArgs {
            name: "compute".to_string(),
            limit: None,
        }))
        .await
        .unwrap();
    let text = content_of(&result);
    assert!(text.contains("src/lib.rs"), "got: {text}");
}

#[tokio::test]
async fn find_calls_and_find_callers_agree() {
    let server = build_server().await;

    let calls = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "compute".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(calls.contains("helper"), "got: {calls}");

    let callers = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "helper".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(callers.contains("compute"), "got: {callers}");
}

#[tokio::test]
async fn find_references_includes_the_python_import() {
    let server = build_server().await;
    let text = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                symbol: "compute".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("test_compute.py"), "got: {text}");
}

#[tokio::test]
async fn impact_analysis_flags_the_test() {
    let server = build_server().await;
    let text = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                symbol: "compute".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("1 likely affected test(s)"), "got: {text}");
    assert!(text.contains("test_compute"), "got: {text}");
}

#[tokio::test]
async fn empty_name_is_rejected_as_invalid_params() {
    let server = build_server().await;
    let result = server
        .find_symbol(Parameters(FindSymbolArgs {
            name: "   ".to_string(),
            limit: None,
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn find_callers_truncates_to_the_default_limit_and_says_so() {
    let server = build_server_at(&many_callers_fixture_root()).await;
    let text = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "target".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    // 70 callers exist; DEFAULT_RESULT_LIMIT (50) must cap the shown list
    // and the header must say how many were omitted.
    assert!(text.starts_with("70 caller(s) of this function"), "got: {text}");
    assert!(
        text.contains("(showing 50, 20 omitted — pass a higher `limit` to see the rest)"),
        "got: {text}"
    );
    let caller_line_count = text.lines().filter(|l| l.contains("caller_")).count();
    assert_eq!(caller_line_count, 50, "got: {text}");
}

#[tokio::test]
async fn find_callers_with_explicit_higher_limit_is_not_truncated() {
    let server = build_server_at(&many_callers_fixture_root()).await;
    let text = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "target".to_string(),
                limit: Some(100),
            }))
            .await
            .unwrap(),
    );
    assert!(text.starts_with("70 caller(s) of this function"), "got: {text}");
    assert!(!text.contains("omitted"), "got: {text}");
    let caller_line_count = text.lines().filter(|l| l.contains("caller_")).count();
    assert_eq!(caller_line_count, 70, "got: {text}");
}

#[tokio::test]
async fn impact_analysis_truncates_the_caller_and_reference_sections() {
    let server = build_server_at(&many_callers_fixture_root()).await;
    let text = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                symbol: "target".to_string(),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("70 direct caller(s)"), "got: {text}");
    assert!(
        text.contains("Direct callers (showing 50, 20 omitted — pass a higher `limit` to see the rest):"),
        "got: {text}"
    );
    assert!(
        text.contains("All references (showing 50, 20 omitted — pass a higher `limit` to see the rest):"),
        "got: {text}"
    );
}
