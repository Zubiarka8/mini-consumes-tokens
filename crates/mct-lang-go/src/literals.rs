//! String literals for exact-text search: each interpreted (`"…"`) or raw
//! (`` `…` ``) string, through the shared prose filter. A raw string keeps
//! its backslashes. A struct field tag (`json:"id" db:"user_id"`) is
//! metadata read by reflection, not a message, and is skipped.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["interpreted_string_literal", "raw_string_literal"],
    fragments: &[
        "interpreted_string_literal_content",
        "raw_string_literal_content",
    ],
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
        (!is_field_tag(string)).then_some(StringStyle {
            raw: string.kind() == "raw_string_literal",
            doubled_braces: false,
            doubled_quotes: false,
        })
    })
}

/// The `tag` of a struct field declaration.
fn is_field_tag(string: Node) -> bool {
    string
        .parent()
        .filter(|p| p.kind() == "field_declaration")
        .and_then(|p| p.child_by_field_name("tag"))
        .is_some_and(|tag| tag.id() == string.id())
}
