//! Declaration details (ADR-034) read from a Python definition's own syntax:
//! its header, parameters with annotations, defaults and kinds, the method
//! receiver, the return annotation, PEP 695 type parameters, and modifiers
//! from `async` and the standard decorators. Python has no visibility syntax,
//! so none is recorded; docstring types are not read.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText};
use tree_sitter::Node;

/// A `function_definition`; `in_class` when it sits directly in a class body.
pub(crate) fn function_detail(
    node: Node,
    in_class: bool,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let decorators = decorators(node, source);
    let mut modifiers: Vec<Modifier> = decorators.iter().filter_map(|d| modifier(d)).collect();
    if node.child(0).is_some_and(|c| c.kind() == "async") {
        modifiers.push(Modifier::Async);
    }
    let static_method = decorators.iter().any(|d| d == "staticmethod");
    let mut parameters = node
        .child_by_field_name("parameters")
        .map(|list| parameters(list, source))
        .unwrap_or_default();
    // A method's first parameter is bound to the instance (or the class,
    // under `@classmethod`), whatever its name; a `@staticmethod` has none.
    if in_class && !static_method {
        if let Some(first) = parameters
            .first_mut()
            .filter(|p| p.kind == ParameterKind::Positional)
        {
            first.kind = ParameterKind::Receiver;
        }
    }
    SymbolDetail {
        symbol,
        signature: header(node, source),
        parameters_known: node.child_by_field_name("parameters").is_some(),
        parameters,
        return_type: node
            .child_by_field_name("return_type")
            .map(|t| TypeText::syntax(slice(t, source))),
        modifiers,
        generics: generics(node, source),
        ..Default::default()
    }
}

/// A `lambda` bound to one name by `assignment`; `in_class` when the
/// assignment sits in a class body. The signature is the binding up to the
/// lambda's body (`double = lambda x`).
pub(crate) fn lambda_detail(
    assignment: Node,
    lambda: Node,
    in_class: bool,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let list = lambda.child_by_field_name("parameters");
    let mut parameters = list.map(|l| parameters(l, source)).unwrap_or_default();
    if in_class {
        if let Some(first) = parameters
            .first_mut()
            .filter(|p| p.kind == ParameterKind::Positional)
        {
            first.kind = ParameterKind::Receiver;
        }
    }
    let end = lambda
        .child_by_field_name("body")
        .map_or(lambda.end_byte(), |b| b.start_byte());
    let signature = source
        .get(assignment.start_byte()..end)
        .map(|t| t.trim_end().trim_end_matches(':').trim_end().to_string())
        .filter(|t| !t.is_empty());
    SymbolDetail {
        symbol,
        signature,
        parameters_known: true,
        parameters,
        ..Default::default()
    }
}

/// A `class_definition` or a PEP 695 `type_alias_statement`.
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let decorators = decorators(node, source);
    SymbolDetail {
        symbol,
        signature: header(node, source),
        modifiers: decorators.iter().filter_map(|d| modifier(d)).collect(),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// The definition up to its body, without the closing `:`.
fn header(node: Node, source: &str) -> Option<String> {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    let text = source.get(node.start_byte()..end)?.trim_end();
    let text = text.strip_suffix(':').unwrap_or(text).trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

fn generics(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("type_parameters")
        .or_else(|| {
            // `type Alias[T] = …` keeps its parameters on the left side.
            node.child_by_field_name("left")
                .and_then(|l| l.named_child(0))
                .and_then(|g| {
                    g.child_by_field_name("type_parameters")
                        .or(g.named_child(1))
                })
                .filter(|p| p.kind() == "type_parameter")
        })
        .map(|g| slice(g, source).to_string())
}

/// The last name of each decorator (`abc.abstractmethod` → `abstractmethod`)
/// on a definition wrapped by a `decorated_definition`.
fn decorators(node: Node, source: &str) -> Vec<String> {
    let Some(wrapper) = node.parent().filter(|p| p.kind() == "decorated_definition") else {
        return Vec::new();
    };
    let mut cursor = wrapper.walk();
    wrapper
        .children(&mut cursor)
        .filter(|c| c.kind() == "decorator")
        .filter_map(|d| d.named_child(0))
        .map(|expr| {
            let expr = match expr.kind() {
                "call" => expr.child_by_field_name("function").unwrap_or(expr),
                _ => expr,
            };
            let name = slice(expr, source);
            name.rsplit('.').next().unwrap_or(name).to_string()
        })
        .collect()
}

fn modifier(decorator: &str) -> Option<Modifier> {
    match decorator {
        "staticmethod" => Some(Modifier::Static),
        "abstractmethod" => Some(Modifier::Abstract),
        "override" => Some(Modifier::Override),
        "final" => Some(Modifier::Final),
        _ => None,
    }
}

fn parameters(list: Node, source: &str) -> Vec<Parameter> {
    let mut out = Vec::new();
    let mut keyword_only = false;
    let mut cursor = list.walk();
    for param in list.named_children(&mut cursor) {
        let positional = if keyword_only {
            ParameterKind::KeywordOnly
        } else {
            ParameterKind::Positional
        };
        let (pattern, type_text, default) = match param.kind() {
            "keyword_separator" => {
                keyword_only = true;
                continue;
            }
            "identifier" | "list_splat_pattern" | "dictionary_splat_pattern" => (param, None, None),
            "typed_parameter" => {
                let Some(pattern) = param.named_child(0) else {
                    continue;
                };
                (pattern, param.child_by_field_name("type"), None)
            }
            "default_parameter" | "typed_default_parameter" => {
                let Some(name) = param.child_by_field_name("name") else {
                    continue;
                };
                (
                    name,
                    param.child_by_field_name("type"),
                    param.child_by_field_name("value"),
                )
            }
            _ => continue,
        };
        let (name, kind) = match pattern.kind() {
            "identifier" => (Some(pattern), positional),
            "list_splat_pattern" => {
                keyword_only = true;
                (pattern.named_child(0), ParameterKind::Variadic)
            }
            "dictionary_splat_pattern" => (pattern.named_child(0), ParameterKind::KeywordVariadic),
            _ => (None, positional),
        };
        out.push(Parameter {
            name: name
                .filter(|n| n.kind() == "identifier")
                .map(|n| slice(n, source).to_string()),
            type_text: type_text.map(|t| TypeText::syntax(slice(t, source))),
            default: default.map(|d| slice(d, source).to_string()),
            kind,
        });
    }
    out
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
