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

use crate::{builtins, find_child, parameter_variable, variable_text};

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
    "property_hook",
];

/// Variable name (with `$`) to the class name as written, for one function.
pub(crate) type Types = HashMap<String, String>;

/// Proven receiver types and the first binding of each named local, in source order.
pub(crate) fn locals<'tree>(function: Node<'tree>, source: &str) -> (Types, Vec<Node<'tree>>) {
    let mut typed = Types::new();
    let mut parameters = HashSet::new();
    let mut scan = Scan::default();
    if let Some(params) = function.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            let Some(name) = parameter_variable(param) else {
                continue;
            };
            parameters.insert(text(name, source));
            scan.bindings.push(name);
            if !matches!(
                param.kind(),
                "simple_parameter" | "property_promotion_parameter"
            ) {
                continue;
            }
            if let Some(class) = param
                .child_by_field_name("type")
                .and_then(|t| class_of(t, source))
            {
                typed.insert(text(name, source).to_string(), class);
            }
        }
    }
    if let Some(captures) = find_child(function, "anonymous_function_use_clause") {
        let mut cursor = captures.walk();
        for capture in captures.named_children(&mut cursor) {
            scan.bind_target(capture, 0);
        }
        scan.poison_all(captures, source);
    }
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
                && !parameters.contains(name.as_str())
            {
                proven.insert(name.clone(), class.clone());
            }
        }
    }
    let mut seen = HashSet::new();
    scan.bindings.retain(|node| {
        variable_text(*node, source).is_some_and(|name| {
            name != "this"
                && !builtins::is_superglobal(name)
                && !scan.globals.contains(name)
                && seen.insert(name)
        })
    });
    (proven, scan.bindings)
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

#[derive(Default)]
struct Scan<'tree, 'src> {
    /// Per variable, what each assignment gives it: `Some(class)` for
    /// `new Class(...)`, `None` for anything else.
    writes: HashMap<String, Vec<Option<String>>>,
    /// Variables a statement may rebind in a way the scan does not follow.
    poisoned: HashSet<String>,
    bindings: Vec<Node<'tree>>,
    globals: HashSet<&'src str>,
}

impl<'tree, 'src> Scan<'tree, 'src> {
    fn visit(&mut self, node: Node<'tree>, source: &'src str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH || SCOPES.contains(&node.kind()) {
            return;
        }
        match node.kind() {
            "assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let right = node.child_by_field_name("right");
                    let class = right.and_then(|r| constructed(r, source));
                    if left.kind() == "variable_name" {
                        if !right.is_some_and(|right| {
                            matches!(right.kind(), "anonymous_function" | "arrow_function")
                        }) {
                            self.bindings.push(left);
                        }
                        self.writes
                            .entry(text(left, source).to_string())
                            .or_default()
                            .push(class);
                    } else {
                        self.bind_target(left, depth + 1);
                        self.poison_all(left, source);
                    }
                }
            }
            "augmented_assignment_expression" | "reference_assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    if node.kind() == "reference_assignment_expression" {
                        self.bindings.push(left);
                    }
                    self.poison_all(left, source);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.poison_all(right, source);
                }
            }
            "global_declaration" => {
                let mut cursor = node.walk();
                self.globals.extend(
                    node.named_children(&mut cursor)
                        .filter_map(|child| variable_text(child, source)),
                );
                self.poison_all(node, source);
            }
            "static_variable_declaration" | "catch_clause" => {
                self.bindings.extend(node.child_by_field_name("name"));
                self.poison_all(node, source);
            }
            "foreach_statement" => {
                // The first child is the iterable, the second is the target or key/value pair.
                let mut cursor = node.walk();
                if let Some(target) = node
                    .named_children(&mut cursor)
                    .filter(|child| child.kind() != "comment")
                    .nth(1)
                {
                    if target.kind() == "pair" {
                        let mut pair = target.walk();
                        for part in target.named_children(&mut pair) {
                            self.bind_target(part, depth + 1);
                        }
                    } else {
                        self.bind_target(target, depth + 1);
                    }
                }
                self.poison_all(node, source);
            }
            "by_ref" | "reference_modifier" | "unset_statement" | "update_expression" => {
                self.poison_all(node, source)
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, source, depth + 1);
        }
    }

    /// Record only writable variable slots. A key, property receiver or array
    /// index may contain a variable without binding it.
    fn bind_target(&mut self, node: Node<'tree>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "variable_name" => self.bindings.push(node),
            "by_ref" => {
                if let Some(target) = node.named_child(0) {
                    self.bind_target(target, depth + 1);
                }
            }
            "list_literal" => {
                let mut cursor = node.walk();
                let mut parts = node
                    .children(&mut cursor)
                    .filter(|part| part.kind() != "comment")
                    .peekable();
                while let Some(part) = parts.next() {
                    if part.is_named() && parts.peek().is_none_or(|next| next.kind() != "=>") {
                        self.bind_target(part, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }

    /// Every variable under `node` is no longer proven.
    fn poison_all(&mut self, node: Node, source: &str) {
        let mut cursor = node.walk();
        loop {
            let current = cursor.node();
            if current.kind() == "variable_name" {
                self.poisoned.insert(text(current, source).to_string());
            }
            if cursor.goto_first_child() {
                continue;
            }
            loop {
                if cursor.node() == node {
                    return;
                }
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    return;
                }
            }
        }
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
