//! D5: the receiver types a function's own code states, for qualifying
//! `x.m()` by the type `x` is declared with.
//!
//! A local counts only when the function binds its name exactly once, as a
//! parameter typed `T`, `&T`, `&mut T` or `T<..>`, as `let x: T` or as
//! `let x = T { .. }`. Any second binding of the name (a shadowing `let`, a
//! closure parameter, a `match` arm) leaves it unproven, and so does a type
//! the caller rules out (a type parameter, a standard library type, which
//! may deref to another type's methods).
//!
//! A field reached from a proven receiver (`self.index`, `ledger.store`)
//! counts too, when this file declares the struct with that field typed
//! `T`, `&T` or `T<..>` ([`field_types`]).

use std::collections::HashMap;

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::text;

/// Local name → base name of the type its only binding in `function` states.
/// `self_type` stands in for `Self`; `rules_out(name, at)` rejects a type
/// name written at byte offset `at`.
pub(crate) fn stated_receivers(
    function: Node,
    source: &str,
    self_type: Option<&str>,
    rules_out: impl Fn(&str, usize) -> bool,
) -> HashMap<String, String> {
    let mut counts = HashMap::new();
    let mut stated = Vec::new();
    walk(function, source, &mut counts, &mut stated, 0);
    stated
        .into_iter()
        .filter(|(name, _)| counts.get(name) == Some(&1))
        .filter_map(|(name, ty)| {
            let base = type_name(ty, source, self_type)?;
            (!rules_out(&base, ty.start_byte())).then_some((name, base))
        })
        .collect()
}

/// Counts every name `node` binds and records each binding that states a
/// type, without descending into a nested `fn` (its own scope).
fn walk<'t>(
    node: Node<'t>,
    source: &str,
    counts: &mut HashMap<String, usize>,
    stated: &mut Vec<(String, Node<'t>)>,
    depth: u32,
) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if let Some(ty) = stated_type(node) {
        if let Some(name) = node.child_by_field_name("pattern").and_then(plain_binding) {
            stated.push((text(name, source).to_string(), ty));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_item" {
            continue;
        }
        let is_pattern = node
            .child_by_field_name("pattern")
            .is_some_and(|p| p.id() == child.id())
            || node.kind() == "closure_parameters";
        if is_pattern {
            count_identifiers(child, source, counts, depth + 1);
        } else {
            walk(child, source, counts, stated, depth + 1);
        }
    }
}

/// The type node a parameter or `let` states for its pattern: its `type`,
/// or for an untyped `let x = T { .. }` the struct expression's name.
fn stated_type(node: Node) -> Option<Node> {
    match node.kind() {
        "parameter" => node.child_by_field_name("type"),
        "let_declaration" => node.child_by_field_name("type").or_else(|| {
            node.child_by_field_name("value")
                .filter(|v| v.kind() == "struct_expression")
                .and_then(|v| v.child_by_field_name("name"))
        }),
        _ => None,
    }
}

/// The identifier of a pattern that is one plain name (`x`, `mut x`).
fn plain_binding(pattern: Node) -> Option<Node> {
    match pattern.kind() {
        "identifier" => Some(pattern),
        "mut_pattern" => pattern.named_child(0).filter(|n| n.kind() == "identifier"),
        _ => None,
    }
}

/// The base name of a stated type whose methods are its own: `T`, `&T`,
/// `&mut T`, `T<..>`, or `Self`. A path, tuple, slice, `dyn` or `impl` type
/// is none.
fn type_name(ty: Node, source: &str, self_type: Option<&str>) -> Option<String> {
    match ty.kind() {
        "type_identifier" => match text(ty, source) {
            "Self" => self_type.map(str::to_string),
            name => Some(name.to_string()),
        },
        "reference_type" => type_name(ty.child_by_field_name("type")?, source, self_type),
        "generic_type" => ty
            .child_by_field_name("type")
            .filter(|t| t.kind() == "type_identifier")
            .and_then(|t| type_name(t, source, self_type)),
        _ => None,
    }
}

fn count_identifiers(node: Node, source: &str, counts: &mut HashMap<String, usize>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if matches!(node.kind(), "identifier" | "shorthand_field_identifier") {
        *counts.entry(text(node, source).to_string()).or_default() += 1;
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_identifiers(child, source, counts, depth + 1);
    }
}

/// The stated type of each named field of the structs this file declares:
/// `(struct, field)` → the field type's base name and the byte offset it is
/// written at. A struct name declared twice in the file (in two modules)
/// gives nothing, and so does a field typed by one of its struct's own type
/// parameters.
pub(crate) fn field_types(root: Node, source: &str) -> HashMap<(String, String), (String, usize)> {
    let mut structs = Vec::new();
    collect_structs(root, &mut structs, 0);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for item in &structs {
        if let Some(name) = item.child_by_field_name("name") {
            *counts.entry(text(name, source)).or_default() += 1;
        }
    }
    let mut fields = HashMap::new();
    for item in structs {
        let Some(name) = item.child_by_field_name("name").map(|n| text(n, source)) else {
            continue;
        };
        if counts.get(name) != Some(&1) {
            continue;
        }
        let generics = type_parameters(item, source);
        let Some(body) = item
            .child_by_field_name("body")
            .filter(|b| b.kind() == "field_declaration_list")
        else {
            continue;
        };
        let mut cursor = body.walk();
        for field in body.named_children(&mut cursor) {
            if field.kind() != "field_declaration" {
                continue;
            }
            let (Some(field_name), Some(ty)) = (
                field.child_by_field_name("name"),
                field.child_by_field_name("type"),
            ) else {
                continue;
            };
            if let Some(base) = type_name(ty, source, Some(name)) {
                if !generics.contains(&base) {
                    fields.insert(
                        (name.to_string(), text(field_name, source).to_string()),
                        (base, ty.start_byte()),
                    );
                }
            }
        }
    }
    fields
}

/// Every `struct` item under `node`, inline modules included, function
/// bodies not (a struct declared in a function is local to it).
fn collect_structs<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "struct_item" => out.push(child),
            "mod_item" | "declaration_list" => collect_structs(child, out, depth + 1),
            _ => {}
        }
    }
}

/// The names of an item's type parameters (`T` in `struct S<T>`).
fn type_parameters(item: Node, source: &str) -> Vec<String> {
    let Some(params) = item.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter_map(|param| match param.kind() {
            "type_identifier" => Some(text(param, source).to_string()),
            _ => param
                .child_by_field_name("name")
                .or_else(|| param.named_child(0))
                .filter(|n| n.kind() == "type_identifier")
                .map(|n| text(n, source).to_string()),
        })
        .collect()
}
