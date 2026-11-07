use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{MctServer, ServerOptions};

pub fn server_with(root: &Path, options: ServerOptions) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::with_options(index, registry, options)
}

pub async fn reindex(server: &MctServer) {
    let registry = server.registry_handle();
    server
        .index_handle()
        .lock()
        .await
        .reindex(&registry, false)
        .unwrap();
}
