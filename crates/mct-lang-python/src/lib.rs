//! `LanguageParser` implementation for Python, via `tree-sitter-python`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for Python support.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

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

        let tree = parser.parse(&file.contents, None).ok_or_else(|| ParseError::Syntax {
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
        let mut walker = Walker::new(&file.contents);
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, location(root), None);
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

fn first_error(node: Node) -> Option<Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = first_error(child) {
            return Some(found);
        }
    }
    None
}

fn location(node: Node) -> Location {
    let start = node.start_position();
    let end = node.end_position();
    Location {
        line: start.row as u32 + 1,
        column: start.column as u32 + 1,
        byte_len: (node.end_byte() - node.start_byte()) as u32,
        end_line: Some(end.row as u32 + 1),
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
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
        });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
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
    fn visit_children(&mut self, node: Node, owner: SymbolId, class_name: Option<&str>, depth: u32) {
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
                let mut cursor = node.walk();
                for decorator in node.children(&mut cursor) {
                    if decorator.kind() == "decorator" {
                        if let Some(expr) = decorator.named_child(0) {
                            if let Some(name) = expr_name(expr, self.source) {
                                self.push_relation(
                                    owner,
                                    RelationKind::References,
                                    name,
                                    location(decorator),
                                );
                            }
                        }
                    }
                }
                if let Some(definition) = node.child_by_field_name("definition") {
                    self.visit(definition, owner, class_name, depth + 1);
                }
            }
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
                let id = self.push_symbol(name, kind, location(node), class_name.map(str::to_string));
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, None, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
            }
            "class_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), SymbolKind::Class, location(node), None);
                if let Some(superclasses) = node.child_by_field_name("superclasses") {
                    let mut cursor = superclasses.walk();
                    for base in superclasses.named_children(&mut cursor) {
                        if let Some(base_name) = expr_name(base, self.source) {
                            self.push_relation(id, RelationKind::Extends, base_name, location(base));
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
            }
            "import_statement" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    for (name, loc) in import_names(child, self.source) {
                        self.push_relation(owner, RelationKind::Imports, name, loc);
                    }
                }
            }
            "import_from_statement" => {
                let module_child_id = node.child_by_field_name("module_name").map(|n| n.id());
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

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
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
