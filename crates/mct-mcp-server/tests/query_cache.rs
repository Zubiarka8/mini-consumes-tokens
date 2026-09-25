//! Wires `crate::cache::QueryCache` end to end through `MctServer`, not just
//! in isolation (see `crate::cache`'s own unit tests). The property that
//! actually matters — and the one a wiring bug would silently break — is
//! that the `reindex` tool call invalidates every prior cached answer, so a
//! repeated tool call never serves a stale result after the index changes.
//! Response *content* is otherwise identical whether it was cached or freshly
//! computed (the cache is transparent by design), so that's the only
//! behavior these tests can observe from the outside.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{FindSymbolArgs, ListSymbolsArgs, MctServer, ReindexArgs};
use rmcp::handler::server::wrapper::Parameters;

/// Matches the hand-rolled temp-dir convention in
/// `tests/background_watcher.rs` — no `tempfile` dependency needed for a
/// single throwaway dir per test.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-mcp-server-cache-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn uuid_like() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

async fn build_server_at(root: &std::path::Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_symbol_added_after_a_cached_miss_is_found_once_reindex_runs() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    let server = build_server_at(&dir).await;

    let find_two = || FindSymbolArgs {
        name: "two".to_string(),
        match_mode: None,
        path: None,
        language: None,
        limit: None,
        format: None,
    };

    // Miss — `two` doesn't exist yet. This populates the fast-path cache
    // under the index's current generation.
    let before = content_of(&server.find_symbol(Parameters(find_two())).await.unwrap());
    assert!(before.contains("No symbol named `two`"), "got: {before}");

    // Add the symbol on disk and force a reindex through the same `reindex`
    // tool a real client would call — this must bump the index's generation
    // and, through it, invalidate the cache entry above.
    fs::write(dir.join("lib.rs"), "pub fn one() {}\npub fn two() {}\n").unwrap();
    let report = server
        .reindex(Parameters(ReindexArgs { force: true }))
        .await
        .unwrap();
    assert!(
        content_of(&report).contains("1 parsed"),
        "got: {}",
        content_of(&report)
    );

    // If the cache had wrongly kept serving the pre-reindex "not found"
    // answer, this would still fail here.
    let after = content_of(&server.find_symbol(Parameters(find_two())).await.unwrap());
    assert!(
        after.contains("two") && after.contains("lib.rs"),
        "got: {after}"
    );
}

#[tokio::test]
async fn a_repeated_identical_call_returns_byte_identical_output_across_a_cache_hit() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\npub fn two() {}\n").unwrap();
    let server = build_server_at(&dir).await;

    let args = || ListSymbolsArgs {
        path: "lib.rs".to_string(),
        kind: None,
        language: None,
        limit: None,
        format: None,
    };
    let first = content_of(&server.list_symbols(Parameters(args())).await.unwrap());
    let second = content_of(&server.list_symbols(Parameters(args())).await.unwrap());
    assert_eq!(first, second);
    assert!(first.contains("one") && first.contains("two"), "got: {first}");
}
