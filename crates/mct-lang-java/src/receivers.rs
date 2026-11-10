//! Declared types of the names a method can see, so a call `x.m()` can be
//! qualified by `x`'s type instead of resolved by `m`'s name alone.
//!
//! Only types written in the source count: a parameter, local, `for`
//! variable, resource, `catch` parameter, pattern variable or field. A name
//! bound more than once with different types, or bound without a type
//! (`var`, an untyped lambda parameter), is unknown. So is a type variable
//! (`T`), an array or a primitive. Unknown receivers stay member calls.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::text;

/// Recursion cap for the binding scan, below the walker's own depth limit.
const MAX_SCAN_DEPTH: u32 = 200;

/// Name → declared simple type name, `None` when it is not provable.
pub(crate) type Bindings = HashMap<String, Option<String>>;

fn bind(map: &mut Bindings, name: &str, ty: Option<String>) {
    match map.get_mut(name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            map.insert(name.to_string(), ty);
        }
    }
}

/// The simple name of a declared reference type (`Id` for `Order.Id`,
/// `List` for `List<Order>`). `None` for `var`, primitives, arrays and the
/// type variables in `generics`.
pub(crate) fn type_name(node: Node, source: &str, generics: &[String]) -> Option<String> {
    match node.kind() {
        "type_identifier" => {
            let name = text(node, source);
            (name != "var" && !generics.iter().any(|g| g == name)).then(|| name.to_string())
        }
        "generic_type" => node
            .named_child(0)
            .and_then(|base| type_name(base, source, generics)),
        "scoped_type_identifier" => {
            let mut cursor = node.walk();
            let last = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "type_identifier")
                .last();
            last.map(|n| text(n, source).to_string())
        }
        "annotated_type" => {
            let mut cursor = node.walk();
            let inner = node
                .named_children(&mut cursor)
                .find(|c| !c.kind().ends_with("annotation"));
            inner.and_then(|n| type_name(n, source, generics))
        }
        _ => None,
    }
}

/// The type parameter names declared by `node` (`T` in `class R<T>` or
/// `<T> void f()`).
pub(crate) fn type_parameters(node: Node, source: &str) -> Vec<String> {
    let Some(params) = node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter(|p| p.kind() == "type_parameter")
        .filter_map(|p| {
            let mut c = p.walk();
            let name = p
                .named_children(&mut c)
                .find(|n| n.kind() == "type_identifier" || n.kind() == "identifier");
            name.map(|n| text(n, source).to_string())
        })
        .collect()
}

fn declarators(node: Node, source: &str, ty: Option<String>, map: &mut Bindings) {
    let mut cursor = node.walk();
    for declarator in node.named_children(&mut cursor) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = declarator.child_by_field_name("name") {
            // `int[] a` is an array whatever its type node says; so is `a[]`.
            let ty = if declarator.child_by_field_name("dimensions").is_some() {
                None
            } else {
                ty.clone()
            };
            bind(map, text(name, source), ty);
        }
    }
}

/// The fields of a class, enum, interface or record body, plus a record's
/// components.
pub(crate) fn fields(declaration: Node, source: &str, generics: &[String]) -> Bindings {
    let mut map = Bindings::new();
    if declaration.kind() == "record_declaration" {
        if let Some(params) = declaration.child_by_field_name("parameters") {
            locals(params, source, generics, &mut map, 0);
        }
    }
    if let Some(body) = declaration.child_by_field_name("body") {
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            if member.kind() == "field_declaration" || member.kind() == "constant_declaration" {
                let ty = member
                    .child_by_field_name("type")
                    .and_then(|t| type_name(t, source, generics));
                declarators(member, source, ty, &mut map);
            }
        }
    }
    map
}

/// Every name `node` binds anywhere inside it, nested lambdas and local or
/// anonymous classes included: a name reused with another type there makes
/// it unknown, which is the safe direction.
pub(crate) fn locals(
    node: Node,
    source: &str,
    generics: &[String],
    map: &mut Bindings,
    depth: u32,
) {
    if depth >= MAX_SCAN_DEPTH {
        return;
    }
    let typed = |n: Node| {
        n.child_by_field_name("type")
            .and_then(|t| type_name(t, source, generics))
    };
    let named = |n: Node| n.child_by_field_name("name").map(|n| text(n, source));
    match node.kind() {
        "formal_parameter" | "enhanced_for_statement" | "resource" => {
            if let Some(name) = named(node) {
                let ty = if node.child_by_field_name("dimensions").is_some() {
                    None
                } else {
                    typed(node)
                };
                bind(map, name, ty);
            }
        }
        "local_variable_declaration" => declarators(node, source, typed(node), map),
        "spread_parameter" => declarators(node, source, None, map),
        "catch_formal_parameter" => {
            if let Some(name) = named(node) {
                let mut cursor = node.walk();
                let ty = node
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "catch_type")
                    .filter(|c| c.named_child_count() == 1)
                    .and_then(|c| c.named_child(0))
                    .and_then(|t| type_name(t, source, generics));
                bind(map, name, ty);
            }
        }
        "type_pattern" | "record_pattern_component" => {
            let count = node.named_child_count();
            if count >= 2 {
                if let (Some(ty), Some(name)) =
                    (node.named_child(count - 2), node.named_child(count - 1))
                {
                    if name.kind() == "identifier" {
                        bind(map, text(name, source), type_name(ty, source, generics));
                    }
                }
            }
        }
        "instanceof_expression" => {
            if let Some(name) = named(node) {
                let ty = node
                    .child_by_field_name("right")
                    .and_then(|t| type_name(t, source, generics));
                bind(map, name, ty);
            }
        }
        "lambda_expression" => {
            if let Some(params) = node.child_by_field_name("parameters") {
                match params.kind() {
                    "identifier" => bind(map, text(params, source), None),
                    "inferred_parameters" => {
                        let mut cursor = params.walk();
                        for p in params.named_children(&mut cursor) {
                            bind(map, text(p, source), None);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        locals(child, source, generics, map, depth + 1);
    }
}
