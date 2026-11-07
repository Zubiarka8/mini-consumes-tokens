use mct_mcp_server::server::HybridSearchArgs;

pub fn hybrid_args(query: &str, alpha: f64) -> HybridSearchArgs {
    HybridSearchArgs {
        query: query.to_string(),
        alpha: Some(alpha),
        ..Default::default()
    }
}
