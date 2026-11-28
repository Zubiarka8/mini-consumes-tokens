//! `find_files` (issue #629) as an MCP client sees it, over the path
//! catalog fixture indexed with the production language registry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use mct_mcp_server::file_query::FindFilesArgs;
use mct_mcp_server::server::{BatchArgs, BatchQuery};
use rmcp::handler::server::wrapper::Parameters;

#[path = "common/content.rs"]
mod content;
use content::content_of;
#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at;

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-find-files-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../mct-index/tests/fixtures/path-catalog"),
        &dir,
    );
    dir
}

#[tokio::test]
async fn find_files_lists_a_category_under_a_path_as_toon() {
    let dir = fixture("audio");
    let server = build_server_at(&dir).await;
    let text = content_of(
        &server
            .find_files(Parameters(FindFilesArgs {
                category: Some("audio".into()),
                path: Some("demo/".into()),
                ..Default::default()
            }))
            .await
            .unwrap(),
    );
    assert!(
        text.starts_with("filters: path=demo/ category=audio\ntotal: 5\n"),
        "{text}"
    );
    assert!(
        text.contains("files[5]{path,category,format,bytes}:\n  demo/audio/clip.opus,audio,opus,"),
        "{text}"
    );
    assert!(text.contains("next_offset: none"), "{text}");
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_typescript_source_is_not_mpeg_ts_but_both_are_found() {
    let dir = fixture("ts");
    let server = build_server_at(&dir).await;
    let find = |format: &str| FindFilesArgs {
        format: Some(format.into()),
        output_format: Some("text".into()),
        ..Default::default()
    };
    let video = content_of(&server.find_files(Parameters(find("ts"))).await.unwrap());
    assert!(
        video.contains("demo/video/broadcast.ts  video/mpeg-ts"),
        "{video}"
    );
    assert!(!video.contains("src/app.ts"), "{video}");
    let source = content_of(
        &server
            .find_files(Parameters(find("javascript_typescript")))
            .await
            .unwrap(),
    );
    assert!(
        source.contains("src/app.ts  source/javascript_typescript"),
        "{source}"
    );
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn invalid_filters_are_invalid_params() {
    let dir = fixture("invalid");
    let server = build_server_at(&dir).await;
    for args in [
        FindFilesArgs {
            category: Some("pictures".into()),
            ..Default::default()
        },
        FindFilesArgs {
            format: Some("nope".into()),
            ..Default::default()
        },
        FindFilesArgs {
            limit: Some(0),
            ..Default::default()
        },
        FindFilesArgs {
            path: Some("../up".into()),
            ..Default::default()
        },
        FindFilesArgs {
            output_format: Some("json".into()),
            ..Default::default()
        },
    ] {
        let err = server
            .find_files(Parameters(args.clone()))
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{args:?}: {err:?}"
        );
    }
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_empty_project_says_the_catalog_is_empty() {
    let dir = std::env::temp_dir().join(format!("mct-find-files-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let server = build_server_at(&dir).await;
    let text = content_of(
        &server
            .find_files(Parameters(FindFilesArgs::default()))
            .await
            .unwrap(),
    );
    assert!(text.starts_with("The path catalog is empty"), "{text}");
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn batch_runs_find_files() {
    let dir = fixture("batch");
    let server = build_server_at(&dir).await;
    let query: BatchQuery = serde_json::from_value(serde_json::json!({
        "tool": "find_files",
        "args": { "category": "document", "format": "pdf", "limit": 20 }
    }))
    .unwrap();
    let text = content_of(
        &server
            .batch(Parameters(BatchArgs {
                queries: vec![query],
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("docs/manual.pdf,document,pdf,"), "{text}");
    // The default exclusions keep `secrets/manual.pdf` out.
    assert!(!text.contains("secrets/"), "{text}");
    let _ = fs::remove_dir_all(dir);
}
