//! Declaration details (ADR-034) for Bash: a function's header up to its
//! body, with its parameters unknown (a shell function declares none; it
//! reads `$1`, `$@`), and a file-level variable's declaration keyword and
//! flags with its name, never its value. `readonly`, or a `-r` flag, is the
//! one modifier. Bash has no types or visibility.

use mct_core::{Modifier, SymbolDetail, SymbolId};
use tree_sitter::Node;

pub(crate) fn function_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    let header = source.get(node.start_byte()..end).map(str::trim);
    SymbolDetail {
        symbol,
        signature: header.filter(|h| !h.is_empty()).map(str::to_string),
        ..Default::default()
    }
}

/// `assignment` is a `variable_assignment`, alone (`X=1`) or inside a
/// `declaration_command` (`readonly X=1`, `declare -r X=1`).
pub(crate) fn variable_detail(assignment: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let name = assignment
        .child_by_field_name("name")
        .map(|n| slice(n, source))
        .unwrap_or_default();
    let mut head: Vec<&str> = Vec::new();
    if let Some(command) = assignment
        .parent()
        .filter(|p| p.kind() == "declaration_command")
    {
        let mut cursor = command.walk();
        for (i, child) in command.children(&mut cursor).enumerate() {
            let word = slice(child, source);
            if i == 0 || (child.kind() == "word" && word.starts_with('-')) {
                head.push(word);
            }
        }
    }
    let readonly = head.first() == Some(&"readonly")
        || head
            .iter()
            .skip(1)
            .any(|flag| flag.starts_with('-') && flag.contains('r'));
    head.push(name);
    SymbolDetail {
        symbol,
        signature: (!name.is_empty()).then(|| head.join(" ")),
        modifiers: if readonly {
            vec![Modifier::Readonly]
        } else {
            Vec::new()
        },
        ..Default::default()
    }
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
