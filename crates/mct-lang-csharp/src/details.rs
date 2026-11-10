//! Declaration details (ADR-034) read from a C# declaration's own syntax:
//! its header without attributes or body, parameters (an extension method's
//! `this` parameter as the receiver, `params`, defaults), the return type,
//! type parameters, visibility and modifiers. Nothing is inferred.

use mct_core::{Modifier, Parameter, ParameterKind, SymbolDetail, SymbolId, TypeText, Visibility};
use tree_sitter::Node;

/// A method, constructor, destructor, operator, conversion operator, local
/// function or delegate: the header up to its body, `;` or `: base(…)`.
pub(crate) fn method_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = first_start(
        node,
        &[
            "block",
            "arrow_expression_clause",
            "constructor_initializer",
            ";",
        ],
    );
    let parameters = node.child_by_field_name("parameters");
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: parameters.is_some(),
        parameters: parameters.map_or_else(Vec::new, |p| parameter_list(p, source)),
        return_type: node
            .child_by_field_name("returns")
            .or_else(|| node.child_by_field_name("type"))
            .map(|t| TypeText::syntax(slice(t, source))),
        visibility: visibility(node, source),
        modifiers: modifiers(node, source),
        generics: generics(node, source),
    }
}

/// A class, struct, interface, record or enum: the header up to its body
/// (a positional record's parameter list is part of it).
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or_else(|| first_start(node, &[";"]), |b| b.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: visibility(node, source),
        modifiers: modifiers(node, source),
        generics: generics(node, source),
        ..Default::default()
    }
}

/// A property, indexer or event with accessors: the header without its
/// accessors, expression body or initializer. An indexer's parameters are
/// recorded.
pub(crate) fn member_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = first_start(
        node,
        &["accessor_list", "arrow_expression_clause", "=", ";"],
    );
    let parameters = node.child_by_field_name("parameters");
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: parameters.is_some(),
        parameters: parameters.map_or_else(Vec::new, |p| parameter_list(p, source)),
        visibility: visibility(node, source),
        modifiers: modifiers(node, source),
        ..Default::default()
    }
}

/// One declarator of a field or event field declaration (`int a, b;`):
/// the shared header up to the type, then the declarator's name.
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
    let head = declarator
        .parent()
        .and_then(|variables| variables.child_by_field_name("type"))
        .and_then(|ty| header(declaration, ty.end_byte(), source));
    SymbolDetail {
        symbol,
        signature: head.map(|head| format!("{head} {name}")),
        visibility: visibility(declaration, source),
        modifiers: modifiers(declaration, source),
        ..Default::default()
    }
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// The start of `node`'s first child of one of `kinds`, else its end.
fn first_start(node: Node, kinds: &[&str]) -> usize {
    children(node)
        .into_iter()
        .find(|c| kinds.contains(&c.kind()))
        .map_or(node.end_byte(), |c| c.start_byte())
}

/// `node`'s text up to `end`, from its first child that is not an
/// attribute list or comment (attributes precede the modifiers).
fn header(node: Node, end: usize, source: &str) -> Option<String> {
    let start = children(node)
        .into_iter()
        .find(|c| !matches!(c.kind(), "attribute_list" | "comment"))
        .map_or(node.start_byte(), |c| c.start_byte());
    let text = source.get(start..end.max(start))?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn modifier_words<'a>(node: Node, source: &'a str) -> Vec<&'a str> {
    children(node)
        .into_iter()
        .filter(|c| c.kind() == "modifier")
        .map(|m| slice(m, source))
        .collect()
}

/// The access keywords; `protected internal` and `private protected` are
/// a restricted scope. Without one, the language default for where the
/// declaration sits: public in an interface, private in a class, struct or
/// record, internal in a namespace or at top level. An explicit interface
/// implementation, a destructor, a static constructor and a local function
/// take no access keyword and record none.
fn visibility(node: Node, source: &str) -> Option<Visibility> {
    let words = modifier_words(node, source);
    let access: Vec<_> = words
        .iter()
        .copied()
        .filter(|w| matches!(*w, "public" | "private" | "protected" | "internal" | "file"))
        .collect();
    match access.as_slice() {
        ["public"] => return Some(Visibility::Public),
        ["private"] => return Some(Visibility::Private),
        ["protected"] => return Some(Visibility::Protected),
        ["internal"] => return Some(Visibility::Internal),
        ["file"] => return Some(Visibility::FilePrivate),
        [_, _] => return Some(Visibility::Restricted),
        _ => {}
    }
    let takes_none = match node.kind() {
        "destructor_declaration" | "local_function_statement" => true,
        "constructor_declaration" => words.contains(&"static"),
        _ => children(node)
            .iter()
            .any(|c| c.kind() == "explicit_interface_specifier"),
    };
    if takes_none {
        return None;
    }
    let parent = node.parent()?;
    match parent.kind() {
        "compilation_unit" => Some(Visibility::Internal),
        "declaration_list" => match parent.parent()?.kind() {
            "interface_declaration" => Some(Visibility::Public),
            "namespace_declaration" => Some(Visibility::Internal),
            "class_declaration" | "struct_declaration" | "record_declaration" => {
                Some(Visibility::Private)
            }
            _ => None,
        },
        _ => None,
    }
}

fn modifiers(node: Node, source: &str) -> Vec<Modifier> {
    modifier_words(node, source)
        .into_iter()
        .filter_map(|word| match word {
            "abstract" => Some(Modifier::Abstract),
            "async" => Some(Modifier::Async),
            "const" => Some(Modifier::Const),
            "extern" => Some(Modifier::Extern),
            "sealed" => Some(Modifier::Sealed),
            "static" => Some(Modifier::Static),
            "override" => Some(Modifier::Override),
            "virtual" => Some(Modifier::Virtual),
            "readonly" => Some(Modifier::Readonly),
            "unsafe" => Some(Modifier::Unsafe),
            _ => None,
        })
        .collect()
}

fn generics(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("type_parameters")
        .or_else(|| {
            children(node)
                .into_iter()
                .find(|c| c.kind() == "type_parameter_list")
        })
        .map(|g| slice(g, source).to_string())
}

/// `(…)` or an indexer's `[…]`. A `params` array's type and name are
/// children of the list itself in this grammar, right after `params`.
fn parameter_list(list: Node, source: &str) -> Vec<Parameter> {
    let mut out = Vec::new();
    let mut variadic: Option<Parameter> = None;
    let mut cursor = list.walk();
    for (i, child) in list.children(&mut cursor).enumerate() {
        let field = u32::try_from(i)
            .ok()
            .and_then(|i| list.field_name_for_child(i));
        match (child.kind(), field) {
            ("parameter", _) => out.push(parameter(child, source)),
            ("params", _) => {
                variadic = Some(Parameter {
                    name: None,
                    type_text: None,
                    default: None,
                    kind: ParameterKind::Variadic,
                });
            }
            (_, Some("type")) => {
                if let Some(p) = variadic.as_mut() {
                    p.type_text = Some(TypeText::syntax(slice(child, source)));
                }
            }
            (_, Some("name")) => {
                if let Some(mut p) = variadic.take() {
                    p.name = Some(slice(child, source).to_string());
                    out.push(p);
                }
            }
            _ => {}
        }
    }
    out.extend(variadic);
    out
}

fn parameter(node: Node, source: &str) -> Parameter {
    let words = modifier_words(node, source);
    let mut default = None;
    let mut after_equals = false;
    for child in children(node) {
        if child.kind() == "=" {
            after_equals = true;
        } else if after_equals && child.is_named() {
            default = Some(slice(child, source).to_string());
            break;
        }
    }
    Parameter {
        name: node
            .child_by_field_name("name")
            .map(|n| slice(n, source).to_string()),
        type_text: node
            .child_by_field_name("type")
            .map(|t| TypeText::syntax(slice(t, source))),
        default,
        kind: if words.contains(&"this") {
            ParameterKind::Receiver
        } else if words.contains(&"params") {
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
