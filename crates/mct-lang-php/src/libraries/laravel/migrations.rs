//! Migrations declared as `return new class extends Migration { ... }`.

use mct_core::SymbolKind;
use mct_tree_sitter::location;
use tree_sitter::Node;

use crate::names::ResolvedName;
use crate::{find_child, last_segment, named_children, text, Walker};

const MIGRATION_NAMESPACE: &str = "Illuminate\\Database\\Migrations";

impl Walker<'_> {
    /// Whether the anonymous class `node` extends Laravel's `Migration`.
    pub(crate) fn is_anonymous_migration(&self, node: Node) -> bool {
        let Some(base) = find_child(node, "base_clause").and_then(|clause| {
            named_children(clause)
                .into_iter()
                .find(|c| last_segment(*c).is_some())
        }) else {
            return false;
        };
        matches!(
            self.scope.resolve_class(text(base, self.source)),
            Some(ResolvedName { namespace, name })
                if name == "Migration" && namespace == MIGRATION_NAMESPACE
        )
    }

    /// Indexes `up`/`down` and the like as methods of the migration file's
    /// module: an anonymous migration has no class name to hang them on,
    /// and the schema calls inside them belong to them, not to the file.
    pub(crate) fn visit_migration_body(&mut self, body: Node, depth: u32) {
        let module = self
            .symbols
            .get(self.module_id as usize)
            .map(|s| s.name.clone());
        for member in named_children(body) {
            if member.kind() != "method_declaration" {
                self.visit(member, self.module_id, None, depth + 1);
                continue;
            }
            let name = member
                .child_by_field_name("name")
                .map(|n| text(n, self.source).to_string())
                .unwrap_or_default();
            let id = self.push_symbol(name, SymbolKind::Method, location(member), module.clone());
            for field in ["parameters", "return_type", "body"] {
                if let Some(part) = member.child_by_field_name(field) {
                    self.visit_children(part, id, None, depth + 1);
                }
            }
        }
    }
}
