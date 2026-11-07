//! A server over a freshly indexed root, as the tool tests build it.
//!
//! Each consumer includes this file with `#[path]` instead of going through
//! `common/mod.rs`, so none of them compiles the large retrieval fixtures.
//! Tests that build their server differently (extra options, an embedder,
//! the fixture root baked in) keep their own helpers.

use std::path::Path;

use mct_mcp_server::server::MctServer;

#[path = "indexed_server.rs"]
mod indexed;

/// An in-memory index of `root`, fully reindexed with the production
/// language registry, wrapped in a default-configured server.
pub async fn build_server_at(root: &Path) -> MctServer {
    indexed::indexed_server(root)
}
