//! Regressions from the read-only workspace audit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

fn index(source: &str) -> (Index, std::path::PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "mct-audit-fix-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lib.rs"), source).unwrap();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    (index, root)
}

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    server
        .call_read_only_tool(tool, Some(args.as_object().unwrap().clone()))
        .await
        .unwrap()
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn multihop_output_reports_continuation_in_both_formats() {
    let (index, _) = index("fn a(){b();c();d();} fn b(){e();} fn c(){} fn d(){} fn e(){}");
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    for format in ["text", "toon"] {
        let first = call(
            &server,
            "find_calls",
            json!({"function":"a","depth":2,"limit":1,"format":format}),
        )
        .await;
        assert!(first.contains("At least 2"), "{first}");
        assert!(first.contains("more available"), "{first}");
        assert!(!first.contains("0 more available"), "{first}");
        let last = call(
            &server,
            "find_calls",
            json!({"function":"a","depth":2,"limit":1,"offset":3,"format":format}),
        )
        .await;
        assert!(last.starts_with("4 call(s)"), "{last}");
        assert!(last.contains("0 more available"), "{last}");
        let direct = call(
            &server,
            "find_calls",
            json!({"function":"a","limit":1,"format":format}),
        )
        .await;
        assert!(direct.starts_with("3 call(s)"), "{direct}");
    }
}

#[tokio::test]
async fn affected_tests_are_paged_after_filtering_non_test_callers() {
    let (index, _) = index("fn target(){}\nfn caller_a(){target();}\nfn caller_b(){target();}\nfn test_a(){target();}\nfn test_b(){target();}\nfn test_c(){target();}\nfn test_d(){target();}");
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    for format in ["text", "toon"] {
        for depth in [1, 2] {
            for (offset, expected) in [(0, ["test_a", "test_b"]), (2, ["test_c", "test_d"])] {
                let text = call(&server,"impact_analysis",json!({"symbol":"target","depth":depth,"limit":2,"offset":offset,"format":format})).await;
                let tests = text
                    .split("Likely affected tests")
                    .nth(1)
                    .unwrap()
                    .split("Direct callers")
                    .next()
                    .unwrap();
                for name in expected {
                    assert!(tests.contains(name), "{text}");
                }
                assert!(text.contains("4 likely affected test(s)"), "{text}");
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn status_exposes_read_failures_and_stale_rows() {
    use std::os::unix::fs::PermissionsExt;
    let (mut index, root) = index("fn example(){}");
    let file = root.join("lib.rs");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o0)).unwrap();
    let report = index.reindex(&mct_mcp_server::registry::build_registry(), false);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(report.unwrap().issues.len(), 1);
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    let text = call(&server, "get_indexing_status", json!({})).await;
    assert!(text.contains("last-good index data may be stale"), "{text}");
    assert!(text.contains("lib.rs"), "{text}");
}

#[tokio::test]
async fn affected_tests_keep_same_named_functions_in_different_files() {
    let (mut index, root) = index("fn target(){}");
    for path in ["a.rs", "b.rs"] {
        std::fs::write(root.join(path), "fn test_same(){target();target();}").unwrap();
    }
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    for format in ["text", "toon"] {
        for (offset, path) in [(0, "a.rs"), (1, "b.rs")] {
            let text = call(
                &server,
                "impact_analysis",
                json!({"symbol":"target","limit":1,"offset":offset,"format":format}),
            )
            .await;
            assert!(text.contains("2 likely affected test(s)"), "{text}");
            let tests = text
                .split("Likely affected tests")
                .nth(1)
                .unwrap()
                .split("Direct callers")
                .next()
                .unwrap();
            assert!(tests.contains(path), "{text}");
        }
    }
}

#[test]
fn oversized_manifest_keeps_last_good_dependencies_and_reports_failure() {
    let (mut index, root) = index("fn example(){}");
    let manifest = root.join("Cargo.toml");
    std::fs::write(&manifest, "[dependencies]\nserde = \"1\"\n").unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    let before = index.status().unwrap();
    assert!(!before.dependencies.is_empty());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&manifest)
        .unwrap()
        .set_len(mct_index::DEFAULT_MAX_FILE_BYTES + 1)
        .unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    let after = index.status().unwrap();
    assert_eq!(after.dependencies.len(), before.dependencies.len());
    assert!(after
        .read_failures
        .iter()
        .any(|failure| failure.relative_path == "Cargo.toml"));
}

#[tokio::test]
async fn oversized_source_is_rejected_by_indexing_and_source_tools() {
    let (mut index, root) = index("fn example(){}");
    let file = root.join("lib.rs");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_len(mct_index::DEFAULT_MAX_FILE_BYTES + 1)
        .unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    assert_eq!(index.status().unwrap().read_failures.len(), 1);
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    let result = server
        .call_read_only_tool(
            "get_file_skeleton",
            Some(json!({"path":"lib.rs"}).as_object().unwrap().clone()),
        )
        .await;
    assert!(
        result.is_err(),
        "oversized file must not be read by the skeleton tool"
    );
}
