//! Saved dashboard dismissals are resolved before MCP dead-code pagination.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mct_index::{review_keys::Fingerprints, ExcludeSet, Index};
use mct_mcp_server::server::{FindDeadCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mct-mcp-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".mct-index")).unwrap();
        let root = path.canonicalize().unwrap();
        fs::write(
            root.join("lib.rs"),
            "pub fn discarded() {}\npub fn pending() {}\n",
        )
        .unwrap();
        Self(root)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn index(root: &Path) -> Index {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    index
}
async fn output(index: Index, offset: usize) -> String {
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    let result = server
        .find_dead_code(Parameters(FindDeadCodeArgs {
            path: None,
            language: None,
            limit: Some(1),
            offset: Some(offset),
            format: Some("text".into()),
        }))
        .await
        .unwrap();
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|c| c.text.clone())
        .unwrap()
}

#[tokio::test]
async fn unchanged_dismissals_are_filtered_before_limit_and_offset_but_edits_return() {
    let project = Project::new();
    let original = index(&project.0);
    let navigation = original.navigation_snapshot().unwrap();
    let candidates = mct_index::find_dead_code_candidates(&original, None, None).unwrap();
    let keys = Fingerprints::new(&project.0, &navigation).dead_keys(&candidates, &navigation);
    let key = candidates
        .iter()
        .position(|c| c.name == "discarded")
        .unwrap();
    let (identity, fingerprint) = &keys[key];
    fs::write(project.0.join(".mct-index/reviews.toon"),format!("version: 2\nreviews[1]{{kind,identity,fingerprint,status,at,reason}}:\n  dead,{identity},{fingerprint},false_positive,7,framework entry\nhistory[0]{{kind,identity,fingerprint,status,at,reason}}:\n")).unwrap();
    let visible = output(original, 0).await;
    assert!(
        visible.contains("1 dead-code candidate(s)") && visible.contains("pending"),
        "{visible}"
    );
    assert!(!visible.contains("discarded"), "{visible}");
    assert!(!output(index(&project.0), 1).await.contains("  function"));
    fs::write(
        project.0.join("lib.rs"),
        "// moved\npub fn discarded() {}\npub fn pending() {}\n",
    )
    .unwrap();
    assert!(!output(index(&project.0), 0).await.contains("discarded"));
    fs::write(
        project.0.join("lib.rs"),
        "pub fn discarded() { let changed = 1; }\npub fn pending() {}\n",
    )
    .unwrap();
    assert!(output(index(&project.0), 0).await.contains("discarded"));
}
