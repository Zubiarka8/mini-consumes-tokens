//! ES module exports and CommonJS exported declarations and references.

use super::{rightmost_name, text, Walker};
use mct_core::{RelationKind, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

impl Walker<'_> {
    /// `export function foo() {}` / `export class Foo {}` still create their
    /// underlying symbol via the normal declaration visit; `export default
    /// foo` and `export { foo, bar as baz }` reference an *existing* symbol
    /// by name rather than declaring a new one, recorded as `References`
    /// (the same relation the Python plugin uses for decorator references).
    pub(super) fn visit_export_statement(
        &mut self,
        node: Node,
        owner: SymbolId,
        type_name: Option<&str>,
        depth: u32,
    ) {
        if let Some(declaration) = node.child_by_field_name("declaration") {
            self.visit(declaration, owner, type_name, depth + 1);
        }
        if let Some(value) = node.child_by_field_name("value") {
            if let Some(name_node) = rightmost_name(value) {
                self.push_relation(
                    owner,
                    RelationKind::References,
                    text(name_node, self.source).to_string(),
                    location(value),
                );
            } else {
                self.visit(value, owner, type_name, depth + 1);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "export_clause" {
                let mut c2 = child.walk();
                for spec in child.named_children(&mut c2) {
                    if spec.kind() != "export_specifier" {
                        continue;
                    }
                    if let Some(name_node) = spec.child_by_field_name("name") {
                        if name_node.kind() != "default" {
                            self.push_relation(
                                owner,
                                RelationKind::References,
                                text(name_node, self.source).to_string(),
                                location(spec),
                            );
                        }
                    }
                }
            }
        }
    }

    /// CommonJS's only way to export: an assignment to `module.exports`,
    /// `module.exports.name`, or `exports.name`. Not a real relation kind of
    /// its own (`RelationKind` has no `Exports`) — a bare re-export of an
    /// existing identifier is a `References`, same as an ES module named
    /// export; a function/arrow assigned directly as the exported value is
    /// recorded as a new `Function` symbol, same treatment as the ESM
    /// arrow-assigned-to-variable case. Anything that isn't one of these
    /// shapes (e.g. a plain reassignment) is just recursed into normally.
    pub(super) fn visit_assignment_expression(
        &mut self,
        node: Node,
        owner: SymbolId,
        type_name: Option<&str>,
        depth: u32,
    ) {
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");
        if let (Some(left), Some(right)) = (left, right) {
            if let Some(member_name) = commonjs_export_target(left, self.source) {
                self.record_commonjs_export(member_name, right, owner, depth);
                return;
            }
        }
        if let Some(right) = right {
            self.visit(right, owner, type_name, depth + 1);
        }
    }

    fn record_commonjs_export(
        &mut self,
        member_name: Option<String>,
        value: Node,
        owner: SymbolId,
        depth: u32,
    ) {
        match value.kind() {
            "arrow_function" | "function_expression" => match member_name {
                Some(name) => {
                    let id = self.push_symbol(name, SymbolKind::Function, location(value), None);
                    self.visit_function_like_body(value, id, depth);
                }
                None => self.visit_function_like_body(value, owner, depth),
            },
            "identifier" => {
                self.push_relation(
                    owner,
                    RelationKind::References,
                    text(value, self.source).to_string(),
                    location(value),
                );
            }
            "object" => {
                let mut cursor = value.walk();
                for child in value.named_children(&mut cursor) {
                    match child.kind() {
                        "shorthand_property_identifier" => {
                            self.push_relation(
                                owner,
                                RelationKind::References,
                                text(child, self.source).to_string(),
                                location(child),
                            );
                        }
                        "pair" => {
                            let (Some(key), Some(val)) = (
                                child.child_by_field_name("key"),
                                child.child_by_field_name("value"),
                            ) else {
                                continue;
                            };
                            match val.kind() {
                                "identifier" => {
                                    self.push_relation(
                                        owner,
                                        RelationKind::References,
                                        text(val, self.source).to_string(),
                                        location(child),
                                    );
                                }
                                "arrow_function" | "function_expression" => {
                                    let id = self.push_symbol(
                                        text(key, self.source).to_string(),
                                        SymbolKind::Function,
                                        location(child),
                                        None,
                                    );
                                    self.visit_function_like_body(val, id, depth);
                                }
                                _ => {}
                            }
                        }
                        "method_definition" => {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let id = self.push_symbol(
                                    text(name_node, self.source).to_string(),
                                    SymbolKind::Function,
                                    location(child),
                                    None,
                                );
                                self.visit_function_like_body(child, id, depth);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => self.visit(value, owner, None, depth + 1),
        }
    }
}

/// Recognizes an assignment target as one of the three CommonJS export
/// shapes: `module.exports = ...` / `exports.NAME = ...` /
/// `module.exports.NAME = ...`. Returns `Some(None)` for the bare
/// whole-module form, `Some(Some(name))` for a named member, `None` if the
/// left-hand side isn't a CommonJS export at all.
fn commonjs_export_target(left: Node, source: &str) -> Option<Option<String>> {
    if left.kind() != "member_expression" {
        return None;
    }
    let object = left.child_by_field_name("object")?;
    let property = left.child_by_field_name("property")?;
    let property_name = text(property, source).to_string();
    match object.kind() {
        "identifier" => {
            let object_name = text(object, source);
            if object_name == "module" && property_name == "exports" {
                Some(None)
            } else if object_name == "exports" {
                Some(Some(property_name))
            } else {
                None
            }
        }
        "member_expression" => {
            let inner_object = object.child_by_field_name("object")?;
            let inner_property = object.child_by_field_name("property")?;
            if inner_object.kind() == "identifier"
                && text(inner_object, source) == "module"
                && text(inner_property, source) == "exports"
            {
                Some(Some(property_name))
            } else {
                None
            }
        }
        _ => None,
    }
}
