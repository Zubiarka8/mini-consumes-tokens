//! `call_read_only_tool` as the budget tests use it: every text block joined
//! with newlines on success, the error message on failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_mcp_server::server::MctServer;
use serde_json::Value;

pub async fn try_call(server: &MctServer, tool: &str, args: Value) -> Result<String, String> {
    match server
        .call_read_only_tool(tool, args.as_object().cloned())
        .await
    {
        Ok(r) => Ok(r
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")),
        Err(e) => Err(e.message.to_string()),
    }
}
