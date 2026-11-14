//! String literals for exact-text search: each string literal or text
//! block (`"""…"""`), through the shared prose filter, split at a string
//! template's `\{…}` holes.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string_literal"],
    fragments: &["string_fragment", "multiline_string_fragment"],
    escapes: &["escape_sequence"],
    holes: &["string_interpolation"],
    concatenations: &[],
    statements: &[],
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |_| Some(StringStyle::default()))
}
