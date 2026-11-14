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
    /// String kinds with no fragment children (C#'s `@"…"`): the text
    /// between the first and the last `"` is one fragment.
    pub opaque: &'a [&'a str],
    /// A `$` fragment directly followed by a fragment starting with a name
    /// is a `$name` template, a hole: kotlin-ng lexes `"a $x b"` as the
    /// fragments `a `, `$`, `x b` instead of an interpolation node.
    pub dollar_names: bool,
}

/// How the text of one string reads.
#[derive(Clone, Copy, Default)]
pub struct StringStyle {
    /// Backslashes are kept as written (`r"…"`).
    pub raw: bool,
    /// `{{` and `}}` read as one brace (a Python f-string).
    pub doubled_braces: bool,
    /// `""` reads as one quote (a C# verbatim string).
    pub doubled_quotes: bool,
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
    if grammar.opaque.contains(&string.kind()) {
        let raw_text = string.utf8_text(source.as_bytes()).unwrap_or_default();
        let body = match (raw_text.find('"'), raw_text.rfind('"')) {
            (Some(open), Some(close)) if open < close => raw_text.get(open + 1..close),
            _ => None,
        };
        if let Some(body) = body {
            let visible = body.trim_start();
            let before = body.get(..body.len() - visible.len()).unwrap_or_default();
            let line = string.start_position().row as u32 + 1 + before.matches('\n').count() as u32;
            literals.push(&unescape(body, style), line);
        }
        return;
    }
    let mut text = String::new();
    let mut line = None;
    let mut dollar = false;
    let mut cursor = string.walk();
    for child in string.children(&mut cursor) {
        let kind = child.kind();
        let mut raw_text = child.utf8_text(source.as_bytes()).unwrap_or_default();
        let is_fragment = grammar.fragments.contains(&kind);
        if std::mem::take(&mut dollar) {
            let name_len = if is_fragment {
                name_prefix_len(raw_text)
            } else {
                0
            };
            if name_len > 0 {
                flush(&mut text, &mut line, literals);
                raw_text = raw_text.get(name_len..).unwrap_or_default();
            } else {
                text.push('$');
            }
        }
        if grammar.dollar_names && is_fragment && raw_text == "$" {
            dollar = true;
        } else if grammar.holes.contains(&kind) {
            flush(&mut text, &mut line, literals);
        } else if is_fragment || grammar.escapes.contains(&kind) {
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
            // An escape node is always an escape; `raw` keeps a fragment's
            // backslashes as written.
            let style = StringStyle {
                raw: style.raw && is_fragment,
                ..style
            };
            text.push_str(&unescape(raw_text, style));
        }
    }
    if dollar {
        text.push('$');
    }
    flush(&mut text, &mut line, literals);
}

/// Length in bytes of the name `text` starts with (a letter or `_`, then
/// letters, digits or `_`), or 0.
fn name_prefix_len(text: &str) -> usize {
    let mut chars = text.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_alphabetic() || c == '_' => {}
        _ => return 0,
    }
    chars
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_'))
        .map_or(text.len(), |(i, _)| i)
}

fn flush(text: &mut String, line: &mut Option<u32>, literals: &mut LiteralCollector) {
    if let Some(line) = line.take() {
        literals.push(text, line);
    }
    text.clear();
}

/// The text a fragment stands for: `\n`, `\"` and the like resolved (an
/// escape with no printable meaning reads as a space, a backslash-newline
/// continuation as nothing), and doubled braces or quotes as one when the
/// style says so. A raw string keeps its backslashes.
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
            '"' if style.doubled_quotes && chars.peek() == Some(&'"') => {
                chars.next();
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}
