//! String literals for exact-text search: each run of fixed text of an
//! expandable string or here-string (`"…"`, `@"…"@`) between its
//! interpolations (`$x`, `$(…)`, `${…}`: every named child), and each
//! verbatim string or here-string (`'…'`, `@'…'@`), through the shared
//! prose filter.
//!
//! The grammar gives an expandable string no fragment nodes, only its
//! interpolations, so the fixed text is the source between them; that is
//! why this crate does not use `mct_tree_sitter::collect_literals`. A
//! backtick escape is resolved and a doubled quote is one; a backslash is
//! plain text in PowerShell.

use mct_core::{LiteralCollector, StringLiteral};
use tree_sitter::Node;

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    let mut literals = LiteralCollector::default();
    let mut cursor = root.walk();
    // Iterative pre-order walk: no recursion depth to bound.
    loop {
        let node = cursor.node();
        match node.kind() {
            "expandable_string_literal" | "expandable_here_string_literal" => {
                push_expandable(node, source, &mut literals);
            }
            "verbatim_string_characters" | "verbatim_here_string_characters" => {
                if let Some((start, body)) = body(node, source) {
                    push(source, start, &body.replace("''", "'"), &mut literals);
                }
            }
            _ => {}
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

/// The text between a string's delimiters, with its start byte: a
/// here-string's body runs from the line after `@"` / `@'` to the line
/// before `"@` / `'@`; a plain string's from after its opening quote to
/// before its closing one (straight or typographic).
fn body<'s>(string: Node, source: &'s str) -> Option<(usize, &'s str)> {
    let text = string.utf8_text(source.as_bytes()).ok()?;
    let (open, close) = if text.starts_with('@') {
        let open = text.find('\n')? + 1;
        let close = text.rfind('\n')?;
        (open, close.max(open))
    } else {
        let open = text.chars().next()?.len_utf8();
        let close = text.len() - text.chars().last()?.len_utf8();
        (open, close.max(open))
    };
    Some((string.start_byte() + open, text.get(open..close)?))
}

/// One run of fixed text per gap between interpolations.
fn push_expandable(string: Node, source: &str, literals: &mut LiteralCollector) {
    let Some((start, body)) = body(string, source) else {
        return;
    };
    let end = start + body.len();
    let mut from = start;
    let mut cursor = string.walk();
    for hole in string.named_children(&mut cursor) {
        // `from <= end` always holds, so neither bound can panic.
        let to = hole.start_byte().max(from).min(end);
        if let Some(gap) = source.get(from..to) {
            push(source, from, &unescape(gap), literals);
        }
        from = hole.end_byte().max(from).min(end);
    }
    if let Some(gap) = source.get(from..end) {
        push(source, from, &unescape(gap), literals);
    }
}

/// `text` as a literal on the line of its first visible char.
fn push(source: &str, start: usize, text: &str, literals: &mut LiteralCollector) {
    let lead = text.len() - text.trim_start().len();
    let before = source.get(..start).unwrap_or_default();
    let skipped = text.get(..lead).unwrap_or_default();
    let line = before.matches('\n').count() + skipped.matches('\n').count() + 1;
    literals.push(text, line as u32);
}

/// Backtick escapes resolved (an escape with no printable meaning, such as
/// `` `t ``, reads as a space) and `""` read as one quote.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => match chars.next() {
                Some(q @ ('"' | '\'' | '`' | '$')) => out.push(q),
                Some('\n') | None => {}
                Some(_) => out.push(' '),
            },
            '"' if chars.peek() == Some(&'"') => {
                chars.next();
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}
