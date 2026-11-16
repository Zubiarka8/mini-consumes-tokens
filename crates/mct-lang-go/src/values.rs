//! Helpers for the functions a file uses by name as a value.

use std::collections::HashSet;

use tree_sitter::Node;

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// The expression inside a `literal_element` (or the node itself).
pub(crate) fn unwrap_element(node: Node) -> Option<Node> {
    if node.kind() == "literal_element" {
        node.named_child(0)
    } else {
        Some(node)
    }
}

/// The names of the functions declared at package level in `root`.
pub(crate) fn declared_functions(root: Node, source: &str) -> HashSet<String> {
    let mut cursor = root.walk();
    let children: Vec<Node> = root.named_children(&mut cursor).collect();
    children
        .into_iter()
        .filter(|n| n.kind() == "function_declaration")
        .filter_map(|n| n.child_by_field_name("name"))
        .map(|n| text(n, source).to_string())
        .collect()
}

/// Whether a parameter or a local (`:=`, `var`, `range`) named `name`
/// is declared in a function enclosing `node`.
pub(crate) fn is_shadowed(node: Node, name: &str, source: &str) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if matches!(
            ancestor.kind(),
            "function_declaration" | "method_declaration" | "func_literal"
        ) {
            if let Some(parameters) = ancestor.child_by_field_name("parameters") {
                if declares_name(parameters, name, source, 0) {
                    return true;
                }
            }
            if let Some(body) = ancestor.child_by_field_name("body") {
                if declares_local(body, name, source, 0) {
                    return true;
                }
            }
        }
        current = ancestor.parent();
    }
    false
}

fn declares_name(node: Node, name: &str, source: &str, depth: u32) -> bool {
    if depth > 64 {
        return false;
    }
    if node.kind() == "identifier" && text(node, source) == name {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| declares_name(child, name, source, depth + 1))
}

/// A `:=`, `var` or `range` declaration of `name` anywhere under `node`.
fn declares_local(node: Node, name: &str, source: &str, depth: u32) -> bool {
    if depth > 64 {
        return false;
    }
    let declared = match node.kind() {
        "short_var_declaration" | "range_clause" => node.child_by_field_name("left"),
        "var_spec" => node.child_by_field_name("name"),
        _ => None,
    };
    if declared.is_some_and(|d| declares_name(d, name, source, 0)) {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| declares_local(child, name, source, depth + 1))
}
