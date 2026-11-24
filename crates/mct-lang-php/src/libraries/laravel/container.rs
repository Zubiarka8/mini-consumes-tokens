//! Source-declared container bindings and explicit policy registrations.

use mct_core::SymbolId;
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::keys::KeyKind;
use super::routes::argument_values;
use crate::{literal_string_text, text, Walker};

const BINDING_METHODS: &[&str] = &[
    "bind",
    "bindIf",
    "singleton",
    "singletonIf",
    "scoped",
    "scopedIf",
    "instance",
];

impl Walker<'_> {
    pub(super) fn binding_name(&self, argument: Node) -> Option<String> {
        if let Some(class) = self.class_constant(argument) {
            return Some(if class.namespace.is_empty() {
                class.name
            } else {
                format!(r"{}\{}", class.namespace, class.name)
            });
        }
        literal_string_text(argument, self.source)
    }

    fn declare_binding(&mut self, kind: KeyKind, argument: Node) {
        if let Some(name) = self.binding_name(argument) {
            self.laravel_key_declaration(kind, name, location(argument));
        }
    }

    pub(super) fn laravel_service_registration(
        &mut self,
        scope: Node,
        method: &str,
        args: &[Node],
        owner: SymbolId,
    ) -> bool {
        let Some(argument) = args.first().copied() else {
            return false;
        };
        if self.is_facade(scope, "App") {
            if BINDING_METHODS.contains(&method) {
                self.declare_binding(KeyKind::Binding, argument);
                return true;
            }
            if matches!(method, "make" | "makeWith" | "resolve") {
                self.laravel_literal_key(owner, KeyKind::Binding, argument);
                return true;
            }
        }
        if self.is_facade(scope, "Gate") {
            if method == "policy" && args.len() == 2 {
                self.declare_binding(KeyKind::Policy, argument);
                return true;
            }
            if method == "getPolicyFor" {
                self.laravel_literal_key(owner, KeyKind::Policy, argument);
                return true;
            }
        }
        false
    }

    pub(super) fn laravel_provider_binding(&mut self, call: Node, method: &str) {
        if !BINDING_METHODS.contains(&method)
            || call
                .child_by_field_name("object")
                .map(|n| text(n, self.source))
                != Some("$this->app")
        {
            return;
        }
        let mut parent = call.parent();
        while let Some(node) = parent {
            if node.kind() == "class_declaration" {
                if self.extends_framework(node, r"Illuminate\Support", "ServiceProvider") {
                    if let Some(argument) = argument_values(call).first() {
                        self.declare_binding(KeyKind::Binding, *argument);
                    }
                }
                return;
            }
            if node.kind() == "anonymous_class" {
                return;
            }
            parent = node.parent();
        }
    }
}
