//! The text of a tool result's first content block, as the tool-output tests
//! read it.
//!
//! Each consumer includes this file with `#[path]` instead of going through
//! `common/mod.rs`, so none of them compiles the large retrieval fixtures.
//! Tests that need every text block (`tests/contracts.rs`,
//! `tests/context_pack.rs`, `tests/exclusion_privacy.rs`) keep their own
//! helpers: they join the blocks differently.

/// The first content block's text, or an empty string when the result has
/// no text block.
pub fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}
