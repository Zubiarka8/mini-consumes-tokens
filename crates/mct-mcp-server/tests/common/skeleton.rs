use mct_mcp_server::server::{GetFileSkeletonArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

pub async fn skeleton_of(server: &MctServer, path: &str) -> String {
    let result = server
        .get_file_skeleton(Parameters(GetFileSkeletonArgs {
            path: path.to_string(),
        }))
        .await
        .unwrap();
    crate::content::content_of(&result)
}
