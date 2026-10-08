//! Which symbols count as top-level declarations for `get_file_skeleton` and
//! `get_project_overview`, on source shapes the shared `omni-app` fixture does
//! not cover.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{GetFileSkeletonArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

/// A throwaway project under the OS temp dir, removed on drop.
struct Project {
    root: PathBuf,
}

impl Project {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let root =
            std::env::temp_dir().join(format!("mct-top-level-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, content) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, content).unwrap();
        }
        Self { root }
    }

    async fn server(&self) -> MctServer {
        let registry = mct_mcp_server::registry::build_registry();
        let mut index = Index::open_in_memory(&self.root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        MctServer::new(index, registry)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn skeleton_of(server: &MctServer, path: &str) -> String {
    let result = server
        .get_file_skeleton(Parameters(GetFileSkeletonArgs {
            path: path.to_string(),
        }))
        .await
        .unwrap();
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn go_package_sharing_a_name_with_a_type_keeps_its_top_level_declarations() {
    // `package ledger` and `type ledger` share a name. The type and the
    // package-level function `NewLedger` must stay top-level. The method `Add`
    // belongs to the type and must not be promoted.
    let project = Project::new(
        "go-package-type-collision",
        &[(
            "backend/ledger.go",
            "package ledger\n\ntype ledger struct {\n\ttotal int\n}\n\nfunc NewLedger() *ledger {\n\treturn &ledger{}\n}\n\nfunc (l *ledger) Add(n int) {\n\tl.total += n\n}\n",
        )],
    );
    let text = skeleton_of(&project.server().await, "backend/ledger.go").await;
    assert!(text.contains("type ledger struct"), "got: {text}");
    assert!(text.contains("NewLedger"), "got: {text}");
    assert!(!text.contains("Add("), "got: {text}");
}
