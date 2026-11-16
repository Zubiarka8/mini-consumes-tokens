//! The class a local variable is proven to hold, for `$x->method()`.
//!
//! A variable is proven only when the function says so unambiguously: a
//! parameter typed with one class and never assigned in the body, or a local
//! assigned exactly once, to `new Class(...)`. Anything that could change it
//! (a second assignment, `foreach`, `global`, `static`, `catch`, a by-reference
//! use, `list()`) or leave its class open (a union, a builtin or an untyped
//! parameter) proves nothing, so the call stays a member call.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

/// Scopes with their own variables: a nested one is not this function's.
const SCOPES: &[&str] = &[
    "function_definition",
    "method_declaration",
    "anonymous_function",
    "arrow_function",
    "class_declaration",
    "interface_declaration",
    "trait_declaration",
    "enum_declaration",
];

/// Variable name (with `$`) to the class name as written, for one function.
pub(crate) type Types = HashMap<String, String>;

/// The proven locals of `function` (a `function_definition` or
/// `method_declaration`).
pub(crate) fn locals(function: Node, source: &str) -> Types {
    let mut typed = Types::new();
    if let Some(params) = function.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if !matches!(
                param.kind(),
                "simple_parameter" | "property_promotion_parameter"
            ) {
                continue;
            }
            let name = param
                .child_by_field_name("name")
                .map(|n| text(n, source).to_string());
            let class = param
                .child_by_field_name("type")
                .and_then(|t| class_of(t, source));
            if let (Some(name), Some(class)) = (name, class) {
                typed.insert(name, class);
            }
        }
    }
    let mut scan = Scan::default();
    if let Some(body) = function.child_by_field_name("body") {
        scan.visit(body, source, 0);
    }
    let mut proven = Types::new();
    for (name, class) in typed {
        if !scan.writes.contains_key(&name) && !scan.poisoned.contains(&name) {
            proven.insert(name, class);
        }
    }
    for (name, writes) in &scan.writes {
        if let [Some(class)] = writes.as_slice() {
            if !scan.poisoned.contains(name)
                && !proven.contains_key(name)
                && !params(function, source).contains(name)
            {
                proven.insert(name.clone(), class.clone());
            }
        }
    }
    proven
}

/// The class `new Class(...)` constructs, as written.
pub(crate) fn constructed(node: Node, source: &str) -> Option<String> {
    let node = if node.kind() == "parenthesized_expression" {
        node.named_child(0)?
    } else {
        node
    };
    if node.kind() != "object_creation_expression" {
        return None;
    }
    let mut cursor = node.walk();
    let first = node.named_children(&mut cursor).next()?;
    matches!(first.kind(), "name" | "qualified_name").then(|| text(first, source).to_string())
}

/// The one class a parameter type names: `A` or `?A`, never a union,
/// an intersection or a builtin.
fn class_of(ty: Node, source: &str) -> Option<String> {
    match ty.kind() {
        "named_type" => {
            let mut cursor = ty.walk();
            let name = ty.named_children(&mut cursor).next()?;
            matches!(name.kind(), "name" | "qualified_name").then(|| text(name, source).to_string())
        }
        "optional_type" => class_of(ty.named_child(0)?, source),
        _ => None,
    }
}

fn params(function: Node, source: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(list) = function.child_by_field_name("parameters") {
        let mut cursor = list.walk();
        for param in list.named_children(&mut cursor) {
            if let Some(name) = param.child_by_field_name("name") {
                out.insert(text(name, source).to_string());
            }
        }
    }
    out
}

#[derive(Default)]
struct Scan {
    /// Per variable, what each assignment gives it: `Some(class)` for
    /// `new Class(...)`, `None` for anything else.
    writes: HashMap<String, Vec<Option<String>>>,
    /// Variables a statement may rebind in a way the scan does not follow.
    poisoned: HashSet<String>,
}

impl Scan {
    fn visit(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH || SCOPES.contains(&node.kind()) {
            return;
        }
        match node.kind() {
            "assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let class = node
                        .child_by_field_name("right")
                        .and_then(|r| constructed(r, source));
                    if left.kind() == "variable_name" {
                        self.writes
                            .entry(text(left, source).to_string())
                            .or_default()
                            .push(class);
                    } else {
                        self.poison_all(left, source);
                    }
                }
            }
            "augmented_assignment_expression" | "reference_assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.poison_all(left, source);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.poison_all(right, source);
                }
            }
            "foreach_statement"
            | "global_declaration"
            | "static_variable_declaration"
            | "catch_clause"
            | "by_ref"
            | "reference_modifier"
            | "unset_statement"
            | "update_expression" => self.poison_all(node, source),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, source, depth + 1);
        }
    }

    /// Every variable under `node` is no longer proven.
    fn poison_all(&mut self, node: Node, source: &str) {
        if node.kind() == "variable_name" {
            self.poisoned.insert(text(node, source).to_string());
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.poison_all(child, source);
        }
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
