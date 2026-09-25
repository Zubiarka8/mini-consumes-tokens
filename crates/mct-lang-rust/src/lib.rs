//! `LanguageParser` implementation for Rust, via `tree-sitter-rust`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for Rust support.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind,
    SourceFile, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct RustParser;

impl LanguageParser for RustParser {
    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_rust::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("tree-sitter-rust grammar is statically valid");

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
        .trim_end_matches(".rs")
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
            level: None,
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

    /// Walks every child of `node`, attributing calls/imports found along the
    /// way to `owner` (the innermost enclosing function/method/module) and
    /// labeling methods with `impl_type` (the enclosing `impl Type` name, if
    /// any) as their parent.
    fn visit_children(&mut self, node: Node, owner: SymbolId, impl_type: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, impl_type, depth + 1);
        }
    }

    /// `depth` bounds native stack usage against adversarially deep/nested
    /// input (see `MAX_TRAVERSAL_DEPTH`) — every recursive call below passes
    /// `depth + 1`, and this early-return prunes the subtree instead of
    /// recursing further once the ceiling is hit.
    fn visit(&mut self, node: Node, owner: SymbolId, impl_type: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_item" | "function_signature_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if impl_type.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let id = self.push_symbol(name, kind, location(node), impl_type.map(str::to_string));
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, impl_type, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, impl_type, depth + 1);
                }
            }
            "struct_item" => {
                self.push_named(node, SymbolKind::Struct, None);
            }
            "enum_item" => {
                self.push_named(node, SymbolKind::Enum, None);
            }
            "type_item" => {
                self.push_named(node, SymbolKind::TypeAlias, None);
            }
            "const_item" | "static_item" => {
                let id = self.push_named(node, SymbolKind::Constant, None);
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_children(value, id, impl_type, depth + 1);
                }
            }
            "trait_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), SymbolKind::Trait, location(node), None);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
            }
            "impl_item" => {
                let type_name = node
                    .child_by_field_name("type")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                if let Some(trait_node) = node.child_by_field_name("trait") {
                    let trait_name = text(trait_node, self.source).to_string();
                    self.push_relation(owner, RelationKind::Implements, trait_name, location(node));
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&type_name), depth + 1);
                }
            }
            "mod_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Module, location(node), None);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
            }
            "use_declaration" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    let mut names = Vec::new();
                    collect_use_names(argument, self.source, &mut names);
                    for (name, loc) in names {
                        self.push_relation(owner, RelationKind::Imports, name, loc);
                    }
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some((name, name_node)) = call_target(function, self.source) {
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    }
                    self.visit(function, owner, impl_type, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, impl_type, depth + 1);
                }
            }
            _ => self.visit_children(node, owner, impl_type, depth + 1),
        }
    }

    fn push_named(&mut self, node: Node, kind: SymbolKind, parent: Option<String>) -> SymbolId {
        let name = node
            .child_by_field_name("name")
            .map(|n| text(n, self.source).to_string())
            .unwrap_or_default();
        self.push_symbol(name, kind, location(node), parent)
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
}

/// The callee's name *and* the specific node it came from — never the whole
/// `call_expression`, whose start position is shared by every call in a
/// chain (`a.f(x).f(y)`'s outer and inner `call_expression` both start at
/// `a`), which would otherwise make two same-named chained calls collide
/// into one indistinguishable relation row.
fn call_target<'a>(node: Node<'a>, source: &str) -> Option<(String, Node<'a>)> {
    match node.kind() {
        "identifier" => Some((text(node, source).to_string(), node)),
        "field_expression" => {
            node.child_by_field_name("field").map(|n| (text(n, source).to_string(), n))
        }
        "scoped_identifier" => {
            node.child_by_field_name("name").map(|n| (text(n, source).to_string(), n))
        }
        "generic_function" => {
            node.child_by_field_name("function").and_then(|n| call_target(n, source))
        }
        _ => None,
    }
}

fn collect_use_names(node: Node, source: &str, out: &mut Vec<(String, Location)>) {
    match node.kind() {
        "identifier" | "type_identifier" => out.push((text(node, source).to_string(), location(node))),
        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.push((text(name, source).to_string(), location(name)));
            }
        }
        "use_as_clause" => {
            if let Some(alias) = node.child_by_field_name("alias") {
                out.push((text(alias, source).to_string(), location(alias)));
            } else if let Some(path) = node.child_by_field_name("path") {
                collect_use_names(path, source, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_use_names(child, source, out);
            }
        }
        "scoped_use_list" => {
            if let Some(list) = node.child_by_field_name("list") {
                collect_use_names(list, source, out);
            }
        }
        _ => {}
    }
}
