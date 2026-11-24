//! Macro registrations on framework types with a statically known receiver.

use mct_core::{RelationTarget, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::{is_identifier, routes::argument_values, views::application_root};
use crate::{
    details, last_segment, literal_string_text, named_children, namespace_alias, text,
    ClassContext, Walker,
};

impl Walker<'_> {
    pub(crate) fn visit_laravel_macro(
        &mut self,
        call: Node,
        arguments: Node,
        owner: SymbolId,
        type_name: Option<&str>,
        depth: u32,
    ) -> bool {
        let Some((name, implementation, id)) = self.declare_laravel_macro(call) else {
            return false;
        };
        if !matches!(
            implementation.kind(),
            "anonymous_function" | "arrow_function"
        ) {
            return false;
        }
        self.details
            .push(details::function_detail(implementation, id, self.source));
        for argument in named_children(arguments) {
            if implementation.parent() == Some(argument) {
                // Macroable rebinds the closure. The surrounding service
                // provider's type is not evidence for its receiver.
                self.classes.push(ClassContext::Anonymous);
                self.visit_attributes(implementation, id, depth + 1);
                self.visit_callable_parts(implementation, id, &name, None, depth + 1);
                self.classes.pop();
            } else {
                self.visit(argument, owner, type_name, depth + 2);
            }
        }
        true
    }

    fn declare_laravel_macro<'tree>(
        &mut self,
        call: Node<'tree>,
    ) -> Option<(String, Node<'tree>, SymbolId)> {
        let method = call.child_by_field_name("name")?;
        if method.kind() != "name" || !text(method, self.source).eq_ignore_ascii_case("macro") {
            return None;
        }
        let scope = call.child_by_field_name("scope")?;
        last_segment(scope)?;
        let class = self.scope.resolve_class(text(scope, self.source))?;
        let parent = macro_parent(self.path, &namespace_alias(&class.namespace), &class.name)?;
        let args = argument_values(call);
        let [name, implementation] = args.as_slice() else {
            return None;
        };
        let name_text =
            literal_string_text(*name, self.source).filter(|name| is_identifier(name))?;
        let id = self.push_symbol(
            name_text.clone(),
            SymbolKind::Method,
            location(*name),
            Some(parent),
        );
        Some((name_text, *implementation, id))
    }

    pub(crate) fn laravel_macro_target(
        &self,
        original: &RelationTarget,
        method: &str,
    ) -> Option<RelationTarget> {
        if ["macro", "hasMacro", "mixin", "flushMacros"]
            .iter()
            .any(|name| name.eq_ignore_ascii_case(method))
        {
            return None;
        }
        let namespace = original.target_module.as_deref()?;
        let class = original.qualifier.as_deref()?;
        Some(RelationTarget {
            qualifier: Some(macro_parent(self.path, namespace, class)?),
            kind: Some(SymbolKind::Method),
            ..Default::default()
        })
    }
}

fn macro_parent(path: &str, namespace: &str, class: &str) -> Option<String> {
    let class = if namespace.eq_ignore_ascii_case(r"\Illuminate\Support") {
        ["Collection", "Str"]
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(class))?
    } else if namespace.eq_ignore_ascii_case(r"\Illuminate\Database\Query")
        && class.eq_ignore_ascii_case("Builder")
    {
        "Builder"
    } else {
        return None;
    };
    let root = application_root(path)?;
    Some(format!("laravel:macro:{root}:{class}"))
}
