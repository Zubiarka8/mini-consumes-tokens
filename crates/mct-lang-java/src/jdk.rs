//! Which type names a Java file can only mean as the JDK's: those a
//! single-type import of a `java.` or `javax.` type binds
//! (`import java.util.List;`). A single-type import shadows every type of
//! the same package and every on-demand import (JLS §6.4.1), so `List.of()`
//! or `items.add()` on a `List items` there never reaches a repository type
//! named `List`.
//!
//! Not proven: a name the file also declares as a type or a type parameter
//! anywhere (a nested `class List` or `<Map>` shadows the import inside
//! it), two imports of one simple name, static and on-demand imports, and
//! `java.lang` names used without an import (`Math`, `String`), which a
//! class of the same package in another file can shadow.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

#[derive(Default)]
pub(crate) struct JdkImports {
    names: HashSet<String>,
}

impl JdkImports {
    pub(crate) fn of(root: Node, source: &str) -> Self {
        let mut imported: HashMap<String, (usize, bool)> = HashMap::new();
        let mut declared = HashSet::new();
        let complete = collect(root, source, &mut imported, &mut declared, 0);
        let names = if complete {
            imported
                .into_iter()
                .filter(|(name, (count, jdk))| *count == 1 && *jdk && !declared.contains(name))
                .map(|(name, _)| name)
                .collect()
        } else {
            HashSet::new()
        };
        Self { names }
    }

    /// `name`, as a type in this file, is the JDK type it imports.
    pub(crate) fn is_jdk(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// `false` when the walk was cut short and so can't vouch for anything.
fn collect(
    node: Node,
    source: &str,
    imported: &mut HashMap<String, (usize, bool)>,
    declared: &mut HashSet<String>,
    depth: u32,
) -> bool {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return false;
    }
    match node.kind() {
        "import_declaration" => {
            let mut cursor = node.walk();
            let on_demand_or_static = node
                .children(&mut cursor)
                .any(|c| matches!(c.kind(), "static" | "asterisk"));
            if !on_demand_or_static {
                if let Some(path) = node.named_child(0) {
                    let path = text(path, source);
                    let jdk = path.starts_with("java.") || path.starts_with("javax.");
                    if let Some(name) = path.rsplit('.').next() {
                        let entry = imported.entry(name.trim().to_string()).or_default();
                        entry.0 += 1;
                        entry.1 = jdk;
                    }
                }
            }
            return true;
        }
        "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "record_declaration"
        | "annotation_type_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                declared.insert(text(name, source).to_string());
            }
        }
        "type_parameter" => {
            let mut cursor = node.walk();
            let name = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "type_identifier" || c.kind() == "identifier");
            if let Some(name) = name {
                declared.insert(text(name, source).to_string());
            }
        }
        _ => {}
    }
    let mut complete = true;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        complete &= collect(child, source, imported, declared, depth + 1);
    }
    complete
}
