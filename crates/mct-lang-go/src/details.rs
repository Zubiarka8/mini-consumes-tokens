//! Declaration details (ADR-034) read from a Go declaration's own syntax:
//! its header, receiver and parameters, results, type parameters, and the
//! visibility the exported-name rule gives it. Go has no modifiers.

use mct_core::{Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// A `function_declaration`, `method_declaration` or interface `method_elem`.
pub(crate) fn function_detail(
    node: Node,
    name: &str,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    let mut parameters = Vec::new();
    if let Some(receiver) = node.child_by_field_name("receiver") {
        parameters.extend(declared(receiver, source).into_iter().map(|p| Parameter {
            kind: ParameterKind::Receiver,
            ..p
        }));
    }
    let known = node.child_by_field_name("parameters").map(|list| {
        parameters.extend(declared(list, source));
    });
    SymbolDetail {
        symbol,
        signature: header(source, node.start_byte(), end, ""),
        parameters,
        parameters_known: known.is_some(),
        return_type: node
            .child_by_field_name("result")
            .map(|r| TypeText::syntax(slice(r, source))),
        visibility: Some(visibility(name)),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// A `type_spec` or `type_alias`: `type Stack[T any] struct`, without the
/// struct's fields or the interface's methods.
pub(crate) fn type_detail(node: Node, name: &str, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("type")
        .and_then(|ty| match ty.kind() {
            "struct_type" => ty
                .named_children(&mut ty.walk())
                .find(|c| c.kind() == "field_declaration_list"),
            "interface_type" => ty.children(&mut ty.walk()).find(|c| c.kind() == "{"),
            _ => None,
        })
        .map_or(node.end_byte(), |body| body.start_byte());
    SymbolDetail {
        symbol,
        signature: header(source, node.start_byte(), end, "type "),
        visibility: Some(visibility(name)),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// Exported (an upper-case first letter) is visible outside the package.
fn visibility(name: &str) -> Visibility {
    if name.chars().next().is_some_and(char::is_uppercase) {
        Visibility::Public
    } else {
        Visibility::Package
    }
}

fn generics(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("type_parameters")
        .map(|g| slice(g, source).to_string())
}

/// One parameter per declared name: `a, b int` is two `int` parameters, and
/// an unnamed `int` (or `_ int`) is one without a name.
fn declared(list: Node, source: &str) -> Vec<Parameter> {
    let mut out = Vec::new();
    let mut cursor = list.walk();
    for decl in list.named_children(&mut cursor) {
        let kind = match decl.kind() {
            "parameter_declaration" => ParameterKind::Positional,
            "variadic_parameter_declaration" => ParameterKind::Variadic,
            _ => continue,
        };
        let type_text = decl
            .child_by_field_name("type")
            .map(|t| TypeText::syntax(slice(t, source)));
        let mut names = decl.walk();
        let names: Vec<_> = decl
            .children_by_field_name("name", &mut names)
            .map(|n| slice(n, source))
            .collect();
        if names.is_empty() {
            out.push(Parameter {
                name: None,
                type_text,
                default: None,
                kind,
            });
            continue;
        }
        for name in names {
            out.push(Parameter {
                name: (name != "_").then(|| name.to_string()),
                type_text: type_text.clone(),
                default: None,
                kind,
            });
        }
    }
    out
}

fn header(source: &str, start: usize, end: usize, prefix: &str) -> Option<String> {
    let text = source.get(start..end)?.trim_end();
    (!text.is_empty()).then(|| format!("{prefix}{text}"))
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
