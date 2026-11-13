//! Declared types of the names a method call's receiver can be, read from
//! the syntax alone: a TS annotation (`inv: Invoice`, `const x: T`), a
//! constructor (`= new T()`), a class field or a parameter property. A name
//! declared with two different types, or without one, maps to `None`: its
//! type is not proven.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use super::{is_parameter_property, text};

/// Name -> declared type, `None` when unproven.
pub(super) type Types = HashMap<String, Option<String>>;

/// What a class body declares: its methods and fields, which `this.m()`
/// reaches, and its fields' declared types, which `this.f.m()` reaches.
#[derive(Debug, Default)]
pub(super) struct ClassScope {
    pub(super) name: String,
    pub(super) methods: HashSet<String>,
    pub(super) fields: Types,
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

/// The type a `type_annotation` (or its inner type) names, when it is one
/// nominal type: `Invoice`, `Repo<User>`, `ns.Invoice`, or a primitive
/// (`string`), whose methods no repository file declares. Unions, arrays,
/// object and function types, `any`/`unknown` and type parameters are not
/// proven.
pub(super) fn annotated(node: Node, source: &str, generics: &HashSet<String>) -> Option<String> {
    let ty = if node.kind() == "type_annotation" {
        node.named_child(0)?
    } else {
        node
    };
    match ty.kind() {
        "type_identifier" => {
            let name = text(ty, source);
            (!generics.contains(name)).then(|| name.to_string())
        }
        "generic_type" | "nested_type_identifier" => ty
            .child_by_field_name("name")
            .and_then(|n| annotated(n, source, generics)),
        "predefined_type" => match text(ty, source) {
            name @ ("string" | "number" | "boolean" | "bigint" | "symbol") => {
                Some(name.to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

/// `new T(…)` names `T`.
pub(super) fn constructed(value: Node, source: &str) -> Option<String> {
    if value.kind() != "new_expression" {
        return None;
    }
    let constructor = value.child_by_field_name("constructor")?;
    (constructor.kind() == "identifier").then(|| text(constructor, source).to_string())
}

/// The declared types of a function's parameters and of the variables its
/// body declares, not descending into nested functions or classes (their
/// names are theirs). A name declared twice keeps a type only when both
/// declarations agree.
pub(super) fn locals(
    node: Node,
    source: &str,
    generics: &HashSet<String>,
    out: &mut Types,
    depth: u32,
) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match node.kind() {
        "required_parameter" | "optional_parameter" => {
            if let Some(pattern) = node
                .child_by_field_name("pattern")
                .filter(|p| p.kind() == "identifier")
            {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|t| annotated(t, source, generics));
                record(out, text(pattern, source), ty);
            }
        }
        "variable_declarator" => {
            if let Some(name) = node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
            {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|t| annotated(t, source, generics))
                    .or_else(|| {
                        node.child_by_field_name("value")
                            .and_then(|v| constructed(v, source))
                    });
                record(out, text(name, source), ty);
            }
            if let Some(value) = node.child_by_field_name("value") {
                locals(value, source, generics, out, depth + 1);
            }
        }
        "arrow_function"
        | "function_expression"
        | "function_declaration"
        | "generator_function"
        | "generator_function_declaration"
        | "method_definition"
        | "class_declaration"
        | "class" => {}
        _ => mct_tree_sitter::for_each_child(node, |child| {
            locals(child, source, generics, out, depth + 1);
        }),
    }
}

/// What a class body declares: method names, field types, and the
/// constructor's parameter properties (`constructor(private repo: Repo)`).
pub(super) fn class_scope(
    name: &str,
    body: Node,
    source: &str,
    generics: &HashSet<String>,
) -> ClassScope {
    let mut scope = ClassScope {
        name: name.to_string(),
        ..ClassScope::default()
    };
    mct_tree_sitter::for_each_child(body, |member| match member.kind() {
        "method_definition" => {
            let Some(name) = member.child_by_field_name("name") else {
                return;
            };
            let name = text(name, source);
            if let Some(ty) = accessor_type(member, source, generics) {
                record(&mut scope.fields, name, ty);
                return;
            }
            scope.methods.insert(name.to_string());
            if name == "constructor" {
                if let Some(params) = member.child_by_field_name("parameters") {
                    mct_tree_sitter::for_each_child(params, |param| {
                        let is_property =
                            matches!(param.kind(), "required_parameter" | "optional_parameter")
                                && is_parameter_property(param, source);
                        if !is_property {
                            return;
                        }
                        if let Some(pattern) = param
                            .child_by_field_name("pattern")
                            .filter(|p| p.kind() == "identifier")
                        {
                            let ty = param
                                .child_by_field_name("type")
                                .and_then(|t| annotated(t, source, generics));
                            record(&mut scope.fields, text(pattern, source), ty);
                        }
                    });
                }
            }
        }
        "field_definition" | "public_field_definition" => {
            let Some(field) = member
                .child_by_field_name("property")
                .or_else(|| member.child_by_field_name("name"))
            else {
                return;
            };
            let value = member.child_by_field_name("value");
            if value.is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression")) {
                scope.methods.insert(text(field, source).to_string());
                return;
            }
            let ty = member
                .child_by_field_name("type")
                .and_then(|t| annotated(t, source, generics))
                .or_else(|| value.and_then(|v| constructed(v, source)));
            record(&mut scope.fields, text(field, source), ty);
        }
        _ => {}
    });
    scope
}

/// The declared type of the property a class accessor stands for: what
/// `get x(): T` returns or `set x(v: T)` takes, `Some(None)` when
/// unannotated, and `None` for a member that is no accessor.
fn accessor_type(member: Node, source: &str, generics: &HashSet<String>) -> Option<Option<String>> {
    let mut cursor = member.walk();
    let keyword = member
        .children(&mut cursor)
        .find(|c| matches!(c.kind(), "get" | "set"))?;
    let annotation = if keyword.kind() == "get" {
        member.child_by_field_name("return_type")
    } else {
        member
            .child_by_field_name("parameters")
            .and_then(|p| p.named_child(0))
            .and_then(|p| p.child_by_field_name("type"))
    };
    Some(annotation.and_then(|t| annotated(t, source, generics)))
}
