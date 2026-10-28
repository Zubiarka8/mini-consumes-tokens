//! Source ranges must only be applied to the bytes of the indexed revision.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};

const SOURCE: &str = "pub fn alpha() -> i64 {\n    beta()\n}\npub fn beta() -> i64 {\n    41\n}\n";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "mct-source-revision-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(path.join("src/lib.rs"), SOURCE).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn server(&self) -> MctServer {
        let registry = mct_languages::build_registry();
        let mut index = Index::open_in_memory(&self.0, ExcludeSet::default()).unwrap();
        index.reindex(&registry, true).unwrap();
        MctServer::new(index, registry)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn call(server: &MctServer, tool: &str, args: Value) -> Result<String, rmcp::ErrorData> {
    let result: CallToolResult = server
        .call_tool_by_name(tool, args.as_object().cloned())
        .await?;
    Ok(result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn assert_verified_source(server: &MctServer) {
    let pack = call(
        server,
        "build_context_pack",
        json!({"symbol":"alpha", "format":"text"}),
    )
    .await
    .unwrap();
    assert!(pack.contains("pub fn alpha()"), "{pack}");
    assert!(pack.contains("pub fn beta()"), "related signature: {pack}");
    let search = call(
        server,
        "search_symbols_like",
        json!({"query":"alpha", "snippet_lines":3, "cache":false, "format":"text"}),
    )
    .await
    .unwrap();
    assert!(search.contains("pub fn alpha()"), "{search}");
    let skeleton = call(server, "get_file_skeleton", json!({"path":"src/lib.rs"}))
        .await
        .unwrap();
    assert!(skeleton.contains("pub fn alpha()"), "{skeleton}");
}

#[tokio::test]
async fn unchanged_and_reindexed_source_remain_available() {
    let project = Project::new();
    let server = project.server();
    assert_verified_source(&server).await;
    std::fs::write(
        project.0.join("src/lib.rs"),
        format!("// shifted\n{SOURCE}"),
    )
    .unwrap();
    call(&server, "reindex", json!({"force":true}))
        .await
        .unwrap();
    assert_verified_source(&server).await;
}

#[tokio::test]
async fn modified_source_never_supplies_stale_snippets_or_related_signatures() {
    for shifted in [false, true] {
        let project = Project::new();
        let server = project.server();
        let changed = SOURCE
            .replace("alpha", "unrelated")
            .replace("beta", "different");
        let changed = if shifted {
            format!("pub fn stranger() {{}}\n{changed}")
        } else {
            changed
        };
        std::fs::write(project.0.join("src/lib.rs"), changed).unwrap();
        for (tool, args) in [
            (
                "build_context_pack",
                json!({"symbol":"alpha", "format":"text"}),
            ),
            (
                "search_symbols_like",
                json!({"query":"alpha", "snippet_lines":3, "cache":false, "format":"text"}),
            ),
            (
                "hybrid_search_like",
                json!({"query":"alpha", "snippet_lines":3, "cache":false, "format":"text"}),
            ),
        ] {
            let text = call(&server, tool, args).await.unwrap();
            assert!(
                text.contains("alpha"),
                "indexed metadata stays available: {text}"
            );
            assert!(
                !text.contains("pub fn"),
                "{tool} applied stale ranges: {text}"
            );
        }
        let error = call(&server, "get_file_skeleton", json!({"path":"src/lib.rs"}))
            .await
            .unwrap_err();
        assert!(error.message.contains("reindex"), "{error:?}");
    }
}

#[tokio::test]
async fn deleted_and_binary_source_are_not_rendered_and_restoration_recovers() {
    for binary in [false, true] {
        let project = Project::new();
        let server = project.server();
        let path = project.0.join("src/lib.rs");
        if binary {
            std::fs::write(&path, [0xff, 0xfe]).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        let text = call(
            &server,
            "build_context_pack",
            json!({"symbol":"alpha", "format":"text"}),
        )
        .await
        .unwrap();
        assert!(!text.contains("pub fn"), "{text}");
        let text = call(
            &server,
            "search_symbols_like",
            json!({"query":"alpha", "snippet_lines":3, "format":"text"}),
        )
        .await
        .unwrap();
        assert!(!text.contains("pub fn"), "{text}");
        assert!(
            call(&server, "get_file_skeleton", json!({"path":"src/lib.rs"}))
                .await
                .is_err()
        );
        std::fs::write(path, SOURCE).unwrap();
        call(&server, "reindex", json!({"force":true}))
            .await
            .unwrap();
        assert_verified_source(&server).await;
    }
}

#[tokio::test]
async fn a_stale_related_file_loses_only_its_signature() {
    let project = Project::new();
    std::fs::write(
        project.0.join("src/lib.rs"),
        "pub fn alpha() -> i64 { beta() }\n",
    )
    .unwrap();
    std::fs::write(
        project.0.join("src/other.rs"),
        "pub fn beta() -> i64 { 41 }\n",
    )
    .unwrap();
    let server = project.server();
    let args = json!({"symbol":"alpha", "format":"text"});
    let before = call(&server, "build_context_pack", args.clone())
        .await
        .unwrap();
    assert!(before.contains("pub fn beta()"), "{before}");
    std::fs::write(
        project.0.join("src/other.rs"),
        "pub fn unrelated() -> i64 { 99 }\n",
    )
    .unwrap();
    let after = call(&server, "build_context_pack", args).await.unwrap();
    assert!(after.contains("pub fn alpha()"), "{after}");
    assert!(after.contains("callee beta"), "{after}");
    assert!(!after.contains("pub fn unrelated"), "{after}");
}

#[tokio::test]
async fn retrieval_keeps_its_existing_limit_above_two_mib() {
    let project = Project::new();
    let source = format!("// {}\n{SOURCE}", "x".repeat(2 * 1024 * 1024));
    std::fs::write(project.0.join("src/lib.rs"), source).unwrap();
    let server = project.server();
    let text = call(
        &server,
        "search_symbols_like",
        json!({"query":"alpha", "snippet_lines":3, "cache":false, "format":"text"}),
    )
    .await
    .unwrap();
    assert!(text.contains("pub fn alpha()"), "{text}");
}

#[tokio::test]
async fn cached_rankings_do_not_reuse_unverified_source() {
    let project = Project::new();
    let server = project.server();
    let args = json!({"query":"alpha", "snippet_lines":3, "cache":true, "format":"text"});
    let before = call(&server, "search_symbols_like", args.clone())
        .await
        .unwrap();
    assert!(before.contains("pub fn alpha()"), "{before}");
    std::fs::write(
        project.0.join("src/lib.rs"),
        SOURCE.replace("alpha", "unrelated"),
    )
    .unwrap();
    let after = call(&server, "search_symbols_like", args).await.unwrap();
    assert!(!after.contains("pub fn"), "{after}");
}
