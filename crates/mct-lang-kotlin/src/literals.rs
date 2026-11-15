//! String literals for exact-text search: each run of fixed text of a
//! string or raw (`"""…"""`) string between `$x` / `${…}` templates,
//! through the shared prose filter. A raw string keeps its backslashes.
//! kotlin-ng gives a plain string's `$name` no interpolation node, so the
//! shared walk reads a `$` fragment before a name as one (`dollar_names`).

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string_literal", "multiline_string_literal"],
    fragments: &["string_content"],
    escapes: &["escape_sequence"],
    holes: &["interpolation"],
    concatenations: &[],
    statements: &[],
    opaque: &[],
    dollar_names: true,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        Some(StringStyle {
            raw: string.kind() == "multiline_string_literal",
            doubled_braces: false,
            doubled_quotes: false,
        })
    })
}
