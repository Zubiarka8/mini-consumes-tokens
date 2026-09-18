//! `LanguageParser` implementation for Go, via `tree-sitter-go`.
//!
//! **Deferred by design**: `find_implementations` is not implemented for Go.
//! Go interface satisfaction is structural (a type "implements" an interface
//! purely by having a matching method set — there is no `implements`
//! keyword anywhere in the source), so answering it correctly needs a real
//! type checker, not an AST walk. This parser only extracts what the syntax
//! actually declares: an `interface_type`'s own symbol and its declared
//! `method_elem`s (see the `type_spec` arm below) — never a guess at which
//! structs satisfy it. Revisit alongside the general LSP/type-resolution
//! decision (see `checklist.md`).

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct GoParser;

impl LanguageParser for GoParser {
    fn language_id(&self) -> &'static str {
        "go"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_go::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("tree-sitter-go grammar is statically valid");

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

        // A Go package spans multiple files, each restating its own
        // `package name` clause — same pattern C#'s `namespace_declaration`
        // uses (one Module-kind symbol per occurrence, not deduplicated
        // across files), so `find_symbol("billing")` returning one hit per
        // file that declares it is expected, not a bug.
        let package_name = find_child(root, "package_clause")
            .and_then(|pc| pc.named_child(0))
            .map(|n| text(n, &file.contents).to_string());

        let module_name = module_name_for(&file.relative_path);
        let mut walker = Walker::new(&file.contents);
        let file_module_id = walker.push_symbol(module_name, SymbolKind::Module, location(root), None);
        if let Some(ref pkg) = package_name {
            walker.push_symbol(pkg.clone(), SymbolKind::Module, location(root), None);
        }
        walker.visit_children(root, file_module_id, package_name.as_deref(), 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".go")
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

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|n| n.kind() == kind)
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, symbols: Vec::new(), relations: Vec::new(), next_id: 0 }
    }

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location, parent: Option<String>) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord { id, name, kind, location, parent });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    /// `owner` is the innermost enclosing function/method (calls attach to
    /// it); `package_name` is this file's package, used as `parent` for
    /// top-level functions/types/interfaces (Go has no further nesting of
    /// declarations below package scope).
    fn visit_children(&mut self, node: Node, owner: SymbolId, package_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, package_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, package_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_declaration" => {
                let name = node.child_by_field_name("name").map(|n| text(n, self.source).to_string()).unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Function, location(node), package_name.map(str::to_string));
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, package_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, package_name, depth + 1);
                }
            }
            "method_declaration" => {
                let name = node.child_by_field_name("name").map(|n| text(n, self.source).to_string()).unwrap_or_default();
                let receiver_type = receiver_type_name(node, self.source);
                let parent = receiver_type.or_else(|| package_name.map(str::to_string));
                let id = self.push_symbol(name, SymbolKind::Method, location(node), parent);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, package_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, package_name, depth + 1);
                }
            }
            "type_spec" => {
                let name = node.child_by_field_name("name").map(|n| text(n, self.source).to_string()).unwrap_or_default();
                match node.child_by_field_name("type") {
                    Some(type_node) if type_node.kind() == "struct_type" => {
                        self.push_symbol(name, SymbolKind::Struct, location(node), package_name.map(str::to_string));
                    }
                    Some(type_node) if type_node.kind() == "interface_type" => {
                        self.push_symbol(name.clone(), SymbolKind::Interface, location(node), package_name.map(str::to_string));
                        // The interface's own declared method set — NOT a
                        // resolution of which types implement it (see the
                        // module doc's "deferred by design" note above).
                        let mut cursor = type_node.walk();
                        for member in type_node.children(&mut cursor) {
                            if member.kind() == "method_elem" {
                                let method_name = member
                                    .child_by_field_name("name")
                                    .map(|n| text(n, self.source).to_string())
                                    .unwrap_or_default();
                                self.push_symbol(method_name, SymbolKind::Method, location(member), Some(name.clone()));
                            }
                        }
                    }
                    _ => {
                        // `type UserID int` and similar — recorded for
                        // completeness, out of the spec's required set but
                        // free given the trait already models `TypeAlias`.
                        self.push_symbol(name, SymbolKind::TypeAlias, location(node), package_name.map(str::to_string));
                    }
                }
            }
            "import_declaration" => {
                let mut specs = Vec::new();
                collect_import_specs(node, &mut specs);
                for spec in specs {
                    if let Some(path_node) = spec.child_by_field_name("path") {
                        let raw = text(path_node, self.source);
                        let clean = raw.trim_matches(|c| c == '"' || c == '`').to_string();
                        self.push_relation(owner, RelationKind::Imports, clean, location(spec));
                    }
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(name_node) = callee_identifier(function) {
                        // location(name_node), not location(node): a chained
                        // call (`a.F(x).F(y)`) has its outer and inner
                        // call_expression both start at `a`, which would make
                        // two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
                    }
                    self.visit(function, owner, package_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, package_name, depth + 1);
                }
            }
            _ => self.visit_children(node, owner, package_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

fn collect_import_specs<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => out.push(child),
            "import_spec_list" => collect_import_specs(child, out),
            _ => {}
        }
    }
}

/// The receiver's type name (`func (r *Invoice) AddItem(...)` → `"Invoice"`),
/// unwrapping the pointer for a pointer receiver — `Invoice` and `*Invoice`
/// methods both attach to the same `parent`, matching how a real Go method
/// set doesn't distinguish them for indexing purposes.
fn receiver_type_name(method_declaration: Node, source: &str) -> Option<String> {
    let receiver = method_declaration.child_by_field_name("receiver")?;
    let mut cursor = receiver.walk();
    let param = receiver.children(&mut cursor).find(|c| c.kind() == "parameter_declaration")?;
    let ty = param.child_by_field_name("type")?;
    let unwrapped = if ty.kind() == "pointer_type" { ty.named_child(0).unwrap_or(ty) } else { ty };
    Some(text(unwrapped, source).to_string())
}

/// The function/method name being invoked: a bare `identifier`, or the
/// `field` of `receiver.Name(...)`.
fn callee_identifier(function: Node) -> Option<Node> {
    match function.kind() {
        "identifier" => Some(function),
        "selector_expression" => function.child_by_field_name("field"),
        _ => None,
    }
}
