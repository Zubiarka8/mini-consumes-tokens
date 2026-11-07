//! The approximate token counter shared by the token-reduction tests
//! (`tests/batch.rs`, `tests/context_pack.rs`, `tests/intent_eval.rs`),
//! `examples/format_benchmark.rs` and `examples/catalog_report.rs`.
//!
//! Each consumer includes this file with `#[path]` instead of going through
//! `common/mod.rs`, so none of them compiles the large retrieval fixtures.

#[path = "../../../mct-corpus/support/approx_tokens.rs"]
mod estimate;
pub use estimate::approx_tokens;
