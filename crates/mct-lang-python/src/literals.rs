//! String literals for exact-text search: each fixed fragment of a string
//! (the text between f-string holes), through the shared prose filter. A
//! string standing alone as a statement (a docstring) is documentation and
//! is skipped.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

use crate::text;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string"],
    fragments: &["string_content"],
    escapes: &[],
    holes: &["interpolation"],
    concatenations: &["concatenated_string"],
    statements: &["expression_statement"],
    opaque: &[],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        let prefix = string
            .named_child(0)
            .filter(|start| start.kind() == "string_start")
            .map(|start| text(start, source).to_ascii_lowercase())
            .unwrap_or_default();
        Some(StringStyle {
            raw: prefix.contains('r'),
            doubled_braces: prefix.contains('f') || prefix.contains('t'),
            doubled_quotes: false,
            shell_escapes: false,
        })
    })
}
