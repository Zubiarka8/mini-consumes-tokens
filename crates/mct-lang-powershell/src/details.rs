//! Declaration details (ADR-034) for PowerShell: a function's header up to
//! its body (`function Add([int]$a, $b = 2)`, `filter Only-Big`), its
//! parameters from the inline list or the body's `param()` block, and a
//! file-level variable's assignment target (`$Global:Limit`, `[int]$Max`).
//! A parameter's `[type]` literal is its type; other attributes
//! (`[Parameter(Mandatory)]`, validators) are not. A function with neither
//! parameter form has unknown parameters: it can still read `$args`.
//! PowerShell functions have no visibility or return type syntax.

use mct_core::{Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText};
use mct_tree_sitter::find_child;
use tree_sitter::Node;

pub(crate) fn function_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = find_child(node, "{").map_or(node.end_byte(), |b| b.start_byte());
    let header = source.get(node.start_byte()..end).map(str::trim);
    // The inline list or the body's `param()` block, whichever is written.
    let declaration = find_child(node, "function_parameter_declaration")
        .or_else(|| find_child(node, "script_block").and_then(|b| find_child(b, "param_block")));
    let list = declaration.and_then(|d| find_child(d, "parameter_list"));
    SymbolDetail {
        symbol,
        signature: header.filter(|h| !h.is_empty()).map(str::to_string),
        parameters: list.map(|l| parameters(l, source)).unwrap_or_default(),
        parameters_known: declaration.is_some(),
        ..Default::default()
    }
}

fn parameters(list: Node, source: &str) -> Vec<Parameter> {
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .filter(|c| c.kind() == "script_parameter")
        .map(|param| {
            let name = find_child(param, "variable")
                .map(|v| slice(v, source).trim_start_matches('$').to_string())
                .filter(|n| !n.is_empty());
            let type_text = find_child(param, "attribute_list").and_then(|attrs| {
                let mut cursor = attrs.walk();
                let found = attrs
                    .children(&mut cursor)
                    .filter_map(|a| find_child(a, "type_literal"))
                    .filter_map(|t| find_child(t, "type_spec"))
                    .map(|spec| slice(spec, source))
                    .find(|t| !t.is_empty());
                found.map(TypeText::syntax)
            });
            let default = find_child(param, "script_parameter_default").and_then(|d| {
                let text = slice(d, source).trim();
                let value = text.strip_prefix('=').unwrap_or(text).trim();
                (!value.is_empty()).then(|| value.to_string())
            });
            Parameter {
                name,
                type_text,
                default,
                kind: ParameterKind::Positional,
            }
        })
        .collect()
}

/// `target` is the `left_assignment_expression` of a file-level assignment.
pub(crate) fn variable_detail(target: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let header = slice(target, source).trim();
    SymbolDetail {
        symbol,
        signature: (!header.is_empty()).then(|| header.to_string()),
        ..Default::default()
    }
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
