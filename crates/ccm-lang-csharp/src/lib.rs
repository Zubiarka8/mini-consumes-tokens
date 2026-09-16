//! `LanguageParser` implementation for C#, via `tree-sitter-c-sharp`.

use ccm_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct CSharpParser;

impl LanguageParser for CSharpParser {
    fn language_id(&self) -> &'static str {
        "csharp"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["cs"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_c_sharp::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("tree-sitter-c-sharp grammar is statically valid");

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
        walker.visit_children(root, module_id, None);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".cs")
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

    /// `owner` is the innermost enclosing method/property/module (calls
    /// attach to it); `type_name` is the innermost enclosing
    /// class/interface/struct/namespace name, used as `parent` for members
    /// declared directly inside it. Every `method_declaration` — including
    /// each overload — gets its own `SymbolRecord` row, so overloads are
    /// never collapsed.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>) {
        match node.kind() {
            "namespace_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(name.clone(), SymbolKind::Module, location(node), type_name.map(str::to_string));
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name));
                }
            }
            "class_declaration" | "interface_declaration" | "struct_declaration" => {
                let kind = match node.kind() {
                    "class_declaration" => SymbolKind::Class,
                    "interface_declaration" => SymbolKind::Interface,
                    _ => SymbolKind::Struct,
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), kind, location(node), type_name.map(str::to_string));

                // `base_list` is unified in this grammar — C# doesn't
                // syntactically distinguish "extends" from "implements", both
                // are one comma-separated list after `:`. Heuristic (AST-only,
                // no type resolution): C# requires the base class, when
                // present, to be listed first, so treat entry 0 as Extends
                // and the rest as Implements. A class with only interfaces
                // then mislabels entry 0 as Extends — accepted limitation,
                // same class of simplification as the LSP-deferral decision.
                if let Some(base_list) = find_child(node, "base_list") {
                    let mut cursor = base_list.walk();
                    let bases: Vec<Node> = base_list
                        .children(&mut cursor)
                        .filter(|n| n.kind() == "identifier" || n.kind() == "generic_name" || n.kind() == "qualified_name")
                        .collect();
                    for (i, base_node) in bases.iter().enumerate() {
                        let relation_kind = if i == 0 { RelationKind::Extends } else { RelationKind::Implements };
                        self.push_relation(id, relation_kind, text(*base_node, self.source).to_string(), location(*base_node));
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name));
                }
            }
            "method_declaration" | "constructor_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Method, location(node), type_name.map(str::to_string));
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, type_name);
                }
            }
            // A property (`Name { get; set; }`) is ONE symbol, not two —
            // the accessors are walked into using the property's own id as
            // owner, so a custom getter/setter's calls attach to the
            // property, and get/set are never indexed as separate
            // unrelated methods.
            "property_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Field, location(node), type_name.map(str::to_string));
                if let Some(accessors) = node.child_by_field_name("accessors") {
                    let mut cursor = accessors.walk();
                    for accessor in accessors.children(&mut cursor) {
                        if let Some(body) = accessor.child_by_field_name("body") {
                            self.visit_children(body, id, type_name);
                        }
                    }
                }
            }
            "field_declaration" => {
                if let Some(variable_declaration) = find_child(node, "variable_declaration") {
                    let mut cursor = variable_declaration.walk();
                    for declarator in variable_declaration.children(&mut cursor) {
                        if declarator.kind() == "variable_declarator" {
                            if let Some(name_node) = declarator.child_by_field_name("name") {
                                self.push_symbol(
                                    text(name_node, self.source).to_string(),
                                    SymbolKind::Field,
                                    location(declarator),
                                    type_name.map(str::to_string),
                                );
                            }
                        }
                    }
                }
            }
            "using_directive" => {
                if let Some(path) = node.named_child(0) {
                    if let Some(last) = last_identifier(path) {
                        self.push_relation(owner, RelationKind::Imports, text(last, self.source).to_string(), location(node));
                    }
                }
            }
            "invocation_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(name_node) = callee_identifier(function) {
                        // location(name_node), not location(node): a chained
                        // call (`a.F(x).F(y)`) has its outer and inner
                        // invocation_expression both start at `a`, which would
                        // make two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
                    }
                    self.visit(function, owner, type_name);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name);
                }
            }
            _ => self.visit_children(node, owner, type_name),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|n| n.kind() == kind)
}

/// The method/member name being invoked: a bare `identifier`, or the
/// `name` field of a `member_access_expression` (`receiver.Name(...)`).
fn callee_identifier(function: Node) -> Option<Node> {
    match function.kind() {
        "identifier" => Some(function),
        "member_access_expression" => function.child_by_field_name("name"),
        _ => None,
    }
}

/// Last identifier in a `using` path: the bare name, or a `qualified_name`'s
/// `name` field (`using System.Math;` → `Math`).
fn last_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" => Some(node),
        "qualified_name" => node.child_by_field_name("name").and_then(last_identifier),
        _ => None,
    }
}
