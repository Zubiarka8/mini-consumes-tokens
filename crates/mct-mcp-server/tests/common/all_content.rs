//! Every text block of a tool result, joined with newlines.
//!
//! Included with `#[path]` by the tests that must see a second block a tool
//! might add (`tests/context_pack.rs`, `tests/exclusion_privacy.rs`). Tests
//! that read only the first block use `common/content.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// All text blocks joined with `\n`; an empty string when there are none.
pub fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}
