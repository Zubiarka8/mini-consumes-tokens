//! String literals for exact-text search, shared by the grammars whose
//! strings split into fixed fragments, escape sequences and interpolation
//! holes. Each run of fixed text between two holes is one literal, through
//! the shared prose filter ([`mct_core::LiteralCollector`]). A string that
//! stands alone as a statement (a docstring, a `"use strict"` directive) is
//! documentation, not a value, and is skipped.

use mct_core::{LiteralCollector, StringLiteral};
use tree_sitter::Node;

/// How a grammar spells its string literals.
pub struct LiteralGrammar<'a> {
    /// Node kinds that are one string literal.
    pub strings: &'a [&'a str],
    /// Child kinds holding fixed text (escapes inside it are resolved).
    pub fragments: &'a [&'a str],
    /// Child kinds holding one escape sequence.
    pub escapes: &'a [&'a str],
    /// Child kinds interpolating a value: each ends a literal.
    pub holes: &'a [&'a str],
    /// Kinds joining strings into one value (`"a" "b"`), climbed through
    /// when looking for a statement.
    pub concatenations: &'a [&'a str],
    /// Statement kinds where a lone string is documentation.
    pub statements: &'a [&'a str],
}

/// How the text of one string reads.
#[derive(Clone, Copy, Default)]
pub struct StringStyle {
    /// Backslashes are kept as written (`r"…"`).
    pub raw: bool,
    /// `{{` and `}}` read as one brace (a Python f-string).
    pub doubled_braces: bool,
}

/// Every literal of the file under `root`, in source order. `style` reads
/// one string node's prefix, or gives `None` for a string that is no
/// message (a JSX `className` list).
pub fn collect_literals(
    root: Node,
    source: &str,
    grammar: &LiteralGrammar,
    style: impl Fn(Node) -> Option<StringStyle>,
) -> Vec<StringLiteral> {
    let mut literals = LiteralCollector::default();
    let mut cursor = root.walk();
    // Iterative pre-order walk: no recursion depth to bound.
    loop {
        let node = cursor.node();
        if grammar.strings.contains(&node.kind()) && !is_statement(node, grammar) {
            if let Some(style) = style(node) {
                push_fragments(node, source, grammar, style, &mut literals);
            }
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

fn is_statement(string: Node, grammar: &LiteralGrammar) -> bool {
    let mut parent = string.parent();
    while let Some(p) = parent.filter(|p| grammar.concatenations.contains(&p.kind())) {
        parent = p.parent();
    }
    parent.is_some_and(|p| grammar.statements.contains(&p.kind()) && p.named_child_count() == 1)
}

fn push_fragments(
    string: Node,
    source: &str,
    grammar: &LiteralGrammar,
    style: StringStyle,
    literals: &mut LiteralCollector,
) {
    let mut text = String::new();
    let mut line = None;
    let mut cursor = string.walk();
    for child in string.children(&mut cursor) {
        let kind = child.kind();
        let raw_text = child.utf8_text(source.as_bytes()).unwrap_or_default();
        if grammar.holes.contains(&kind) {
            flush(&mut text, &mut line, literals);
        } else if grammar.fragments.contains(&kind) || grammar.escapes.contains(&kind) {
            if line.is_none() {
                let visible = raw_text.trim_start();
                if !visible.is_empty() {
                    let before = raw_text
                        .get(..raw_text.len() - visible.len())
                        .unwrap_or_default();
                    let row = child.start_position().row as u32 + 1;
                    line = Some(row + before.matches('\n').count() as u32);
                }
            }
            text.push_str(&unescape(raw_text, style));
        }
    }
    flush(&mut text, &mut line, literals);
}

fn flush(text: &mut String, line: &mut Option<u32>, literals: &mut LiteralCollector) {
    if let Some(line) = line.take() {
        literals.push(text, line);
    }
    text.clear();
}

/// The text a fragment stands for: `\n`, `\"` and the like resolved (an
/// escape with no printable meaning reads as a space, a backslash-newline
/// continuation as nothing), and doubled braces as one when the style says
/// so. A raw string keeps its backslashes.
fn unescape(fragment: &str, style: StringStyle) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut chars = fragment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !style.raw => match chars.next() {
                Some(q @ ('"' | '\'' | '\\' | '`' | '$')) => out.push(q),
                Some('\n') => {}
                _ => out.push(' '),
            },
            '{' | '}' if style.doubled_braces && chars.peek() == Some(&c) => {
                chars.next();
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}
