//! The named members of a TS interface and enum: symbols owned by their
//! type, so a member call or reference has a declaration to land on.

use super::{details, text, Walker};
use mct_core::SymbolKind;
use mct_tree_sitter::location;
use tree_sitter::Node;

impl Walker<'_> {
    /// `id: string` → `field`, `area(): number` and `get size()` →
    /// `method`. Call, construct and index signatures have no name and a
    /// computed name (`[key]: T`) is not a name the source spells, so
    /// neither is a symbol.
    pub(super) fn push_interface_members(&mut self, body: Node, interface: &str) {
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            let kind = match member.kind() {
                "property_signature" => SymbolKind::Field,
                "method_signature" => SymbolKind::Method,
                _ => continue,
            };
            let Some(name) = member
                .child_by_field_name("name")
                .and_then(|n| member_name(n, self.source))
            else {
                continue;
            };
            let id = self.push_symbol(name, kind, location(member), Some(interface.to_string()));
            let detail = match kind {
                SymbolKind::Method => details::function_detail(member, member, id, self.source),
                _ => details::field_detail(member, id, self.source),
            };
            self.details.push(detail);
        }
    }

    /// `Red`, `Green = "g"`, `'dark-blue' = 4` → `field` of the enum.
    pub(super) fn push_enum_members(&mut self, body: Node, enumeration: &str) {
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            let name_node = match member.kind() {
                "enum_assignment" => member.child_by_field_name("name"),
                "property_identifier" | "string" => Some(member),
                _ => None,
            };
            let Some(name) = name_node.and_then(|n| member_name(n, self.source)) else {
                continue;
            };
            let id = self.push_symbol(
                name,
                SymbolKind::Field,
                location(member),
                Some(enumeration.to_string()),
            );
            self.details
                .push(details::field_detail(member, id, self.source));
        }
    }
}

/// A member's name as written: an identifier, or a string's content
/// (`'css-name'` → `css-name`). `None` for a computed or empty name.
fn member_name(node: Node, source: &str) -> Option<String> {
    let name = match node.kind() {
        "property_identifier" | "private_property_identifier" => text(node, source),
        "string" => {
            let mut cursor = node.walk();
            let fragment = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "string_fragment")?;
            text(fragment, source)
        }
        _ => return None,
    };
    (!name.is_empty()).then(|| name.to_string())
}
