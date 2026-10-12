//! ES module import bindings and CommonJS require paths.

use super::{text, Walker};
use mct_core::{resolve_reference_path, RelationKind, RelationTarget, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

impl Walker<'_> {
    /// Records the file-level dependency of an `import`/`export … from`/
    /// `require`/`import()` on module `spec`, written in this file.
    ///
    /// A relative spec becomes an `Imports` relation to the target file's
    /// `Module` symbol (named after the file stem); a `node:` builtin is
    /// external. A bare specifier (`react`, `@/x`) is neither provably
    /// external (it can be a path alias or a workspace package) nor local,
    /// so it records nothing here. Returns whether the relation was named by
    /// the spec itself (a builtin), so a caller need not record it again.
    pub(super) fn push_module_import(
        &mut self,
        owner: SymbolId,
        spec: &str,
        loc: mct_core::Location,
    ) -> bool {
        let Some((name, target)) = module_import_target(self.path, spec) else {
            return false;
        };
        let named_by_spec = name == spec;
        let index = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name, loc);
        if self.relations.len() > index {
            self.relation_targets.push(RelationTarget {
                relation: index,
                ..target
            });
        }
        named_by_spec
    }

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

/// Extensions whose file is a JS/TS module symbol named after its stem.
/// `.ts`-family and `.jsx` are literal; `.js`/`.mjs`/`.cjs` may be a
/// TypeScript source written with its emitted extension, so they are treated
/// like an extensionless spec.
const EXACT_EXTENSIONS: [&str; 5] = [".tsx", ".mts", ".cts", ".ts", ".jsx"];
const EMITTED_EXTENSIONS: [&str; 3] = [".mjs", ".cjs", ".js"];

/// The relation name and evidence for a module specifier written in
/// `from_file`, or `None` when it proves nothing (bare specifier, computed
/// path, non-module asset).
fn module_import_target(from_file: &str, spec: &str) -> Option<(String, RelationTarget)> {
    if let Some(builtin) = spec.strip_prefix("node:") {
        if builtin.is_empty() {
            return None;
        }
        return Some((
            spec.to_string(),
            RelationTarget {
                external: true,
                ..Default::default()
            },
        ));
    }
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let resolved = resolve_reference_path(from_file, spec)?;
    let (dir, file) = match resolved.rsplit_once('/') {
        Some((dir, file)) => (Some(dir), file),
        None => (None, resolved.as_str()),
    };
    if file.is_empty() {
        return None;
    }
    if file.ends_with(".css") {
        // A stylesheet import links to the stylesheet's own Module symbol.
        let stem = file.trim_end_matches(".css");
        if stem.is_empty() {
            return None;
        }
        return Some((
            stem.to_string(),
            RelationTarget {
                path: Some(resolved.clone()),
                kind: Some(SymbolKind::Module),
                language: Some("css".to_string()),
                ..Default::default()
            },
        ));
    }
    if let Some(ext) = EXACT_EXTENSIONS.iter().find(|e| file.ends_with(**e)) {
        let stem = &file[..file.len() - ext.len()];
        if stem.is_empty() {
            return None;
        }
        return Some((
            stem.to_string(),
            RelationTarget {
                path: Some(resolved.clone()),
                kind: Some(SymbolKind::Module),
                ..Default::default()
            },
        ));
    }
    let stem = EMITTED_EXTENSIONS
        .iter()
        .find_map(|e| file.strip_suffix(*e))
        .unwrap_or(file);
    // Another extension (`.json`, `.svg`) is an asset, not a module.
    if stem.is_empty() || stem.contains('.') {
        return None;
    }
    // The extension is unknown (`./util` may be `util.ts` or `util.js`), so
    // the file cannot be named; only its parent directory and stem are
    // proven, which keeps a same-named module elsewhere from matching.
    Some((
        stem.to_string(),
        RelationTarget {
            module: dir
                .and_then(|d| d.rsplit('/').next())
                .filter(|d| !d.is_empty())
                .map(str::to_string),
            kind: Some(SymbolKind::Module),
            ..Default::default()
        },
    ))
}

/// The text of a string literal node (`"./x"`, `'./x'`), or `None` for a
/// template literal or other computed source.
pub(super) fn string_value(node: Node, source: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let mut cursor = node.walk();
    let fragment = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "string_fragment");
    fragment.map(|c| text(c, source).to_string())
}
