//! Declared types of the names a method call's receiver can be, read from
//! the syntax alone: a parameter, a typed local, `var x = new T()`, a
//! `foreach`/pattern/`out` variable, a field, a property or a primary
//! constructor parameter. A name declared with two different types, or
//! without one (a lambda parameter, `var x = F()`), maps to `None`: its type
//! is not proven.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::{text, type_base_name};

/// Name -> declared type, `None` when unproven.
pub(crate) type Types = HashMap<String, Option<String>>;

/// What a type body declares: its methods, which `this.M()` reaches, and its
/// fields' and properties' declared types.
#[derive(Debug, Default)]
pub(crate) struct ClassScope {
    pub(crate) name: String,
    pub(crate) methods: HashSet<String>,
    pub(crate) fields: Types,
}

fn record(out: &mut Types, name: &str, ty: Option<String>) {
    out.entry(name.to_string())
        .and_modify(|seen| {
            if *seen != ty {
                *seen = None;
            }
        })
        .or_insert(ty);
}

/// The type a type node names: `Invoice`, `Invoice?`, `A.B.Invoice`,
/// `List<Invoice>` (its outer `List`), or a predefined type (`string`), which
/// no repository type is. `var`, arrays, tuples and the rest are not proven.
pub(crate) fn named_type(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "nullable_type" => node
            .child_by_field_name("type")
            .and_then(|t| named_type(t, source)),
        "predefined_type" => Some(text(node, source).to_string()),
        _ => type_base_name(node, source),
    }
}

/// `new Invoice(1)` constructs `Invoice`.
pub(crate) fn constructed(value: Node, source: &str) -> Option<String> {
    (value.kind() == "object_creation_expression")
        .then(|| value.child_by_field_name("type"))
        .flatten()
        .and_then(|t| named_type(t, source))
}

/// The declared type of `type`, or, for `var`, of the constructor call that
/// initializes it.
fn declared(ty: Option<Node>, value: Option<Node>, source: &str) -> Option<String> {
    match ty {
        Some(t) if t.kind() == "implicit_type" => value.and_then(|v| constructed(v, source)),
        Some(t) => named_type(t, source),
        None => None,
    }
}

/// The declarators of a `variable_declaration` with their declared types.
fn declarators(declaration: Node, source: &str, out: &mut Types) {
    let ty = declaration.child_by_field_name("type");
    let mut cursor = declaration.walk();
    for declarator in declaration.named_children(&mut cursor) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = declarator.child_by_field_name("name") else {
            continue;
        };
        let value = (0..declarator.named_child_count())
            .filter_map(|i| declarator.named_child(i))
            .find(|c| c.id() != name.id());
        record(out, text(name, source), declared(ty, value, source));
    }
}

/// The declared types of a member's parameters and of the names its body
/// declares, not descending into local functions or nested types.
pub(crate) fn locals(node: Node, source: &str, out: &mut Types, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    let named = |field: &str| node.child_by_field_name(field);
    match node.kind() {
        "parameter"
        | "foreach_statement"
        | "declaration_pattern"
        | "declaration_expression"
        | "catch_declaration" => {
            let name = match node.kind() {
                "foreach_statement" => named("left"),
                _ => named("name"),
            };
            if let Some(name) = name.filter(|n| n.kind() == "identifier") {
                record(
                    out,
                    text(name, source),
                    declared(named("type"), None, source),
                );
            }
            if node.kind() == "foreach_statement" {
                if let Some(body) = named("body") {
                    locals(body, source, out, depth + 1);
                }
            }
        }
        // A lambda's `x => …` parameter has no declared type.
        "implicit_parameter" => record(out, text(node, source), None),
        "variable_declaration" => {
            declarators(node, source, out);
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                locals(child, source, out, depth + 1);
            }
        }
        "local_function_statement"
        | "class_declaration"
        | "struct_declaration"
        | "record_declaration"
        | "interface_declaration" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                locals(child, source, out, depth + 1);
            }
        }
    }
}

/// What a type declares: method names and field/property types, with a
/// primary constructor's parameters.
pub(crate) fn class_scope(name: &str, class: Node, source: &str) -> ClassScope {
    let mut scope = ClassScope {
        name: name.to_string(),
        ..ClassScope::default()
    };
    let mut cursor = class.walk();
    for part in class.named_children(&mut cursor) {
        if part.kind() == "parameter_list" {
            let mut params = part.walk();
            for param in part.named_children(&mut params) {
                if param.kind() == "parameter" {
                    if let Some(n) = param.child_by_field_name("name") {
                        let ty = declared(param.child_by_field_name("type"), None, source);
                        record(&mut scope.fields, text(n, source), ty);
                    }
                }
            }
        }
    }
    let Some(body) = class.child_by_field_name("body") else {
        return scope;
    };
    let mut members = body.walk();
    for member in body.named_children(&mut members) {
        match member.kind() {
            "method_declaration" => {
                if let Some(n) = member.child_by_field_name("name") {
                    scope.methods.insert(text(n, source).to_string());
                }
            }
            "property_declaration" => {
                if let Some(n) = member.child_by_field_name("name") {
                    let ty = declared(member.child_by_field_name("type"), None, source);
                    record(&mut scope.fields, text(n, source), ty);
                }
            }
            "field_declaration" => {
                let mut parts = member.walk();
                let declaration = member
                    .named_children(&mut parts)
                    .find(|c| c.kind() == "variable_declaration");
                if let Some(declaration) = declaration {
                    declarators(declaration, source, &mut scope.fields);
                }
            }
            _ => {}
        }
    }
    scope
}
