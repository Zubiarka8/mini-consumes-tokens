//! The class a receiver is declared with, for `x.m()` and `p->m()`.
//!
//! C++ is statically typed, so a variable declared with one class in a
//! function proves the class of `x.m()` there. Only an explicit declaration
//! counts: `auto`, a type this scan does not name (a builtin, a qualified
//! non-`std` name) and a name declared more than once with different types
//! in the function (a shadowing block, a lambda parameter) prove nothing.

use std::collections::HashMap;

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

/// What a receiver's declared type names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Declared {
    /// A class or struct, by its unqualified name.
    Class(String),
    /// A `std::` type: the standard library's, never the project's.
    Std,
}

/// Variable name to its one proven declared type, for a whole function.
pub(crate) type Types = HashMap<String, Declared>;

/// The declared types of the variables of `function` (a `function_definition`),
/// parameters and locals of every nested block and lambda alike.
pub(crate) fn declared_types(function: Node, source: &str) -> Types {
    let mut seen: HashMap<String, Vec<Option<Declared>>> = HashMap::new();
    collect(function, source, 0, &mut seen);
    seen.into_iter()
        .filter_map(|(name, types)| {
            let first = types.first()?.clone()?;
            types
                .iter()
                .all(|t| t.as_ref() == Some(&first))
                .then_some((name, first))
        })
        .collect()
}

fn collect(
    node: Node,
    source: &str,
    depth: u32,
    seen: &mut HashMap<String, Vec<Option<Declared>>>,
) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if matches!(
        node.kind(),
        "parameter_declaration" | "optional_parameter_declaration" | "declaration"
    ) {
        let declared = node
            .child_by_field_name("type")
            .and_then(|t| declared_type(t, source));
        let mut cursor = node.walk();
        for declarator in node.children_by_field_name("declarator", &mut cursor) {
            if let Some(name) = declarator_name(declarator, source) {
                seen.entry(name).or_default().push(declared.clone());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, depth + 1, seen);
    }
}

/// The class or `std` type a type specifier names, `None` when it names
/// neither (`auto`, a builtin, a qualified project name).
fn declared_type(ty: Node, source: &str) -> Option<Declared> {
    match ty.kind() {
        "type_identifier" => Some(Declared::Class(source[ty.byte_range()].to_string())),
        "template_type" => ty
            .child_by_field_name("name")
            .and_then(|n| declared_type(n, source)),
        "qualified_identifier" => {
            let scope = ty.child_by_field_name("scope")?;
            (source[scope.byte_range()].trim() == "std").then_some(Declared::Std)
        }
        _ => None,
    }
}

/// The variable a declarator declares, through pointers, references,
/// arrays and initializers. A function declarator declares no variable.
fn declarator_name(declarator: Node, source: &str) -> Option<String> {
    match declarator.kind() {
        "identifier" => Some(source[declarator.byte_range()].to_string()),
        "pointer_declarator" | "array_declarator" | "init_declarator" => declarator
            .child_by_field_name("declarator")
            .and_then(|d| declarator_name(d, source)),
        "reference_declarator" | "parenthesized_declarator" => declarator
            .named_child(0)
            .and_then(|d| declarator_name(d, source)),
        _ => None,
    }
}
