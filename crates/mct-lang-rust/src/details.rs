//! Declaration details (ADR-034) read from a Rust item's own syntax: its
//! header, parameters, return type, visibility, modifiers and generics.
//! Nothing here is inferred: a field the syntax does not state stays `None`.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// The detail of `item` (a `function_item`, `struct_item`, `trait_item`,
/// `type_item`, `const_item`, …) recorded as symbol `symbol`.
pub(crate) fn item_detail(item: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let is_function = matches!(item.kind(), "function_item" | "function_signature_item");
    let mut detail = SymbolDetail {
        symbol,
        signature: header(item, source),
        visibility: visibility(item, source),
        modifiers: modifiers(item),
        generics: item
            .child_by_field_name("type_parameters")
            .map(|g| slice(g, source).to_string()),
        ..Default::default()
    };
    if is_function {
        detail.return_type = item
            .child_by_field_name("return_type")
            .map(|t| TypeText::syntax(slice(t, source)));
        if let Some(params) = item.child_by_field_name("parameters") {
            detail.parameters = parameters(params, source);
            detail.parameters_known = true;
        }
    }
    detail
}

/// The item as written up to its body (`{ … }`, a const's `= value`) or its
/// closing `;`. Attributes and doc comments are sibling nodes, so they are
/// never part of it.
fn header(item: Node, source: &str) -> Option<String> {
    let end = ["body", "value"]
        .iter()
        .find_map(|field| item.child_by_field_name(field))
        // A tuple struct's fields are its declaration, not a body.
        .filter(|body| body.kind() != "ordered_field_declaration_list")
        .map_or(item.end_byte(), |body| body.start_byte());
    let text = source.get(item.start_byte()..end)?.trim_end();
    let text = text.strip_suffix(';').unwrap_or(text).trim_end();
    let text = text.strip_suffix('=').unwrap_or(text).trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

/// What the item's `pub…` says. Without one, an item is private to its
/// module, except inside a trait or a trait implementation, where it is as
/// visible as the trait — which the item's own syntax does not say.
fn visibility(item: Node, source: &str) -> Option<Visibility> {
    let mut cursor = item.walk();
    let keyword = item
        .children(&mut cursor)
        .find(|c| c.kind() == "visibility_modifier");
    match keyword {
        Some(keyword) => Some(
            match slice(keyword, source)
                .split_whitespace()
                .collect::<String>()
                .as_str()
            {
                "pub" => Visibility::Public,
                "pub(crate)" | "crate" => Visibility::Crate,
                "pub(self)" => Visibility::Private,
                _ => Visibility::Restricted,
            },
        ),
        None if in_trait_scope(item) => None,
        None => Some(Visibility::Private),
    }
}

/// Whether `item` sits in a `trait` body or an `impl Trait for T` body.
fn in_trait_scope(item: Node) -> bool {
    let Some(owner) = item
        .parent()
        .filter(|p| p.kind() == "declaration_list")
        .and_then(|list| list.parent())
    else {
        return false;
    };
    match owner.kind() {
        "trait_item" => true,
        "impl_item" => owner.child_by_field_name("trait").is_some(),
        _ => false,
    }
}

/// `async`, `const`, `unsafe` and `extern "…"` of a function, and `unsafe`
/// of a trait.
fn modifiers(item: Node) -> Vec<Modifier> {
    let mut found = Vec::new();
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        match child.kind() {
            "function_modifiers" => {
                let mut inner = child.walk();
                found.extend(
                    child
                        .children(&mut inner)
                        .filter_map(|m| modifier(m.kind())),
                );
            }
            "unsafe" => found.push(Modifier::Unsafe),
            _ => {}
        }
    }
    found
}

fn modifier(kind: &str) -> Option<Modifier> {
    match kind {
        "async" => Some(Modifier::Async),
        "const" => Some(Modifier::Const),
        "unsafe" => Some(Modifier::Unsafe),
        "extern_modifier" => Some(Modifier::Extern),
        _ => None,
    }
}

fn parameters(list: Node, source: &str) -> Vec<Parameter> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter_map(|param| parameter(param, source))
        .collect()
}

fn parameter(param: Node, source: &str) -> Option<Parameter> {
    match param.kind() {
        "self_parameter" => Some(Parameter {
            name: Some("self".to_string()),
            type_text: Some(TypeText::syntax(receiver_type(slice(param, source)))),
            default: None,
            kind: ParameterKind::Receiver,
        }),
        "parameter" => {
            let pattern = param.child_by_field_name("pattern");
            let is_self = pattern.is_some_and(|p| p.kind() == "self");
            Some(Parameter {
                name: pattern
                    .filter(|p| matches!(p.kind(), "identifier" | "self"))
                    .map(|p| slice(p, source).to_string()),
                type_text: param
                    .child_by_field_name("type")
                    .map(|t| TypeText::syntax(slice(t, source))),
                default: None,
                kind: if is_self {
                    ParameterKind::Receiver
                } else {
                    ParameterKind::Positional
                },
            })
        }
        "variadic_parameter" => Some(Parameter {
            name: param
                .child_by_field_name("pattern")
                .filter(|p| p.kind() == "identifier")
                .map(|p| slice(p, source).to_string()),
            type_text: None,
            default: None,
            kind: ParameterKind::Variadic,
        }),
        _ => None,
    }
}

/// The type a shorthand receiver stands for: `&'a mut self` is
/// `&'a mut Self`, and `mut self` (a mutable binding) is `Self`.
fn receiver_type(written: &str) -> String {
    let written = written.trim();
    if let Some((_, ty)) = written.split_once(':') {
        return ty.trim().to_string();
    }
    let without_binding = written.strip_prefix("mut ").unwrap_or(written).trim_start();
    match without_binding.strip_suffix("self") {
        Some(prefix) => format!("{prefix}Self"),
        None => without_binding.to_string(),
    }
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::receiver_type;

    #[test]
    fn a_shorthand_receiver_names_the_self_type() {
        assert_eq!(receiver_type("self"), "Self");
        assert_eq!(receiver_type("mut self"), "Self");
        assert_eq!(receiver_type("&self"), "&Self");
        assert_eq!(receiver_type("&'a mut self"), "&'a mut Self");
        assert_eq!(receiver_type("self: Box<Self>"), "Box<Self>");
    }
}
