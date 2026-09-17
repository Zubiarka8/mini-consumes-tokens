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
    GetFileSkeletonArgs, ImpactAnalysisArgs, ListSymbolsArgs,
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

/// `tests/fixtures/polyglot-app/`: a Go backend, a TypeScript frontend and a
/// Python deploy script (see `tests/polyglot_fixture.rs`) — used here for
/// `list_symbols`' directory/crate, kind- and language-filter cases, which
/// need more than one file/language to be meaningful.
fn polyglot_fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/polyglot-app")
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
async fn list_symbols_on_a_single_file_lists_its_functions_with_line_ranges() {
    let server = build_server().await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "src/lib.rs".to_string(),
                kind: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Functions:"), "got: {text}");
    assert!(text.contains("compute") && text.contains("L1-L3"), "got: {text}");
    assert!(text.contains("helper") && text.contains("L4-L6"), "got: {text}");
    // Single-file listing: the path appears once, in the header, not
    // repeated per entry.
    assert_eq!(
        text.matches("src/lib.rs").count(),
        1,
        "path should appear once, in the header: {text}"
    );
}

#[tokio::test]
async fn list_symbols_on_a_directory_spans_every_file_under_it_with_paths_shown() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "backend".to_string(),
                kind: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Invoice") && text.contains("backend/invoice.go"), "got: {text}");
    assert!(text.contains("Log") && text.contains("backend/logger.go"), "got: {text}");
    assert!(
        text.contains("HandleCreateInvoice") && text.contains("backend/server.go"),
        "got: {text}"
    );
    // Frontend/scripts symbols must not leak into a backend-scoped listing.
    assert!(!text.contains("createInvoice"), "got: {text}");
    assert!(!text.contains("deploy"), "got: {text}");
}

#[tokio::test]
async fn list_symbols_kind_filter_narrows_to_that_kind_only() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "backend".to_string(),
                kind: Some("struct".to_string()),
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Structs:"), "got: {text}");
    assert!(text.contains("Invoice"), "got: {text}");
    assert!(!text.contains("Functions:"), "got: {text}");
    assert!(!text.contains("HandleCreateInvoice"), "got: {text}");
}

#[tokio::test]
async fn list_symbols_language_filter_excludes_other_languages() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "scripts".to_string(),
                kind: None,
                language: Some("python".to_string()),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("deploy"), "got: {text}");
    assert!(
        text.contains("scripts/deploy.py") || text.contains("scripts/notify.py"),
        "got: {text}"
    );
}

#[tokio::test]
async fn list_symbols_combines_kind_and_language_filters_with_and() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "backend".to_string(),
                kind: Some("function".to_string()),
                language: Some("go".to_string()),
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Log"), "got: {text}");
    assert!(text.contains("HandleCreateInvoice"), "got: {text}");
    // AddItem is a Method, not a Function — must be excluded by the kind filter.
    assert!(!text.contains("AddItem"), "got: {text}");
}

#[tokio::test]
async fn list_symbols_with_no_matches_says_so() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "nonexistent_dir".to_string(),
                kind: None,
                language: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("No symbols found"), "got: {text}");
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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
                depth: None,
                offset: None,
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

#[tokio::test]
async fn find_calls_default_depth_is_direct_hits_only() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    // Go: HandleCreateInvoice -> AddItem -> Log — omitting `depth` must stay
    // single-hop, so the second-hop callee must not appear.
    let text = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "HandleCreateInvoice".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("AddItem"), "got: {text}");
    assert!(!text.contains("Log"), "second hop leaked into depth-1 output: {text}");
    assert!(!text.contains("[depth"), "direct hits must not carry a depth tag: {text}");
}

#[tokio::test]
async fn find_calls_with_depth_2_also_returns_the_second_hop() {
    let server = build_server_at(&polyglot_fixture_root()).await;
    let text = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "HandleCreateInvoice".to_string(),
                limit: None,
                depth: Some(2),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("AddItem"), "got: {text}");
    assert!(text.contains("--calls--> Log"), "got: {text}");
    assert!(text.contains("[depth 2]"), "second hop should be tagged: {text}");
}

#[tokio::test]
async fn find_callers_offset_pages_past_the_default_limit() {
    let server = build_server_at(&many_callers_fixture_root()).await;
    let text = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "target".to_string(),
                limit: Some(10),
                depth: None,
                offset: Some(60),
            }))
            .await
            .unwrap(),
    );
    // 70 total callers; offset 60 + limit 10 lands exactly on the last page.
    assert!(text.starts_with("70 caller(s) of this function"), "got: {text}");
    assert!(
        text.contains("(showing 10 starting at offset 60, 0 more available — pass `limit`/`offset` to see the rest)"),
        "got: {text}"
    );
    let caller_line_count = text.lines().filter(|l| l.contains("caller_")).count();
    assert_eq!(caller_line_count, 10, "got: {text}");
}

#[tokio::test]
async fn get_file_skeleton_collapses_rust_function_bodies() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                path: "src/lib.rs".to_string(),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("pub fn compute() -> i32 {"), "got: {text}");
    assert!(text.contains("fn helper() -> i32 {"), "got: {text}");
    assert!(!text.contains("helper()\n"), "compute's body call must be collapsed: {text}");
    assert!(!text.contains("    1\n"), "helper's body must be collapsed: {text}");
    // The synthetic whole-file `module` entry must not leak through as a
    // bogus extra skeleton block.
    assert!(!text.contains("Modules:"), "got: {text}");
    assert_eq!(text.matches("// ...").count(), 2, "one collapsed body per function: {text}");
}

#[tokio::test]
async fn get_file_skeleton_falls_back_to_declaration_only_for_python() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                path: "test_compute.py".to_string(),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("def test_compute():"), "got: {text}");
    assert!(text.contains("# ..."), "got: {text}");
    assert!(!text.contains("assert compute"), "body must not leak through: {text}");
}

#[tokio::test]
async fn get_file_skeleton_rejects_a_directory_path() {
    let server = build_server().await;
    let result = server
        .get_file_skeleton(Parameters(GetFileSkeletonArgs {
            path: "src".to_string(),
        }))
        .await;
    assert!(result.is_err(), "a directory/crate prefix must be rejected, not silently accepted");
}

#[tokio::test]
async fn get_file_skeleton_on_a_missing_file_is_a_clear_error_not_a_panic() {
    let server = build_server().await;
    let result = server
        .get_file_skeleton(Parameters(GetFileSkeletonArgs {
            path: "src/does_not_exist.rs".to_string(),
        }))
        .await;
    assert!(result.is_err());
}
