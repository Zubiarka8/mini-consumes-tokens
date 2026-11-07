//! Every text block of a tool result, joined with newlines.
//!
//! Included with `#[path]` by the tests that must see a second block a tool
//! might add (`tests/context_pack.rs`, `tests/exclusion_privacy.rs`). Tests
//! that read only the first block use `common/content.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// All text blocks joined with `\n`; an empty string when there are none.
pub use mct_mcp_server::content::result_text as content_of;
