//! `LanguageParser` implementation for Java, via `tree-sitter-java`.
//!
//! Known limits: an anonymous class's methods (`new Runnable() { … }`) and
//! an enum constant's body attach to the enclosing type; a sealed type's
//! `permits` list is no relation (it names subtypes, not supertypes).

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use tree_sitter::{Node, Parser};

pub struct JavaParser;

impl LanguageParser for JavaParser {
    fn language_id(&self) -> &'static str {
        "java"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["java"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_java::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("tree-sitter-java grammar is statically valid");

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
        let mut walker = Walker::new(&file.contents);
        let module_id =
            walker.push_symbol(module_name, SymbolKind::Module, module_location(root), None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".java")
        .to_string()
}

/// The file-level module's location: the root node of a file ending in a
/// newline ends at column 0 of the row *after* the last line, which would
/// put the module one line past the end of the file.
fn module_location(root: Node) -> Location {
    let mut loc = location(root);
    let end = root.end_position();
    if end.column == 0 && end.row > root.start_position().row {
        loc.end_line = Some(end.row as u32);
    }
    loc
}

fn first_error(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let mut depth = 0u32;

    loop {
        let current = cursor.node();
        if current.is_error() || current.is_missing() {
            return Some(current);
        }

        if depth < MAX_TRAVERSAL_DEPTH && cursor.goto_first_child() {
            depth += 1;
            continue;
        }

        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 || !cursor.goto_parent() {
                return None;
            }
            depth -= 1;
        }
    }
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

    /// `owner` is the innermost enclosing method/module (calls/imports
    /// attach to it); `type_name` is the innermost enclosing
    /// class/interface/enum name, used as the `parent` of members declared
    /// directly inside it. Each `method_declaration` — including every
    /// overload — gets its own `SymbolRecord` row (same name, same parent,
    /// different location), so overloads are never collapsed into one
    /// symbol; `find_symbol` naturally returns all of them.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "class_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "annotation_type_declaration"
            | "enum_declaration" => {
                let kind = match node.kind() {
                    "class_declaration" | "record_declaration" => SymbolKind::Class,
                    "interface_declaration" | "annotation_type_declaration" => {
                        SymbolKind::Interface
                    }
                    _ => SymbolKind::Enum,
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    kind,
                    location(node),
                    type_name.map(str::to_string),
                );

                if let Some(superclass) = node.child_by_field_name("superclass") {
                    for type_id in find_type_identifiers(superclass) {
                        self.push_relation(
                            id,
                            RelationKind::Extends,
                            text(type_id, self.source).to_string(),
                            location(type_id),
                        );
                    }
                }
                if let Some(interfaces) = node.child_by_field_name("interfaces") {
                    for type_id in find_type_identifiers(interfaces) {
                        self.push_relation(
                            id,
                            RelationKind::Implements,
                            text(type_id, self.source).to_string(),
                            location(type_id),
                        );
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "enum_constant" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(
                    name,
                    SymbolKind::Field,
                    location(node),
                    type_name.map(str::to_string),
                );
                // `NEW { Set<State> next() { … } }`: the constant's body is
                // an anonymous subclass of the enum, so its methods belong
                // to the enum like the abstract declaration they override.
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, type_name, depth + 1);
                }
            }
            "method_declaration" | "constructor_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name,
                    SymbolKind::Method,
                    location(node),
                    type_name.map(str::to_string),
                );
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, type_name, depth + 1);
                }
            }
            "field_declaration" => {
                let mut cursor = node.walk();
                for declarator in node.children(&mut cursor) {
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
            "import_declaration" => {
                if let Some(path) = node.named_child(0) {
                    if let Some(last) = last_identifier(path) {
                        self.push_relation(
                            owner,
                            RelationKind::Imports,
                            text(last, self.source).to_string(),
                            location(node),
                        );
                    }
                }
            }
            "method_invocation" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    // location(name_node), not location(node): a chained call
                    // (`a.f(x).f(y)`) has its outer and inner method_invocation
                    // both start at `a`, which would make two same-named
                    // chained calls collide into one indistinguishable row.
                    self.push_relation(
                        owner,
                        RelationKind::Calls,
                        text(name_node, self.source).to_string(),
                        location(name_node),
                    );
                }
                if let Some(object) = node.child_by_field_name("object") {
                    self.visit(object, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "object_creation_expression" => {
                // `new Foo(…)` calls Foo's constructor: a call to the type's
                // name (`HashMap` for `new java.util.HashMap<K, V>()`).
                if let Some(type_id) = node
                    .child_by_field_name("type")
                    .and_then(|t| find_type_identifiers(t).into_iter().next())
                {
                    self.push_relation(
                        owner,
                        RelationKind::Calls,
                        text(type_id, self.source).to_string(),
                        location(type_id),
                    );
                }
                self.visit_children(node, owner, type_name, depth + 1);
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            ..Default::default()
        }
    }
}

/// `extends`/`implements` clauses wrap their `type_identifier`(s) in
/// grammar nodes (`superclass`, `super_interfaces` → `type_list`) with no
/// per-entry field name, so entries are found by kind rather than field.
/// One name per supertype: the last segment of `java.io.Serializable`, and
/// never the type arguments of `Comparable<Money>`.
fn find_type_identifiers(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    collect_type_identifiers(node, &mut out);
    out
}

fn collect_type_identifiers<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    match node.kind() {
        "type_identifier" => out.push(node),
        "type_arguments" => {}
        "scoped_type_identifier" => {
            let mut cursor = node.walk();
            if let Some(last) = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "type_identifier")
                .last()
            {
                out.push(last);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_type_identifiers(child, out);
            }
        }
    }
}

/// Last `identifier` in a possibly-dotted import path
/// (`scoped_identifier` chains), i.e. the imported class/member name.
fn last_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" => Some(node),
        "scoped_identifier" => node.child_by_field_name("name").and_then(last_identifier),
        "asterisk" => None,
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            children.into_iter().rev().find_map(last_identifier)
        }
    }
}
