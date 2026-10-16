//! `LanguageParser` implementation for Python, via `tree-sitter-python`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for Python support.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location};
use tree_sitter::{Node, Parser};

#[cfg(test)]
mod libraries;

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["py", "pyi"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_python::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("tree-sitter-python grammar is statically valid");

        let tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree".to_string(),
            })?;

        let root = tree.root_node();
        if root.has_error() {
            let error_node = first_error(root).unwrap_or(root);
            return Err(ParseError::Syntax {
                path: file.relative_path.clone(),
                line: error_node.start_position().row as u32 + 1,
                message: "syntax error".to_string(),
            });
        }

        let module_name = module_name_for(&file.relative_path);
        let mut walker = Walker::new(&file.contents, &file.relative_path);
        let mut module_location = location(root);
        // The root node of a file ending in a newline ends at column 0 of
        // the (empty) line after the last one; the file's last line is the
        // one before that.
        let end = root.end_position();
        if end.column == 0 && end.row > 0 {
            module_location.end_line = Some(end.row as u32);
        }
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location, None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".pyi")
        .trim_end_matches(".py")
        .to_string()
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

struct Walker<'a> {
    source: &'a str,
    /// This file's repository-relative path, the base of relative imports.
    path: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
            next_id: 0,
        }
    }

    fn push_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        location: Location,
        parent: Option<String>,
    ) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord {
            id,
            name,
            kind,
            location,
            parent,
            level: None,
        });
        id
    }

    fn push_relation(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        to_name: String,
        loc: Location,
    ) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation {
            from,
            kind,
            to_name,
            location: loc,
        });
    }

    /// `owner` is the innermost enclosing function/method/module (calls and
    /// imports attach to it); `class_name` is the enclosing `class` name, so
    /// a `def` found directly in its body is recorded as a Method with that
    /// parent rather than a bare Function.
    fn visit_children(
        &mut self,
        node: Node,
        owner: SymbolId,
        class_name: Option<&str>,
        depth: u32,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, class_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, class_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "decorated_definition" => {
                let target_id = node
                    .child_by_field_name("definition")
                    .and_then(|definition| {
                        self.visit_definition(definition, owner, class_name, depth + 1)
                    });
                let mut cursor = node.walk();
                for decorator in node.children(&mut cursor) {
                    if decorator.kind() == "decorator" {
                        if let Some(expr) = decorator.named_child(0) {
                            if let Some(name) = expr_name(expr, self.source) {
                                self.push_relation(
                                    target_id.unwrap_or(owner),
                                    RelationKind::References,
                                    name,
                                    location(decorator),
                                );
                            }
                            // Decorator arguments run in the enclosing scope:
                            // `@renderer(fmt_name("csv"))` calls `fmt_name`
                            // from `owner`, not from the decorated symbol.
                            if expr.kind() == "call" {
                                if let Some(arguments) = expr.child_by_field_name("arguments") {
                                    self.visit_children(arguments, owner, class_name, depth + 1);
                                }
                            }
                        }
                    }
                }
            }
            "function_definition" | "class_definition" => {
                self.visit_definition(node, owner, class_name, depth);
            }
            // PEP 695 `type Name[T] = ...`.
            "type_alias_statement" => {
                if let Some(name) = node
                    .child_by_field_name("left")
                    .and_then(|left| first_identifier(left, depth))
                {
                    let name = text(name, self.source).to_string();
                    self.push_symbol(
                        name,
                        SymbolKind::TypeAlias,
                        location(node),
                        class_name.map(str::to_string),
                    );
                }
            }
            "import_statement" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.visit_import_name(child, owner);
                }
            }
            "import_from_statement" => {
                let module_child_id = node.child_by_field_name("module_name").map(|n| n.id());
                if let Some(module_node) = node.child_by_field_name("module_name") {
                    self.visit_from_module(module_node, owner);
                }
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if Some(child.id()) == module_child_id {
                        continue;
                    }
                    for (name, loc) in import_names(child, self.source) {
                        self.push_relation(owner, RelationKind::Imports, name, loc);
                    }
                }
            }
            "call" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some((name, name_node)) = expr_name_node(function, self.source) {
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    }
                    self.visit(function, owner, class_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, class_name, depth + 1);
                }
            }
            _ => self.visit_children(node, owner, class_name, depth + 1),
        }
    }

    /// Creates the symbol for a `function_definition`/`class_definition`
    /// node and walks its body, returning the new symbol's id so a wrapping
    /// `decorated_definition` can attach the decorator's `References`
    /// relation to the decorated symbol itself rather than its enclosing
    /// owner. Any other node kind is dispatched to `visit` and yields `None`
    /// (a decorator on something else has no symbol to attach to).
    fn visit_definition(
        &mut self,
        node: Node,
        owner: SymbolId,
        class_name: Option<&str>,
        depth: u32,
    ) -> Option<SymbolId> {
        match node.kind() {
            "function_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if class_name.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let id =
                    self.push_symbol(name, kind, location(node), class_name.map(str::to_string));
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, None, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
                Some(id)
            }
            "class_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                // A class nested directly in another class's body belongs to
                // it, like a method does.
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Class,
                    location(node),
                    class_name.map(str::to_string),
                );
                if let Some(superclasses) = node.child_by_field_name("superclasses") {
                    let mut cursor = superclasses.walk();
                    for base in superclasses.named_children(&mut cursor) {
                        if let Some(base_name) = expr_name(base, self.source) {
                            self.push_relation(
                                id,
                                RelationKind::Extends,
                                base_name,
                                location(base),
                            );
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
                Some(id)
            }
            _ => {
                self.visit(node, owner, class_name, depth);
                None
            }
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            ..Default::default()
        }
    }

    /// An `Imports` relation to the `Module` symbol of an imported module
    /// file, with the evidence the import statement proves about it.
    fn push_module_import(
        &mut self,
        owner: SymbolId,
        name: String,
        loc: Location,
        module: Option<String>,
    ) {
        if name.is_empty() {
            return;
        }
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name, loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module,
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
    }

    /// `import a.b.c [as x]`: the module `c`, which must live under `b`.
    /// The alias, when present, is still recorded as the local name.
    fn visit_import_name(&mut self, child: Node, owner: SymbolId) {
        let dotted = match child.kind() {
            "dotted_name" => Some(child),
            "aliased_import" => child
                .child_by_field_name("name")
                .filter(|n| n.kind() == "dotted_name"),
            _ => None,
        };
        let aliased = child.kind() == "aliased_import";
        if !aliased {
            // Unaliased: the module itself is the local name.
            if let Some(dotted) = dotted {
                let (name, module, loc) = dotted_parts(dotted, self.source);
                self.push_module_import(owner, name, loc, module);
            }
            return;
        }
        for (name, loc) in import_names(child, self.source) {
            self.push_relation(owner, RelationKind::Imports, name, loc);
        }
        if let Some(dotted) = dotted {
            let (name, module, loc) = dotted_parts(dotted, self.source);
            self.push_module_import(owner, name, loc, module);
        }
    }

    /// `from [.]a.b import …`: the module `b`, which must live under `a`
    /// (or, for `from .b`, under this file's package directory).
    fn visit_from_module(&mut self, module_node: Node, owner: SymbolId) {
        match module_node.kind() {
            "dotted_name" => {
                let (name, module, loc) = dotted_parts(module_node, self.source);
                self.push_module_import(owner, name, loc, module);
            }
            "relative_import" => {
                let mut cursor = module_node.walk();
                let mut dots = 0usize;
                let mut dotted = None;
                for part in module_node.named_children(&mut cursor) {
                    match part.kind() {
                        "import_prefix" => dots = text(part, self.source).matches('.').count(),
                        "dotted_name" => dotted = Some(part),
                        _ => {}
                    }
                }
                // `from . import x` names no module file of its own.
                let Some(dotted) = dotted else { return };
                let (name, module, loc) = dotted_parts(dotted, self.source);
                let module = module.or_else(|| relative_package(self.path, dots));
                self.push_module_import(owner, name, loc, module);
            }
            _ => {}
        }
    }
}

/// `a.b.c` -> (`c`, `Some("b")`, location of `c`).
fn dotted_parts(dotted: Node, source: &str) -> (String, Option<String>, Location) {
    let count = dotted.named_child_count();
    let Some(last) = count.checked_sub(1).and_then(|i| dotted.named_child(i)) else {
        return (String::new(), None, location(dotted));
    };
    let previous = count
        .checked_sub(2)
        .and_then(|i| dotted.named_child(i))
        .map(|n| text(n, source).to_string());
    (text(last, source).to_string(), previous, location(last))
}

/// The directory name `dots` leading dots reach from `path`'s package: one
/// dot is the file's own directory, each further dot climbs one level.
/// `None` at the repository root, where no directory name proves anything.
fn relative_package(path: &str, dots: usize) -> Option<String> {
    let mut dirs: Vec<&str> = path.split('/').collect();
    dirs.pop();
    let keep = dirs.len().checked_sub(dots.saturating_sub(1))?;
    dirs.get(keep.checked_sub(1)?)
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string())
}

/// The first `identifier` at or under `node` in document order — the alias
/// name of a `type` statement's left side (`Pair` in `Pair[T]`).
fn first_identifier(node: Node, depth: u32) -> Option<Node> {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return None;
    }
    if node.kind() == "identifier" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .find_map(|child| first_identifier(child, depth + 1))
}

/// Name of a callable/base-class expression: a bare identifier, or the
/// trailing attribute of a dotted access (`module.Class`, `obj.method`).
fn expr_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(text(node, source).to_string()),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|n| text(n, source).to_string()),
        "call" => node
            .child_by_field_name("function")
            .and_then(|n| expr_name(n, source)),
        _ => None,
    }
}

/// Same as [`expr_name`], but also returns the specific node the name came
/// from — never the enclosing `call`, whose start position is shared by
/// every call in a chain (`a.f(x).f(y)`'s outer and inner `call` both start
/// at `a`), which would otherwise make two same-named chained calls collide
/// into one indistinguishable relation row.
fn expr_name_node<'a>(node: Node<'a>, source: &str) -> Option<(String, Node<'a>)> {
    match node.kind() {
        "identifier" => Some((text(node, source).to_string(), node)),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|n| (text(n, source).to_string(), n)),
        "call" => node
            .child_by_field_name("function")
            .and_then(|n| expr_name_node(n, source)),
        _ => None,
    }
}

/// Names introduced by one element of an `import`/`from ... import` clause:
/// a `dotted_name` (last segment), a `wildcard_import` (skipped — no single
/// name), or an `aliased_import` (its alias, or the underlying name).
fn import_names(node: Node, source: &str) -> Vec<(String, Location)> {
    match node.kind() {
        "dotted_name" => {
            let last = node.named_child(node.named_child_count().saturating_sub(1));
            last.map(|n| vec![(text(n, source).to_string(), location(n))])
                .unwrap_or_default()
        }
        "aliased_import" => {
            if let Some(alias) = node.child_by_field_name("alias") {
                vec![(text(alias, source).to_string(), location(alias))]
            } else if let Some(name) = node.child_by_field_name("name") {
                import_names(name, source)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}
