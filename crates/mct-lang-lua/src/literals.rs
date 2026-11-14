//! String literals for exact-text search: each quoted or long (`[[…]]`,
//! `[==[…]==]`) string, through the shared prose filter. A quoted string's
//! escapes sit inside its `string_content` and are resolved there; a long
//! string has none and keeps its backslashes.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string"],
    fragments: &["string_content"],
    escapes: &[],
    holes: &[],
    concatenations: &[],
    statements: &[],
    opaque: &[],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        let long = string
            .utf8_text(source.as_bytes())
            .is_ok_and(|t| t.starts_with('['));
        Some(StringStyle {
            raw: long,
            ..StringStyle::default()
        })
    })
}
