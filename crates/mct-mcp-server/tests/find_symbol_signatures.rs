//! `find_symbol` shows each definition's stored declaration header
//! (ADR-034, issue #347), whole when written over several lines, and none
//! once the file no longer holds the indexed revision.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at as build_server;

#[path = "common/all_content.rs"]
mod all_content;
use all_content::content_of;

use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

const SOURCE: &str = "pub fn total(\n    prices: &[u64],\n    tax: u64,\n) -> u64 {\n    tax\n}\n";

async fn find(server: &MctServer, format: &str) -> String {
    let Value::Object(args) = json!({ "name": "total", "format": format }) else {
        unreachable!()
    };
    content_of(
        &server
            .call_read_only_tool("find_symbol", Some(args))
            .await
            .unwrap(),
    )
}

fn project(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("mct-find-sig-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), SOURCE).unwrap();
    root
}

#[tokio::test]
async fn a_definition_shows_its_whole_stored_header() {
    let root = project("whole");
    let server = build_server(&root).await;
    let text = find(&server, "text").await;
    let toon = find(&server, "toon").await;
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        text.contains(
            "src/lib.rs:1:1 [rust] function total | pub fn total( prices: &[u64], tax: u64, ) -> u64\n"
        ),
        "{text}"
    );
    assert!(toon.contains(",signature}"), "{toon}");
    assert!(
        toon.contains("\"pub fn total( prices: &[u64], tax: u64, ) -> u64\""),
        "{toon}"
    );
}

#[tokio::test]
async fn a_changed_file_shows_no_header() {
    let root = project("stale");
    let server = build_server(&root).await;
    std::fs::write(root.join("src/lib.rs"), SOURCE.replace("tax", "fee")).unwrap();
    let text = find(&server, "text").await;
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        text.contains("src/lib.rs:1:1 [rust] function total\n"),
        "{text}"
    );
    assert!(!text.contains("pub fn"), "{text}");
}
