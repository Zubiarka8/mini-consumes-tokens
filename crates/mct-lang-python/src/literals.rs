//! String literals for exact-text search: each fixed fragment of a string
//! (the text between f-string holes), through the shared prose filter. A
//! string standing alone as a statement (a docstring) is documentation and
//! is skipped.

use mct_core::{LiteralCollector, StringLiteral};
use tree_sitter::Node;

use crate::text;

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    let mut literals = LiteralCollector::default();
    let mut cursor = root.walk();
    // Iterative pre-order walk: no recursion depth to bound.
    loop {
        let node = cursor.node();
        if node.kind() == "string" && !is_statement(node) {
            push_fragments(node, source, &mut literals);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return literals.finish();
            }
        }
    }
}

/// A string that is a whole statement: a docstring, or one used as a
/// comment. A part of an implicit concatenation counts as its whole.
fn is_statement(string: Node) -> bool {
    let mut parent = string.parent();
    while let Some(p) = parent.filter(|p| p.kind() == "concatenated_string") {
        parent = p.parent();
    }
    parent.is_some_and(|p| p.kind() == "expression_statement" && p.named_child_count() == 1)
}

fn push_fragments(string: Node, source: &str, literals: &mut LiteralCollector) {
    let prefix = string
        .named_child(0)
        .filter(|start| start.kind() == "string_start")
        .map(|start| text(start, source).to_ascii_lowercase())
        .unwrap_or_default();
    let raw = prefix.contains('r');
    let formatted = prefix.contains('f') || prefix.contains('t');
    let mut cursor = string.walk();
    for content in string
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "string_content")
    {
        let fragment = text(content, source);
        let leading = fragment.len() - fragment.trim_start().len();
        let before = fragment.get(..leading).unwrap_or_default();
        let line = content.start_position().row as u32 + 1 + before.matches('\n').count() as u32;
        literals.push(&unescape(fragment, raw, formatted), line);
    }
}

/// The text a fragment stands for: `\n`, `\"` and the like resolved (an
/// escape with no printable meaning reads as a space), and `{{` / `}}` of
/// an f-string as one brace. A raw string keeps its backslashes.
fn unescape(fragment: &str, raw: bool, formatted: bool) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut chars = fragment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !raw => match chars.next() {
                Some(q @ ('"' | '\'' | '\\')) => out.push(q),
                Some('\n') => {}
                _ => out.push(' '),
            },
            '{' | '}' if formatted && chars.peek() == Some(&c) => {
                chars.next();
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}
