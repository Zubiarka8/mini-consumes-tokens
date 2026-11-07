use mct_mcp_server::server::SearchSymbolsArgs;

pub fn search_args(query: &str) -> SearchSymbolsArgs {
    SearchSymbolsArgs {
        query: query.to_string(),
        path: None,
        language: None,
        limit: None,
        offset: None,
        snippet_lines: None,
        format: Some("text".to_string()),
        cache: None,
        detail: None,
        max_tokens: None,
    }
}
