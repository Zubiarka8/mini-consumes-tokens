//! Literal Artisan commands and command references.

use mct_core::SymbolId;
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::keys::KeyKind;
use crate::{literal_string_text, Walker};

impl Walker<'_> {
    pub(super) fn laravel_command(&mut self, argument: Node, caller: Option<SymbolId>) {
        if let Some(signature) = literal_string_text(argument, self.source) {
            if let Some(name) = signature.split_whitespace().next() {
                match caller {
                    Some(owner) => self.laravel_key_reference(
                        owner,
                        KeyKind::Command,
                        name,
                        location(argument),
                    ),
                    None => {
                        self.laravel_key_declaration(
                            KeyKind::Command,
                            name.to_string(),
                            location(argument),
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn laravel_command_class(&mut self, class: Node) {
        if self.extends_framework(class, r"Illuminate\Console", "Command") {
            if let Some(value) = self.property_default(class, "$signature") {
                self.laravel_command(value, None);
            }
        }
    }
}
