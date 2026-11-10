//! Declared types of the names a method call's receiver can be, read from
//! the syntax alone: a method receiver, a parameter, `var x T`,
//! `x := T{…}`, `x := &T{…}`, `x := new(T)`, a package-level `var`, and the
//! fields of the structs this file declares. A type without a name (a slice,
//! an anonymous interface) is `Unnamed`. A name declared with two different
//! types, or whose type the syntax does not state (`x := f()`, a `range`
//! variable), is `Unknown`.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::text;

/// What the syntax proves about a name's type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    /// A named type: `Invoice` for `x *Invoice`.
    Named(String),
    /// A declared type without a name: `[]T`, `map[K]V`, `interface{ … }`.
    Unnamed,
    /// Not stated: `x := f()`, a `range` variable, two different types.
    Unknown,
}

/// Name -> what its declarations prove about its type.
pub(crate) type Types = HashMap<String, Ty>;

fn record(out: &mut Types, name: &str, ty: Ty) {
    out.entry(name.to_string())
        .and_modify(|seen| {
            if *seen != ty {
                *seen = Ty::Unknown;
            }
        })
        .or_insert(ty);
}

/// A declared type node: named, or a type without a name.
fn declared(node: Node, source: &str) -> Ty {
    named_type(node, source).map_or(Ty::Unnamed, Ty::Named)
}

/// A value's type when it constructs a named type, else unknown.
fn inferred(value: Option<&Node>, source: &str) -> Ty {
    value
        .and_then(|v| constructed(*v, source))
        .map_or(Ty::Unknown, Ty::Named)
}

/// The named type a type node is: `Invoice`, `*Invoice`, `Stack[T]` (its
/// `Stack`), `pkg.Invoice` (its `Invoice`). Slices, maps, channels, function
/// types and the rest are not proven.
pub(crate) fn named_type(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "type_identifier" => Some(text(node, source).to_string()),
        "pointer_type" => node.named_child(0).and_then(|t| named_type(t, source)),
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|t| named_type(t, source)),
        "qualified_type" => node
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string()),
        "parenthesized_type" => node.named_child(0).and_then(|t| named_type(t, source)),
        _ => None,
    }
}

/// The type a value constructs: `T{…}`, `&T{…}`, `new(T)`.
pub(crate) fn constructed(value: Node, source: &str) -> Option<String> {
    match value.kind() {
        "composite_literal" => value
            .child_by_field_name("type")
            .and_then(|t| named_type(t, source)),
        "unary_expression" => value
            .child_by_field_name("operand")
            .filter(|_| value.child(0).is_some_and(|op| op.kind() == "&"))
            .and_then(|o| constructed(o, source)),
        "call_expression" => {
            let callee = value.child_by_field_name("function")?;
            if callee.kind() != "identifier" || text(callee, source) != "new" {
                return None;
            }
            let arguments = value.child_by_field_name("arguments")?;
            let ty = arguments.named_child(0)?;
            named_type(ty, source)
        }
        _ => None,
    }
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// `name` fields of a declaration (`a, b int` has two).
fn names<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children_by_field_name("name", &mut cursor)
        .filter(|n| n.kind() == "identifier")
        .collect()
}

/// The declared types of a function's receiver, parameters and the names its
/// body declares, closures included.
pub(crate) fn locals(node: Node, source: &str, out: &mut Types, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match node.kind() {
        "parameter_declaration" | "variadic_parameter_declaration" => {
            // A variadic `xs ...T` is a slice.
            let ty = match node.child_by_field_name("type") {
                Some(t) if node.kind() == "parameter_declaration" => declared(t, source),
                Some(_) => Ty::Unnamed,
                None => Ty::Unknown,
            };
            for name in names(node) {
                record(out, text(name, source), ty.clone());
            }
        }
        "var_spec" => {
            let stated = node
                .child_by_field_name("type")
                .map(|t| declared(t, source));
            let values = node
                .child_by_field_name("value")
                .map(named_children)
                .unwrap_or_default();
            let declared_names = names(node);
            let paired = values.len() == declared_names.len();
            for (i, name) in declared_names.iter().enumerate() {
                let ty = stated
                    .clone()
                    .unwrap_or_else(|| inferred(paired.then(|| values.get(i)).flatten(), source));
                record(out, text(*name, source), ty);
            }
            for value in values {
                locals(value, source, out, depth + 1);
            }
        }
        "short_var_declaration" => {
            let left = node
                .child_by_field_name("left")
                .map(named_children)
                .unwrap_or_default();
            let right = node
                .child_by_field_name("right")
                .map(named_children)
                .unwrap_or_default();
            let paired = left.len() == right.len();
            for (i, name) in left.iter().enumerate() {
                if name.kind() != "identifier" {
                    continue;
                }
                let ty = inferred(paired.then(|| right.get(i)).flatten(), source);
                record(out, text(*name, source), ty);
            }
            for value in right {
                locals(value, source, out, depth + 1);
            }
        }
        // `for k, v := range xs`: the element type is not proven.
        "range_clause" => {
            if let Some(left) = node.child_by_field_name("left") {
                for name in named_children(left) {
                    if name.kind() == "identifier" {
                        record(out, text(name, source), Ty::Unknown);
                    }
                }
            }
        }
        _ => {
            for child in named_children(node) {
                locals(child, source, out, depth + 1);
            }
        }
    }
}

/// The package-level `var x T` declarations of a file.
pub(crate) fn globals(root: Node, source: &str) -> Types {
    let mut out = Types::new();
    for declaration in named_children(root) {
        if declaration.kind() == "var_declaration" {
            for spec in named_children(declaration) {
                if spec.kind() == "var_spec" {
                    locals(spec, source, &mut out, 1);
                }
            }
        }
    }
    out
}

/// What this file declares about its types.
#[derive(Debug, Default)]
pub(crate) struct Structs {
    /// Struct name -> field name -> declared type. An embedded field is
    /// named by its type.
    pub(crate) fields: HashMap<String, Types>,
    /// The structs with an embedded field, whose method set may hold
    /// promoted methods declared elsewhere.
    pub(crate) embedding: HashSet<String>,
    /// Type name -> the methods this file declares on it.
    pub(crate) methods: HashMap<String, HashSet<String>>,
}

/// The structs and methods this file declares.
pub(crate) fn structs(root: Node, source: &str) -> Structs {
    let mut out = Structs::default();
    let mut specs = Vec::new();
    collect_kind(root, "type_spec", &mut specs, 0);
    for spec in specs {
        let (Some(name), Some(ty)) = (
            spec.child_by_field_name("name"),
            spec.child_by_field_name("type"),
        ) else {
            continue;
        };
        if ty.kind() != "struct_type" {
            continue;
        }
        let mut fields = Types::new();
        let mut declarations = Vec::new();
        collect_kind(ty, "field_declaration", &mut declarations, 0);
        for field in declarations {
            let field_type = field
                .child_by_field_name("type")
                .map_or(Ty::Unknown, |t| declared(t, source));
            let field_names = names_of_field(field);
            if field_names.is_empty() {
                // Embedded: `Base` or `*Base` is the field `Base`.
                out.embedding.insert(text(name, source).to_string());
                if let Ty::Named(embedded) = &field_type {
                    record(&mut fields, &embedded.clone(), field_type.clone());
                }
            }
            for n in field_names {
                record(&mut fields, text(n, source), field_type.clone());
            }
        }
        out.fields.insert(text(name, source).to_string(), fields);
    }
    for declaration in named_children(root) {
        if declaration.kind() != "method_declaration" {
            continue;
        }
        let receiver = declaration
            .child_by_field_name("receiver")
            .and_then(|r| named_children(r).into_iter().next())
            .and_then(|p| p.child_by_field_name("type"))
            .and_then(|t| named_type(t, source));
        if let (Some(receiver), Some(method)) = (receiver, declaration.child_by_field_name("name"))
        {
            out.methods
                .entry(receiver)
                .or_default()
                .insert(text(method, source).to_string());
        }
    }
    out
}

fn names_of_field<'t>(field: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = field.walk();
    field
        .children_by_field_name("name", &mut cursor)
        .filter(|n| n.kind() == "field_identifier")
        .collect()
}

fn collect_kind<'t>(node: Node<'t>, kind: &str, out: &mut Vec<Node<'t>>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    for child in named_children(node) {
        if child.kind() == kind {
            out.push(child);
        } else {
            collect_kind(child, kind, out, depth + 1);
        }
    }
}
