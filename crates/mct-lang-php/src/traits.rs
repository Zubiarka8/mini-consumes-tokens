//! Methods made available by PHP's explicit trait adaptations.

use std::collections::HashSet;

use mct_core::{RelationKind, RelationTarget, SymbolDetail, SymbolId, SymbolKind};
use mct_tree_sitter::{find_child, location};
use tree_sitter::Node;

use crate::{named_children, namespace_alias, text, ClassContext, ResolvedName, Walker};

impl Walker<'_> {
    #[inline(never)]
    pub(crate) fn visit_trait_use(&mut self, node: Node, owner: SymbolId) {
        let mut traits = Vec::new();
        for child in named_children(node) {
            traits.extend(self.push_class_reference(
                owner,
                RelationKind::Implements,
                Some(SymbolKind::Trait),
                child,
            ));
        }
        let (Some(adaptations), Some(body), Some(ClassContext::Named { name, .. })) = (
            find_child(node, "use_list"),
            node.parent(),
            self.classes.last(),
        ) else {
            return;
        };
        let class = name.clone();
        let rules = named_children(adaptations);
        let mut methods: HashSet<_> = named_children(body)
            .into_iter()
            .filter(|node| node.kind() == "method_declaration")
            .filter_map(|node| node.child_by_field_name("name"))
            .map(|node| text(node, self.source).to_ascii_lowercase())
            .collect();
        for rule in &rules {
            let Some((source, method, exported)) = self.trait_adaptation(*rule, &rules, &traits)
            else {
                continue;
            };
            let method_name = text(method, self.source);
            let exported_name = text(exported, self.source);
            let origin = if methods.insert(exported_name.to_ascii_lowercase()) {
                let id = self.push_symbol(
                    exported_name.to_string(),
                    SymbolKind::Method,
                    location(*rule),
                    Some(class.clone()),
                );
                self.add_qualified_alias(id, &format!("{class}::{exported_name}"));
                self.details.push(SymbolDetail {
                    symbol: id,
                    signature: Some(text(*rule, self.source).to_string()),
                    ..Default::default()
                });
                id
            } else {
                owner
            };
            self.push_targeted(
                origin,
                RelationKind::References,
                method_name.to_string(),
                location(method),
                Some(RelationTarget {
                    qualifier: Some(source.name),
                    target_module: Some(namespace_alias(&source.namespace)),
                    kind: Some(SymbolKind::Method),
                    ..Default::default()
                }),
            );
        }
    }

    fn trait_adaptation<'tree>(
        &self,
        rule: Node<'tree>,
        rules: &[Node<'tree>],
        traits: &[ResolvedName],
    ) -> Option<(ResolvedName, Node<'tree>, Node<'tree>)> {
        let mut parts = named_children(rule)
            .into_iter()
            .filter(|node| node.kind() != "comment");
        let original = parts.next()?;
        let (source, method) = if original.kind() == "class_constant_access_expression" {
            self.trait_method(original)?
        } else {
            // For an unqualified alias, a single trait or a precedence rule
            // supplies the declaring trait. Other cases need cross-file lookup.
            let method_name = text(original, self.source);
            let source = match traits {
                [only] => only.clone(),
                _ => {
                    rules
                        .iter()
                        .filter(|rule| rule.kind() == "use_instead_of_clause")
                        .filter_map(|rule| self.trait_method(rule.named_child(0)?))
                        .find(|(_, method)| {
                            text(*method, self.source).eq_ignore_ascii_case(method_name)
                        })?
                        .0
                }
            };
            (source, original)
        };
        if !traits.iter().any(|used| {
            used.name.eq_ignore_ascii_case(&source.name)
                && used.namespace.eq_ignore_ascii_case(&source.namespace)
        }) {
            return None;
        }
        let exported = match rule.kind() {
            "use_as_clause" => parts.find(|node| node.kind() == "name")?,
            "use_instead_of_clause" => method,
            _ => return None,
        };
        Some((source, method, exported))
    }

    fn trait_method<'tree>(&self, node: Node<'tree>) -> Option<(ResolvedName, Node<'tree>)> {
        let mut cursor = node.walk();
        let mut parts = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() != "comment");
        let scope = parts.next()?;
        let class = self.scope.resolve_class(text(scope, self.source))?;
        Some((class, parts.next()?))
    }
}
