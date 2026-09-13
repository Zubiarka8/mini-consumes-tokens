//! In-process test of the tool layer (bypassing the stdio/JSON-RPC
//! transport): calls the generated tool methods directly against a small
//! polyglot (Rust + Python) fixture, so a regression here is caught without
//! standing up a full MCP client.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ccm_index::{ExcludeSet, Index};
use ccm_mcp_server::server::{
    CcmServer, FindCallersArgs, FindCallsArgs, FindReferencesArgs, FindSymbolArgs,
    ImpactAnalysisArgs,
};
use rmcp::handler::server::wrapper::Parameters;

fn tempdir() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let dir = std::env::temp_dir().join(format!(
        "ccm-mcp-server-test-{}",
        nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
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
    let dir = tempdir();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        "pub fn compute() -> i32 { helper() }\nfn helper() -> i32 { 1 }\n",
    )
    .unwrap();
    fs::write(
        dir.join("test_compute.py"),
        "from src_bindings import compute\n\ndef test_compute():\n    assert compute() == 1\n",
    )
    .unwrap();

    let registry = ccm_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    CcmServer::new(index, registry)
}

#[tokio::test]
async fn find_symbol_locates_rust_function() {
    let server = build_server().await;
    let result = server
        .find_symbol(Parameters(FindSymbolArgs {
            name: "compute".to_string(),
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
            }))
            .await
            .unwrap(),
    );
    assert!(calls.contains("helper"), "got: {calls}");

    let callers = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "helper".to_string(),
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
        }))
        .await;
    assert!(result.is_err());
}
