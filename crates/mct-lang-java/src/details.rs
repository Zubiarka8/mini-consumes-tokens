//! Declaration details (ADR-034) read from a Java declaration's own syntax:
//! its header without annotations or body, parameters, return type, type
//! parameters, visibility and modifiers. Nothing is inferred.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// A `method_declaration` or `constructor_declaration`.
pub(crate) fn method_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let parameters = node
        .child_by_field_name("parameters")
        .map(|list| parameters(list, source));
    SymbolDetail {
        symbol,
        signature: header(node, body_start(node), source),
        parameters_known: parameters.is_some(),
        parameters: parameters.unwrap_or_default(),
        return_type: node
            .child_by_field_name("type")
            .map(|t| TypeText::syntax(slice(t, source))),
        visibility: Some(visibility(node, source)),
        modifiers: modifiers(node, source),
        generics: generics(node, source),
    }
}

/// A class, record, interface, annotation type or enum.
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    SymbolDetail {
        symbol,
        signature: header(node, body_start(node), source),
        visibility: Some(visibility(node, source)),
        modifiers: modifiers(node, source),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// One `variable_declarator` of a `field_declaration`: the declaration's
/// modifiers and type with this declarator's own name.
pub(crate) fn field_detail(
    declaration: Node,
    declarator: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let name = declarator
        .child_by_field_name("name")
        .map(|n| slice(n, source))
        .unwrap_or_default();
    let head = declaration
        .child_by_field_name("type")
        .and_then(|ty| header(declaration, ty.end_byte(), source));
    SymbolDetail {
        symbol,
        signature: head.map(|head| format!("{head} {name}")),
        visibility: Some(visibility(declaration, source)),
        modifiers: modifiers(declaration, source),
        ..Default::default()
    }
}

fn body_start(node: Node) -> usize {
    node.child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte())
}

/// `node`'s text up to `end`, without its annotations, comments or `;`.
fn header(node: Node, end: usize, source: &str) -> Option<String> {
    let mut skipped = Vec::new();
    if let Some(modifiers) = modifiers_node(node) {
        let mut cursor = modifiers.walk();
        skipped.extend(
            modifiers
                .children(&mut cursor)
                .filter(|c| c.kind().ends_with("annotation") || c.kind().ends_with("comment"))
                .map(|c| c.byte_range()),
        );
    }
    let mut text = String::new();
    let mut at = node.start_byte();
    for range in skipped {
        text.push_str(source.get(at..range.start.min(end))?);
        text.push(' ');
        at = range.end;
    }
    text.push_str(source.get(at.min(end)..end)?);
    let text = text.trim();
    let text = text.strip_suffix(';').unwrap_or(text).trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

fn modifiers_node(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == "modifiers");
    found
}

/// The access keyword; without one, public in an interface or annotation
/// type (the language default), package-private elsewhere.
fn visibility(node: Node, source: &str) -> Visibility {
    let keyword = modifiers_node(node).and_then(|m| {
        let mut cursor = m.walk();
        let found = m
            .children(&mut cursor)
            .map(|c| slice(c, source))
            .find(|k| matches!(*k, "public" | "protected" | "private"));
        found
    });
    match keyword {
        Some("public") => Visibility::Public,
        Some("protected") => Visibility::Protected,
        Some(_) => Visibility::Private,
        None if in_interface(node) => Visibility::Public,
        None => Visibility::Package,
    }
}

fn in_interface(node: Node) -> bool {
    node.parent()
        .is_some_and(|p| matches!(p.kind(), "interface_body" | "annotation_type_body"))
}

fn modifiers(node: Node, source: &str) -> Vec<Modifier> {
    let Some(modifiers) = modifiers_node(node) else {
        return Vec::new();
    };
    let mut cursor = modifiers.walk();
    let found = modifiers
        .children(&mut cursor)
        .filter_map(|c| match c.kind() {
            "abstract" => Some(Modifier::Abstract),
            "static" => Some(Modifier::Static),
            "final" => Some(Modifier::Final),
            "sealed" => Some(Modifier::Sealed),
            "marker_annotation" | "annotation"
                if c.child_by_field_name("name")
                    .is_some_and(|n| slice(n, source) == "Override") =>
            {
                Some(Modifier::Override)
            }
            _ => None,
        })
        .collect();
    found
}

fn generics(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("type_parameters")
        .map(|g| slice(g, source).to_string())
}

fn parameters(list: Node, source: &str) -> Vec<Parameter> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter_map(|param| {
            let (name, type_node, kind) = match param.kind() {
                "formal_parameter" => (
                    param.child_by_field_name("name"),
                    param.child_by_field_name("type"),
                    ParameterKind::Positional,
                ),
                "spread_parameter" => {
                    let mut inner = param.walk();
                    let children: Vec<_> = param.named_children(&mut inner).collect();
                    (
                        children
                            .iter()
                            .find(|c| c.kind() == "variable_declarator")
                            .and_then(|d| d.child_by_field_name("name")),
                        children
                            .iter()
                            .find(|c| !matches!(c.kind(), "modifiers" | "variable_declarator"))
                            .copied(),
                        ParameterKind::Variadic,
                    )
                }
                "receiver_parameter" => {
                    let mut inner = param.walk();
                    let ty = param.named_children(&mut inner).find(|c| {
                        !matches!(c.kind(), "this" | "identifier")
                            && !c.kind().ends_with("annotation")
                    });
                    return Some(Parameter {
                        name: Some("this".to_string()),
                        type_text: ty.map(|t| TypeText::syntax(slice(t, source))),
                        default: None,
                        kind: ParameterKind::Receiver,
                    });
                }
                _ => return None,
            };
            Some(Parameter {
                name: name.map(|n| slice(n, source).to_string()),
                type_text: type_node.map(|t| TypeText::syntax(slice(t, source))),
                default: None,
                kind,
            })
        })
        .collect()
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
