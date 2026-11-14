//! String literals for exact-text search: each run of fixed text of a
//! string or template literal (between `${…}` holes), through the shared
//! prose filter. A directive (`"use strict"`) or other string standing alone
//! as a statement is skipped, and so is a JSX `className` / `class` value,
//! a list of CSS classes rather than a message.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string", "template_string"],
    fragments: &["string_fragment"],
    escapes: &["escape_sequence"],
    holes: &["template_substitution"],
    concatenations: &[],
    statements: &["expression_statement"],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        (!is_class_list(string, source)).then_some(StringStyle::default())
    })
}

/// `className="…"` / `class="…"` of a JSX element.
fn is_class_list(string: Node, source: &str) -> bool {
    string
        .parent()
        .filter(|p| p.kind() == "jsx_attribute")
        .and_then(|attribute| attribute.named_child(0))
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
        .is_some_and(|name| matches!(name, "className" | "class"))
}
