//! Declaration details (ADR-034) read from a C++ declaration's own syntax:
//! its header without attributes or body (with the enclosing `template
//! <…>`), parameters, the return type, template parameters, visibility and
//! modifiers. A type is everything a declaration writes but the name, so
//! `const char* fmt` has the type `const char*`. Nothing is inferred: an
//! `auto` return without a trailing `-> T` stays `auto`.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// A function or method, defined (`function_definition`) or only declared
/// (`declaration`/`field_declaration`); `function` is its
/// `function_declarator`.
pub(crate) fn function_detail(
    node: Node,
    function: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = children(node)
        .into_iter()
        .find(|c| {
            matches!(
                c.kind(),
                "compound_statement"
                    | "field_initializer_list"
                    | "try_statement"
                    | "default_method_clause"
                    | "delete_method_clause"
                    | ";"
            )
        })
        .map_or(node.end_byte(), |c| c.start_byte());
    let parameters = function.child_by_field_name("parameters");
    let mut modifiers = modifiers(node, source);
    modifiers.extend(virtual_specifiers(function, source));
    let pure = node.kind() == "field_declaration"
        && node
            .child_by_field_name("default_value")
            .is_some_and(|v| slice(v, source) == "0");
    if pure {
        modifiers.push(Modifier::Abstract);
    }
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: parameters.is_some(),
        parameters: parameters.map_or_else(Vec::new, |p| parameter_list(p, source)),
        return_type: return_type(node, function, source),
        visibility: visibility(node, source),
        modifiers,
        generics: generics(node, source),
    }
}

/// A class, struct, union or enum definition: the header up to its body.
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    // The specifier's own text starts at its class key; the declaration
    // that holds it (attributes, `template <…>`) is its parent.
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: visibility(node, source),
        modifiers: virtual_specifiers(node, source),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// `using Name = Type;` or `typedef Type A, *B;`: the declaration without
/// its `;`, shared by every name a `typedef` declares.
pub(crate) fn alias_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = children(node)
        .into_iter()
        .find(|c| c.kind() == ";")
        .map_or(node.end_byte(), |c| c.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: visibility(node, source),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// One declarator of a variable or field declaration (`int a = 1, b;`):
/// the shared specifiers and type, then the declarator without its
/// initializer.
pub(crate) fn variable_detail(
    node: Node,
    declarator: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let first = node
        .child_by_field_name("declarator")
        .map_or(declarator.start_byte(), |d| d.start_byte());
    let own = if declarator.kind() == "init_declarator" {
        declarator
            .child_by_field_name("declarator")
            .unwrap_or(declarator)
    } else {
        declarator
    };
    SymbolDetail {
        symbol,
        signature: header(node, first, source).map(|head| format!("{head} {}", slice(own, source))),
        visibility: visibility(node, source),
        modifiers: modifiers(node, source),
        ..Default::default()
    }
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// The declaration a member list or namespace holds: a templated one is
/// wrapped in its `template_declaration`.
fn outermost(node: Node) -> Node {
    match node.parent() {
        Some(p) if p.kind() == "template_declaration" => p,
        _ => node,
    }
}

/// `node`'s text up to `end`, from its `template <…>` if it has one, else
/// from its first child that is not an attribute or comment.
fn header(node: Node, end: usize, source: &str) -> Option<String> {
    let start = if outermost(node).id() == node.id() {
        children(node)
            .into_iter()
            .find(|c| !matches!(c.kind(), "attribute_declaration" | "comment"))
            .map_or(node.start_byte(), |c| c.start_byte())
    } else {
        outermost(node).start_byte()
    };
    let text = source.get(start..end.max(start))?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn generics(node: Node, source: &str) -> Option<String> {
    let template = outermost(node);
    (template.id() != node.id())
        .then(|| template.child_by_field_name("parameters"))
        .flatten()
        .map(|p| slice(p, source).to_string())
}

/// The access label in force in a class body (the last one before the
/// member), else the class key's default: private in a `class`, public in
/// a `struct` or `union`. Outside a class C++ has linkage, not access, and
/// an out-of-line definition's access is its declaration's: none.
fn visibility(node: Node, source: &str) -> Option<Visibility> {
    // A nested class definition is the type of a `field_declaration`.
    let declared_by = node
        .parent()
        .filter(|p| p.kind() == "field_declaration" && node.kind().ends_with("_specifier"))
        .unwrap_or(node);
    let member = outermost(declared_by);
    let list = member
        .parent()
        .filter(|p| p.kind() == "field_declaration_list")?;
    let mut sibling = member.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "access_specifier" {
            return match slice(s, source) {
                "private" => Some(Visibility::Private),
                "protected" => Some(Visibility::Protected),
                _ => Some(Visibility::Public),
            };
        }
        sibling = s.prev_sibling();
    }
    match list.parent()?.kind() {
        "class_specifier" => Some(Visibility::Private),
        _ => Some(Visibility::Public),
    }
}

fn modifiers(node: Node, source: &str) -> Vec<Modifier> {
    children(node)
        .into_iter()
        .filter_map(|c| match (c.kind(), slice(c, source)) {
            ("virtual", _) => Some(Modifier::Virtual),
            ("storage_class_specifier", "static") => Some(Modifier::Static),
            ("storage_class_specifier", "extern") => Some(Modifier::Extern),
            ("storage_class_specifier", "inline") => Some(Modifier::Inline),
            ("type_qualifier", "constexpr") => Some(Modifier::Const),
            _ => None,
        })
        .collect()
}

/// `override` and `final` after a function's parameters or a class name.
fn virtual_specifiers(node: Node, source: &str) -> Vec<Modifier> {
    children(node)
        .into_iter()
        .filter(|c| c.kind() == "virtual_specifier")
        .filter_map(|c| match slice(c, source) {
            "override" => Some(Modifier::Override),
            "final" => Some(Modifier::Final),
            _ => None,
        })
        .collect()
}

/// The written return type: the specifiers' type plus any `*`/`&` between
/// it and the function's name, or a trailing `-> T` after `auto`. A
/// constructor, destructor or conversion has none.
fn return_type(node: Node, function: Node, source: &str) -> Option<TypeText> {
    let ty = node.child_by_field_name("type")?;
    if slice(ty, source) == "auto" {
        if let Some(trailing) = children(function)
            .into_iter()
            .find(|c| c.kind() == "trailing_return_type")
        {
            let written = trailing.named_child(0).map_or("", |t| slice(t, source));
            return (!written.is_empty()).then(|| TypeText::syntax(written));
        }
    }
    let before: Vec<_> = children(node)
        .into_iter()
        .take_while(|c| c.id() != ty.id())
        .collect();
    let start = before
        .into_iter()
        .rev()
        .take_while(|c| c.kind() == "type_qualifier" && slice(*c, source) != "constexpr")
        .last()
        .map_or(ty.start_byte(), |q| q.start_byte());
    let written = source.get(start..function.start_byte().max(start))?.trim();
    (!written.is_empty()).then(|| TypeText::syntax(written))
}

/// The name a declarator declares, through pointer, reference, array and
/// function declarators.
fn declared_name(declarator: Node) -> Option<Node> {
    match declarator.kind() {
        "identifier"
        | "field_identifier"
        | "qualified_identifier"
        | "destructor_name"
        | "operator_name" => Some(declarator),
        _ => declarator
            .child_by_field_name("declarator")
            .or_else(|| {
                let count = declarator.named_child_count();
                count
                    .checked_sub(1)
                    .and_then(|last| u32::try_from(last).ok())
                    .and_then(|last| declarator.named_child(last as usize))
            })
            .and_then(declared_name),
    }
}

fn parameter_list(list: Node, source: &str) -> Vec<Parameter> {
    let mut out = Vec::new();
    for child in children(list) {
        match child.kind() {
            // C varargs: no name, no type.
            "..." => out.push(Parameter {
                name: None,
                type_text: None,
                default: None,
                kind: ParameterKind::Variadic,
            }),
            "parameter_declaration"
            | "optional_parameter_declaration"
            | "variadic_parameter_declaration" => out.push(parameter(child, source)),
            _ => {}
        }
    }
    out
}

fn parameter(node: Node, source: &str) -> Parameter {
    let default = node.child_by_field_name("default_value");
    let declarator = node.child_by_field_name("declarator");
    let name = declarator.and_then(declared_name);
    let end = declarator.map_or_else(
        || {
            children(node)
                .into_iter()
                .find(|c| c.kind() == "=")
                .map_or(node.end_byte(), |c| c.start_byte())
        },
        |d| d.end_byte(),
    );
    let type_text = match name {
        Some(n) => format!(
            "{}{}",
            source
                .get(node.start_byte()..n.start_byte())
                .unwrap_or_default(),
            source
                .get(n.end_byte()..end.max(n.end_byte()))
                .unwrap_or_default()
        ),
        None => source
            .get(node.start_byte()..end)
            .unwrap_or_default()
            .to_string(),
    };
    let type_text = type_text.trim();
    let variadic = node.kind() == "variadic_parameter_declaration";
    Parameter {
        name: name.map(|n| slice(n, source).to_string()),
        type_text: (!type_text.is_empty()).then(|| TypeText::syntax(type_text)),
        default: default.map(|d| slice(d, source).to_string()),
        kind: if variadic {
            ParameterKind::Variadic
        } else {
            ParameterKind::Positional
        },
    }
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
