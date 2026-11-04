//! A server over a freshly indexed root, as the tool tests build it.
//!
//! Each consumer includes this file with `#[path]` instead of going through
//! `common/mod.rs`, so none of them compiles the large retrieval fixtures.
//! Tests that build their server differently (extra options, an embedder,
//! the fixture root baked in) keep their own helpers.

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;

/// An in-memory index of `root`, fully reindexed with the production
/// language registry, wrapped in a default-configured server.
pub async fn build_server_at(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}
