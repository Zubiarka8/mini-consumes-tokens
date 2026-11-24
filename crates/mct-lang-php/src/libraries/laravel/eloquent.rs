//! Eloquent metadata visible in literal model declarations and morph maps.

use std::collections::HashSet;

use mct_core::{RelationKind, RelationTarget, SymbolDetail, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::keys::KeyKind;
use crate::{literal_string_text, named_children, namespace_alias, text, ClassContext, Walker};

impl Walker<'_> {
    pub(crate) fn laravel_model(
        &mut self,
        class: Node,
        owner: SymbolId,
        name: &str,
    ) -> HashSet<String> {
        let mut attributes = HashSet::new();
        let Some(base) = self.framework_base(class) else {
            return attributes;
        };
        let model_base = [
            (r"Illuminate\Database\Eloquent", "Model"),
            (r"Illuminate\Foundation\Auth", "User"),
        ];
        if !model_base.iter().any(|(namespace, name)| {
            base.namespace.eq_ignore_ascii_case(namespace) && base.name.eq_ignore_ascii_case(name)
        }) {
            return attributes;
        }
        if let Some(value) = self.property_default(class, "$table") {
            self.laravel_literal_key(owner, KeyKind::Table, value);
        }
        let Some(body) = class.child_by_field_name("body") else {
            return attributes;
        };
        let methods = named_children(body);
        for method in methods
            .iter()
            .filter(|node| node.kind() == "method_declaration")
        {
            if let Some(attribute) = self.eloquent_accessor(*method) {
                self.declare_eloquent_attribute(&mut attributes, name, attribute, *method, None);
            }
        }
        let literal_casts = methods
            .iter()
            .filter(|node| node.kind() == "method_declaration")
            .filter(|node| {
                node.child_by_field_name("name")
                    .is_some_and(|method| text(method, self.source).eq_ignore_ascii_case("casts"))
            })
            .filter_map(|node| node.child_by_field_name("body"))
            .flat_map(named_children)
            .filter(|node| node.kind() == "return_statement")
            .filter_map(|node| node.named_child(0));
        for casts in literal_casts.chain(self.property_default(class, "$casts")) {
            for (key, value) in literal_entries(casts) {
                if let Some(attribute) = literal_string_text(key, self.source) {
                    self.declare_eloquent_attribute(
                        &mut attributes,
                        name,
                        attribute,
                        key,
                        Some(value),
                    );
                }
            }
        }
        attributes
    }

    fn declare_eloquent_attribute(
        &mut self,
        attributes: &mut HashSet<String>,
        class: &str,
        name: String,
        at: Node,
        cast: Option<Node>,
    ) {
        if name.is_empty() || !attributes.insert(name.clone()) {
            return;
        }
        let id = self.push_symbol(
            name.clone(),
            SymbolKind::Field,
            location(at),
            Some(class.to_string()),
        );
        self.add_qualified_alias(id, &format!("{class}::${name}"));
        if let Some(cast) = cast {
            self.details.push(SymbolDetail {
                symbol: id,
                signature: Some(format!(
                    "{} => {}",
                    text(at, self.source),
                    text(cast, self.source)
                )),
                ..Default::default()
            });
        }
    }

    fn eloquent_accessor(&self, method: Node) -> Option<String> {
        let name = method.child_by_field_name("name")?;
        let return_type = method.child_by_field_name("return_type")?;
        let ty = self.scope.resolve_class(text(return_type, self.source))?;
        if !ty.name.eq_ignore_ascii_case("Attribute")
            || !ty
                .namespace
                .eq_ignore_ascii_case(r"Illuminate\Database\Eloquent\Casts")
        {
            return None;
        }
        let mut attribute = String::new();
        for (index, ch) in text(name, self.source)
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .enumerate()
        {
            if index > 0 && ch.is_uppercase() {
                attribute.push('_');
            }
            attribute.extend(ch.to_lowercase());
        }
        Some(attribute)
    }

    pub(crate) fn laravel_accessor_reference(&mut self, method: Node, owner: SymbolId) {
        if let Some(attribute) = self.eloquent_accessor(method) {
            self.reference_eloquent_attribute(owner, &attribute, method);
        }
    }

    pub(crate) fn laravel_attribute_access(&mut self, access: Node, owner: SymbolId) {
        let (Some(object), Some(name)) = (
            access.child_by_field_name("object"),
            access.child_by_field_name("name"),
        ) else {
            return;
        };
        if text(object, self.source) == "$this" && name.kind() == "name" {
            self.reference_eloquent_attribute(owner, text(name, self.source), name);
        }
    }

    fn reference_eloquent_attribute(&mut self, owner: SymbolId, attribute: &str, at: Node) {
        if !matches!(
            self.classes.last(),
            Some(ClassContext::Named { eloquent_attributes, .. }) if eloquent_attributes.contains(attribute)
        ) {
            return;
        }
        let mut target = self.current_class_target();
        target.kind = Some(SymbolKind::Field);
        self.push_targeted(
            owner,
            RelationKind::References,
            attribute.to_string(),
            location(at),
            Some(target),
        );
    }

    pub(super) fn laravel_schema_table(&mut self, argument: Node) {
        if let Some(table) = literal_string_text(argument, self.source) {
            self.laravel_key_declaration(KeyKind::Table, table, location(argument));
        }
    }

    pub(super) fn laravel_morph_map(
        &mut self,
        scope: Node,
        method: &str,
        args: &[Node],
        owner: SymbolId,
    ) {
        if !matches!(method, "morphMap" | "enforceMorphMap" | "getMorphedModel") {
            return;
        }
        let Some(class) = crate::last_segment(scope)
            .and_then(|_| self.scope.resolve_class(text(scope, self.source)))
        else {
            return;
        };
        if !class
            .namespace
            .eq_ignore_ascii_case(r"Illuminate\Database\Eloquent\Relations")
            || !class.name.eq_ignore_ascii_case("Relation")
        {
            return;
        }
        let Some(argument) = args.first().copied() else {
            return;
        };
        if method == "getMorphedModel" {
            self.laravel_literal_key(owner, KeyKind::Morph, argument);
            return;
        }
        for (key, value) in literal_entries(argument) {
            let Some(alias) = literal_string_text(key, self.source) else {
                continue;
            };
            let class = self.class_constant(value).or_else(|| {
                // A class name in a string is absolute. Imports only apply to syntax.
                let written = literal_string_text(value, self.source)?;
                let name = written.strip_prefix('\\').unwrap_or(&written);
                let valid = name.split('\\').all(|segment| {
                    let mut chars = segment.chars();
                    chars
                        .next()
                        .is_some_and(|c| c == '_' || c.is_alphabetic() || !c.is_ascii())
                        && chars.all(|c| c == '_' || c.is_alphanumeric() || !c.is_ascii())
                });
                valid
                    .then(|| self.scope.resolve_class(&format!("\\{name}")))
                    .flatten()
            });
            let Some(class) = class else { continue };
            if let Some(symbol) = self.laravel_key_declaration(KeyKind::Morph, alias, location(key))
            {
                self.push_targeted(
                    symbol,
                    RelationKind::References,
                    class.name,
                    location(value),
                    Some(RelationTarget {
                        kind: Some(SymbolKind::Class),
                        target_module: Some(namespace_alias(&class.namespace)),
                        ..Default::default()
                    }),
                );
            }
        }
    }
}

fn literal_entries(array: Node<'_>) -> impl Iterator<Item = (Node<'_>, Node<'_>)> {
    named_children(crate::without_parentheses(array))
        .into_iter()
        .filter_map(move |entry| {
            if entry.kind() != "array_element_initializer" {
                return None;
            }
            let mut parts = named_children(entry)
                .into_iter()
                .filter(|part| part.kind() != "comment");
            let (key, value) = (parts.next()?, parts.next()?);
            parts.next().is_none().then_some((key, value))
        })
}
