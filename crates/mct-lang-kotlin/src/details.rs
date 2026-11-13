//! Declaration details (ADR-034) read from a Kotlin declaration's own syntax:
//! its header without annotations or body, parameters (an extension's
//! receiver type as `this`, `vararg`, defaults), the return type, type
//! parameters, visibility and modifiers. Nothing is inferred: an
//! expression-bodied function without a written return type has none.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// Kinds of a written type in this grammar.
const TYPE_KINDS: &[&str] = &[
    "user_type",
    "nullable_type",
    "function_type",
    "parenthesized_type",
    "non_nullable_type",
    "definitely_non_nullable_type",
];

pub(crate) fn function_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let children = children(node);
    let name_at = children
        .iter()
        .position(|c| Some(c.id()) == node.child_by_field_name("name").map(|n| n.id()));
    let params_at = children
        .iter()
        .position(|c| c.kind() == "function_value_parameters");
    let mut parameters = Vec::new();
    // `fun String.shout()`: the type before the name is `this` in the body.
    if let Some(name_at) = name_at {
        if let Some(receiver) = children[..name_at]
            .iter()
            .rev()
            .find(|c| TYPE_KINDS.contains(&c.kind()))
        {
            parameters.push(Parameter {
                name: Some("this".to_string()),
                type_text: Some(TypeText::syntax(slice(*receiver, source))),
                default: None,
                kind: ParameterKind::Receiver,
            });
        }
    }
    if let Some(at) = params_at {
        parameters.extend(value_parameters(children[at], source));
    }
    let return_type = params_at.and_then(|at| {
        children[at + 1..]
            .iter()
            .take_while(|c| !matches!(c.kind(), "function_body" | "type_constraints"))
            .find(|c| TYPE_KINDS.contains(&c.kind()))
            .map(|t| TypeText::syntax(slice(*t, source)))
    });
    let end = children
        .iter()
        .find(|c| c.kind() == "function_body")
        .map_or(node.end_byte(), |b| b.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: params_at.is_some(),
        parameters,
        return_type,
        visibility: visibility(node, source),
        modifiers: modifiers(node),
        generics: generics(node, source),
    }
}

/// A class, interface or object: the header up to its body.
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = children(node)
        .into_iter()
        .find(|c| matches!(c.kind(), "class_body" | "enum_class_body"))
        .map_or(node.end_byte(), |b| b.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: visibility(node, source),
        modifiers: modifiers(node),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// A member `property_declaration`, or a `val`/`var` constructor parameter:
/// the header without its initializer, accessors or delegate.
pub(crate) fn property_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = children(node)
        .into_iter()
        .find(|c| matches!(c.kind(), "=" | "getter" | "setter" | "property_delegate"))
        .map_or(node.end_byte(), |c| c.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: visibility(node, source),
        modifiers: modifiers(node),
        ..Default::default()
    }
}

/// A property bound to a lambda or an anonymous function: the binding up
/// to the body (through `->` for a lambda with parameters, the bare binding
/// for one without), and the parameters it declares. A lambda that writes
/// none may still take `it`, so its parameters are unknown.
pub(crate) fn lambda_detail(
    node: Node,
    lambda: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let parts = children(lambda);
    let (end, parameters) = if lambda.kind() == "anonymous_function" {
        let end = parts
            .iter()
            .find(|c| c.kind() == "function_body")
            .map_or(lambda.end_byte(), |c| c.start_byte());
        let parameters = parts
            .iter()
            .find(|c| c.kind() == "function_value_parameters")
            .map(|list| value_parameters(*list, source));
        (end, parameters)
    } else {
        let arrow = parts.iter().find(|c| c.kind() == "->");
        let parameters = parts
            .iter()
            .find(|c| c.kind() == "lambda_parameters")
            .map(|list| lambda_parameters(*list, source));
        match arrow {
            Some(arrow) => (arrow.end_byte(), parameters),
            None => {
                let binding_end = children(node)
                    .into_iter()
                    .find(|c| c.kind() == "=")
                    .map_or(node.end_byte(), |c| c.start_byte());
                (binding_end, None)
            }
        }
    };
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: parameters.is_some(),
        parameters: parameters.unwrap_or_default(),
        visibility: visibility(node, source),
        modifiers: modifiers(node),
        ..Default::default()
    }
}

/// `x: Int, y` of a lambda: each name with its type when written.
fn lambda_parameters(list: Node, source: &str) -> Vec<Parameter> {
    children(list)
        .into_iter()
        .filter(|c| c.kind() == "variable_declaration")
        .map(|declaration| {
            let parts = children(declaration);
            Parameter {
                name: parts
                    .iter()
                    .find(|p| p.kind() == "identifier")
                    .map(|n| slice(*n, source).to_string()),
                type_text: parts
                    .iter()
                    .find(|p| TYPE_KINDS.contains(&p.kind()))
                    .map(|t| TypeText::syntax(slice(*t, source))),
                default: None,
                kind: ParameterKind::Positional,
            }
        })
        .collect()
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn modifiers_node(node: Node) -> Option<Node> {
    children(node).into_iter().find(|c| c.kind() == "modifiers")
}

/// `node`'s text up to `end`, without the annotations in its modifiers.
fn header(node: Node, end: usize, source: &str) -> Option<String> {
    let skipped: Vec<_> = modifiers_node(node)
        .map(|m| {
            children(m)
                .into_iter()
                .filter(|c| c.kind().contains("annotation") || c.kind().ends_with("comment"))
                .map(|c| c.byte_range())
                .collect()
        })
        .unwrap_or_default();
    let mut text = String::new();
    let mut at = node.start_byte();
    for range in skipped {
        text.push_str(source.get(at..range.start.min(end))?);
        text.push(' ');
        at = range.end;
    }
    text.push_str(source.get(at.min(end)..end)?);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The visibility keyword; without one, public (Kotlin's default) for a
/// top-level or member declaration. A local declaration has none.
fn visibility(node: Node, source: &str) -> Option<Visibility> {
    let keyword = modifiers_node(node).and_then(|m| {
        children(m)
            .into_iter()
            .find(|c| c.kind() == "visibility_modifier")
            .map(|v| slice(v, source).to_string())
    });
    match keyword.as_deref() {
        Some("private") => Some(Visibility::Private),
        Some("protected") => Some(Visibility::Protected),
        Some("internal") => Some(Visibility::Internal),
        Some(_) => Some(Visibility::Public),
        None => {
            let declared_at = node.parent().map(|p| p.kind());
            matches!(
                declared_at,
                Some("source_file" | "class_body" | "enum_class_body" | "class_parameters")
            )
            .then_some(Visibility::Public)
        }
    }
}

fn modifiers(node: Node) -> Vec<Modifier> {
    let Some(modifiers) = modifiers_node(node) else {
        return Vec::new();
    };
    children(modifiers)
        .into_iter()
        .filter_map(|group| group.child(0))
        .filter_map(|keyword| match keyword.kind() {
            "abstract" => Some(Modifier::Abstract),
            "open" => Some(Modifier::Open),
            "final" => Some(Modifier::Final),
            "sealed" => Some(Modifier::Sealed),
            "override" => Some(Modifier::Override),
            "const" => Some(Modifier::Const),
            "inline" => Some(Modifier::Inline),
            "external" => Some(Modifier::Extern),
            _ => None,
        })
        .collect()
}

fn generics(node: Node, source: &str) -> Option<String> {
    children(node)
        .into_iter()
        .find(|c| c.kind() == "type_parameters")
        .map(|g| slice(g, source).to_string())
}

/// The grammar keeps a parameter's `vararg` and `= default` as siblings of
/// the `parameter` node, before and after it.
fn value_parameters(list: Node, source: &str) -> Vec<Parameter> {
    let mut out: Vec<Parameter> = Vec::new();
    let mut vararg = false;
    let mut default_next = false;
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        match child.kind() {
            "parameter_modifiers" => {
                vararg |= slice(child, source)
                    .split_whitespace()
                    .any(|m| m == "vararg");
            }
            "parameter" => {
                let mut inner = child.walk();
                let parts: Vec<_> = child.named_children(&mut inner).collect();
                out.push(Parameter {
                    name: parts
                        .iter()
                        .find(|p| p.kind() == "identifier")
                        .map(|n| slice(*n, source).to_string()),
                    type_text: parts
                        .iter()
                        .find(|p| TYPE_KINDS.contains(&p.kind()))
                        .map(|t| TypeText::syntax(slice(*t, source))),
                    default: None,
                    kind: if vararg {
                        ParameterKind::Variadic
                    } else {
                        ParameterKind::Positional
                    },
                });
                vararg = false;
            }
            "=" => default_next = true,
            "," | "(" | ")" => default_next = false,
            _ if default_next && child.is_named() => {
                if let Some(last) = out.last_mut() {
                    last.default = Some(slice(child, source).to_string());
                }
                default_next = false;
            }
            _ => {}
        }
    }
    out
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
