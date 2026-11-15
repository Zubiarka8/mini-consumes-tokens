//! `using Alias = Target;`: the alias names its target, so a use of
//! `Alias` later in the same file (`Alias.M()`, `new Alias()`) is a use of
//! the target. A C# alias is visible only in the file (or namespace block)
//! that declares it, so the file's own directives are the evidence; a
//! `global using` alias is visible project-wide and is not recorded, so its
//! uses stay as they were. The alias itself is no symbol: the index has no
//! way to keep a symbol out of other files' name matching (#446).

use tree_sitter::Node;

use crate::{text, Walker};

/// The alias name and target of `using Alias = Target;`, or `None` for a
/// plain or `static` directive.
pub(crate) fn alias_parts(directive: Node) -> Option<(Node, Node)> {
    let name = directive.child_by_field_name("name")?;
    let mut cursor = directive.walk();
    let target = directive
        .named_children(&mut cursor)
        .filter(|c| c.id() != name.id())
        .last()?;
    Some((name, target))
}

/// A target that names a namespace or type (`A.B`, `A.B<T>`, `global::A`),
/// not a tuple, array, pointer or predefined type.
pub(crate) fn is_name(target: Node) -> bool {
    matches!(
        target.kind(),
        "identifier" | "qualified_name" | "generic_name" | "alias_qualified_name"
    )
}

fn is_global(directive: Node) -> bool {
    let mut cursor = directive.walk();
    let found = directive
        .children(&mut cursor)
        .any(|c| c.kind() == "global");
    found
}

impl Walker<'_> {
    /// Records `using Alias = Target;` (outside `global using`) when the
    /// target is a name.
    pub(crate) fn record_using_alias(&mut self, directive: Node) {
        let Some((name, target)) = alias_parts(directive) else {
            return;
        };
        if is_global(directive) || !is_name(target) {
            return;
        }
        let target: String = text(target, self.source).split_whitespace().collect();
        self.aliases
            .insert(text(name, self.source).to_string(), target);
    }

    /// The target an alias of this file names (`System.Decimal` for
    /// `Money`).
    pub(crate) fn alias_target(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(String::as_str)
    }
}
