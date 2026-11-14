//! String literals for exact-text search: each string or raw (`R"(…)"`)
//! string, through the shared prose filter. A raw string keeps its
//! backslashes. Each piece of a concatenation (`"a" "b"`) is its own
//! literal, as in Python.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string_literal", "raw_string_literal"],
    fragments: &["string_content", "raw_string_content"],
    escapes: &["escape_sequence"],
    holes: &[],
    concatenations: &[],
    statements: &[],
    opaque: &[],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        Some(StringStyle {
            raw: string.kind() == "raw_string_literal",
            ..StringStyle::default()
        })
    })
}
