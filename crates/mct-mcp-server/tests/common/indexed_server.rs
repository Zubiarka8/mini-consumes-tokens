use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;

pub fn indexed_server(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}
