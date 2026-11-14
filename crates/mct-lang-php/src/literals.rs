//! String literals for exact-text search: each run of fixed text of a
//! single-quoted, double-quoted, heredoc or nowdoc string between `$x` /
//! `{$…}` interpolations, through the shared prose filter. A heredoc's or
//! nowdoc's text sits in its body node, which is read as the string. A
//! single-quoted string's only escapes (`\'`, `\\`) are `escape_sequence`
//! nodes, so other backslashes stay as written.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string", "encapsed_string", "heredoc_body", "nowdoc_body"],
    fragments: &["string_content", "nowdoc_string"],
    escapes: &["escape_sequence"],
    // `{` opens a complex interpolation (`{$o->m()}`), whatever follows.
    holes: &[
        "{",
        "variable_name",
        "member_access_expression",
        "subscript_expression",
        "dynamic_variable_name",
    ],
    concatenations: &[],
    statements: &[],
    opaque: &[],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        Some(StringStyle {
            // Escapes are their own nodes; a fragment's backslashes are text.
            raw: matches!(string.kind(), "string" | "nowdoc_body"),
            ..StringStyle::default()
        })
    })
}
