//! ES module import bindings and CommonJS require paths.

use super::{text, Walker};
use mct_core::{RelationKind, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

impl Walker<'_> {
    /// One `import` statement's clause: a default binding (bare
    /// `identifier`), a namespace binding (`* as ns`), and/or a named-import
    /// list. Each records the *local* bound name as `to_name` (the alias
    /// when present, matching the Python plugin's `import x as y` ->
    /// `"y"` precedent), since that is the name later calls in this file
    /// will actually reference — not the name the source module exports it
    /// under.
    pub(super) fn visit_import_clause(&mut self, clause: Node, owner: SymbolId) {
        let mut cursor = clause.walk();
        for child in clause.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    self.push_relation(
                        owner,
                        RelationKind::Imports,
                        text(child, self.source).to_string(),
                        location(child),
                    );
                }
                "namespace_import" => {
                    if let Some(name_node) = child.named_child(0) {
                        self.push_relation(
                            owner,
                            RelationKind::Imports,
                            text(name_node, self.source).to_string(),
                            location(child),
                        );
                    }
                }
                "named_imports" => {
                    let mut c2 = child.walk();
                    for spec in child.named_children(&mut c2) {
                        if spec.kind() != "import_specifier" {
                            continue;
                        }
                        let target = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"));
                        if let Some(target) = target {
                            if target.kind() == "default" {
                                continue;
                            }
                            self.push_relation(
                                owner,
                                RelationKind::Imports,
                                text(target, self.source).to_string(),
                                location(spec),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// `require('module_name')`'s argument: the literal module path, or `None`
/// for a non-literal argument (`require(computedPath())`) — nothing static
/// to record in that case.
pub(super) fn require_argument(call_node: Node, source: &str) -> Option<String> {
    let arguments = call_node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    if first.kind() != "string" {
        return None;
    }
    let mut string_cursor = first.walk();
    let fragment = first
        .named_children(&mut string_cursor)
        .find(|c| c.kind() == "string_fragment");
    fragment.map(|c| text(c, source).to_string())
}
