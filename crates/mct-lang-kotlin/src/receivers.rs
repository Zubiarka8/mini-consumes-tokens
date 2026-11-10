//! Declared types of the names a method call's receiver can be, read from
//! the syntax alone: a parameter or `val x: T`, a constructor call
//! (`= Invoice(1)`, a capitalized callee by Kotlin's naming convention), a
//! class property or a constructor property. A name declared with two
//! different types, or without one (a lambda parameter, a `for` variable,
//! `val x = f()`), maps to `None`: its type is not proven.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::text;

/// Name -> declared type, `None` when unproven.
pub(crate) type Types = HashMap<String, Option<String>>;

/// What a class or object body declares: its methods, which `this.m()`
/// reaches, and its properties' declared types.
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

/// The nominal type `node` names: `Invoice`, `Invoice?`, `a.b.Invoice`,
/// `List<Invoice>` (its outer `List`). Function types and the rest are not
/// proven.
pub(crate) fn named_type(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "user_type" => {
            let mut cursor = node.walk();
            let last = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .last();
            last.map(|n| text(n, source).to_string())
        }
        "nullable_type" => node.named_child(0).and_then(|n| named_type(n, source)),
        _ => None,
    }
}

/// The type child of a declaration (`parameter`, `class_parameter`,
/// `variable_declaration`): the first named child that is a type.
fn declared(node: Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let ty = node
        .named_children(&mut cursor)
        .find(|c| matches!(c.kind(), "user_type" | "nullable_type"));
    ty.and_then(|t| named_type(t, source))
}

/// `Invoice(1)` constructs `Invoice`: a call whose callee is one
/// capitalized name.
pub(crate) fn constructed(value: Node, source: &str) -> Option<String> {
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = value.named_child(0)?;
    let name = text(callee, source);
    (callee.kind() == "identifier" && name.starts_with(char::is_uppercase))
        .then(|| name.to_string())
}

/// The initializer of a `property_declaration`: the named child after its
/// `variable_declaration`.
fn initializer(property: Node) -> Option<Node> {
    let mut cursor = property.walk();
    let mut seen = false;
    for child in property.named_children(&mut cursor) {
        if seen {
            return Some(child);
        }
        seen = child.kind() == "variable_declaration";
    }
    None
}

/// Record a property's name and declared or constructed type in its scope.
fn record_property(property: Node, source: &str, out: &mut Types) {
    let declaration = property
        .named_children(&mut property.walk())
        .find(|c| c.kind() == "variable_declaration");
    let Some(declaration) = declaration else {
        return;
    };
    let Some(name) = declaration
        .named_child(0)
        .filter(|n| n.kind() == "identifier")
    else {
        return;
    };
    let ty = declared(declaration, source)
        .or_else(|| initializer(property).and_then(|v| constructed(v, source)));
    record(out, text(name, source), ty);
}

/// The declared types of a function's parameters and of the names its body
/// declares (`val`, lambda and `for` variables), not descending into nested
/// functions or classes.
pub(crate) fn locals(node: Node, source: &str, out: &mut Types, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match node.kind() {
        "parameter" => {
            if let Some(name) = node.named_child(0).filter(|n| n.kind() == "identifier") {
                record(out, text(name, source), declared(node, source));
            }
        }
        "property_declaration" => {
            record_property(node, source, out);
            if let Some(value) = initializer(node) {
                locals(value, source, out, depth + 1);
            }
        }
        // A lambda parameter, a `for` variable, a destructuring entry.
        "variable_declaration" => {
            if let Some(name) = node.named_child(0).filter(|n| n.kind() == "identifier") {
                record(out, text(name, source), declared(node, source));
            }
        }
        "function_declaration" | "class_declaration" | "object_declaration" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                locals(child, source, out, depth + 1);
            }
        }
    }
}

/// What a class body declares: method names and property types, with the
/// primary constructor's `val`/`var` parameters.
pub(crate) fn class_scope(name: &str, class: Node, source: &str) -> ClassScope {
    let mut scope = ClassScope {
        name: name.to_string(),
        ..ClassScope::default()
    };
    let mut cursor = class.walk();
    for part in class.named_children(&mut cursor) {
        match part.kind() {
            "primary_constructor" => {
                let mut params = Vec::new();
                collect_kind(part, "class_parameter", &mut params, 0);
                for param in params {
                    let promoted = (0..param.child_count())
                        .filter_map(|i| param.child(i))
                        .any(|c| matches!(c.kind(), "val" | "var"));
                    if !promoted {
                        continue;
                    }
                    let name = (0..param.named_child_count())
                        .filter_map(|i| param.named_child(i))
                        .find(|c| c.kind() == "identifier");
                    if let Some(name) = name {
                        record(
                            &mut scope.fields,
                            text(name, source),
                            declared(param, source),
                        );
                    }
                }
            }
            "class_body" | "enum_class_body" => {
                let mut members = part.walk();
                for member in part.named_children(&mut members) {
                    match member.kind() {
                        "function_declaration" => {
                            if let Some(name) = member.child_by_field_name("name") {
                                scope.methods.insert(text(name, source).to_string());
                            }
                        }
                        "property_declaration" => {
                            record_property(member, source, &mut scope.fields);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    scope
}

fn collect_kind<'t>(node: Node<'t>, kind: &str, out: &mut Vec<Node<'t>>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            out.push(child);
        } else {
            collect_kind(child, kind, out, depth + 1);
        }
    }
}
