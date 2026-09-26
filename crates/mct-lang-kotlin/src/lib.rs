//! `LanguageParser` implementation for Kotlin, via `tree-sitter-kotlin-ng`.
//!
//! Node kinds below were confirmed against the real compiled grammar (a
//! throwaway `to_sexp`/full-child dump, same method documented in
//! `checklist.md` for PHP/Bash/PowerShell), not assumed from grammar docs.
//! `class_declaration` covers both `class` and `interface` — the grammar
//! distinguishes them only by an anonymous `interface` token, never a named
//! field, so `is_interface` below checks the raw child list. `object`
//! declarations get their own `object_declaration` node; this parser reuses
//! `SymbolKind::Class` for them (Kotlin's singleton class), the same kind of
//! documented reuse as PHP's `SymbolKind::Trait`. None of
//! `call_expression`/`navigation_expression`/`property_declaration`/
//! `class_parameter`'s children are field-tagged in this grammar, so they're
//! read positionally/by-kind rather than via `child_by_field_name`.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct KotlinParser;

impl LanguageParser for KotlinParser {
    fn language_id(&self) -> &'static str {
        "kotlin"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_kotlin_ng::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .expect("tree-sitter-kotlin-ng grammar is statically valid");

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

        // `package foo.bar` — one Module-kind symbol per occurrence, not
        // deduplicated across files, same convention as Go's package clause
        // and C#'s namespace_declaration.
        if let Some(package_header) = find_child(root, "package_header") {
            if let Some(qid) = package_header.named_child(0) {
                walker.push_symbol(text(qid, &file.contents).to_string(), SymbolKind::Module, location(package_header), None);
            }
        }

        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".kts")
        .trim_end_matches(".kt")
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

/// `class_declaration` covers `class`/`data class`/`interface` alike; the
/// grammar's only signal is the anonymous `interface` token (no field, no
/// dedicated node kind), so it's found by scanning all children (not just
/// named ones — the token itself is unnamed).
fn is_interface(class_declaration: Node) -> bool {
    let mut cursor = class_declaration.walk();
    let children: Vec<Node> = class_declaration.children(&mut cursor).collect();
    children.into_iter().any(|c| c.kind() == "interface")
}

/// The last `identifier` child of a `navigation_expression`
/// (`receiver.member` → `member`), i.e. the callee name for a
/// `receiver.method(...)` call — same "outermost field wins" precedent as
/// Java's `method_invocation`/Go's `selector_expression`.
fn last_identifier_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let idents: Vec<Node> = node.named_children(&mut cursor).filter(|c| c.kind() == "identifier").collect();
    idents.last().copied()
}

/// The name being invoked by a `call_expression`: its first child is the
/// callee expression (no field name in this grammar), either a bare
/// `identifier` (`foo()`) or a `navigation_expression` (`a.b.foo()`, where
/// the last identifier segment is the call name).
fn callee_identifier(call_expression: Node) -> Option<Node> {
    let callee = call_expression.child(0)?;
    match callee.kind() {
        "identifier" => Some(callee),
        "navigation_expression" => last_identifier_child(callee),
        _ => None,
    }
}

/// A `function_declaration`'s extension receiver type (`fun String.shout()`
/// → `"String"`), if any: the grammar puts a bare `user_type` positionally
/// before the `name` field only for extension functions — generics
/// (`type_parameters`) and modifiers use different node kinds, so this can't
/// collide with them.
fn extension_receiver(function_declaration: Node, source: &str) -> Option<String> {
    let name = function_declaration.child_by_field_name("name")?;
    let mut cursor = function_declaration.walk();
    for child in function_declaration.children(&mut cursor) {
        if child.start_byte() >= name.start_byte() {
            break;
        }
        if child.kind() == "user_type" {
            return Some(text(child, source).to_string());
        }
    }
    None
}

/// The parameter name inside a primary-constructor `class_parameter`
/// (`val name: String`, `private val items: ...`): the first plain
/// `identifier` child — modifiers/`val`/`var` are distinct node kinds, so
/// this can't pick up the wrong token.
fn class_parameter_name(class_parameter: Node) -> Option<Node> {
    let mut cursor = class_parameter.walk();
    let children: Vec<Node> = class_parameter.children(&mut cursor).collect();
    children.into_iter().find(|c| c.kind() == "identifier")
}

/// Whether a `class_parameter` is promoted to a property (`val`/`var`
/// before the name) versus a plain constructor-only parameter.
fn is_promoted_property(class_parameter: Node) -> bool {
    let mut cursor = class_parameter.walk();
    let children: Vec<Node> = class_parameter.children(&mut cursor).collect();
    children.into_iter().any(|c| c.kind() == "val" || c.kind() == "var")
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
        self.symbols.push(SymbolRecord { id, name, kind, location, parent, level: None });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    /// `owner` is the innermost enclosing function/method (calls attach to
    /// it) or the file-level module for top-level statements; `type_name` is
    /// the innermost enclosing class/interface/object, used as `parent` for
    /// members declared directly inside it.
    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "class_declaration" | "object_declaration" => {
                let kind = if node.kind() == "class_declaration" && is_interface(node) {
                    SymbolKind::Interface
                } else {
                    // `object_declaration` reuses Class — see module doc.
                    SymbolKind::Class
                };
                let name = node.child_by_field_name("name").map(|n| text(n, self.source).to_string()).unwrap_or_default();
                let id = self.push_symbol(name.clone(), kind, location(node), type_name.map(str::to_string));

                if let Some(delegations) = find_child(node, "delegation_specifiers") {
                    let mut cursor = delegations.walk();
                    for spec in delegations.named_children(&mut cursor) {
                        if spec.kind() != "delegation_specifier" {
                            continue;
                        }
                        if let Some(target) = spec.named_child(0) {
                            match target.kind() {
                                // `Shape(4)` — a constructor call, so `Shape` is the superclass.
                                "constructor_invocation" => {
                                    if let Some(user_type) = target.named_child(0) {
                                        if let Some(type_id) = user_type.named_child(0) {
                                            self.push_relation(id, RelationKind::Extends, text(type_id, self.source).to_string(), location(type_id));
                                        }
                                    }
                                }
                                // A bare type name, no call — an interface.
                                "user_type" => {
                                    if let Some(type_id) = target.named_child(0) {
                                        self.push_relation(id, RelationKind::Implements, text(type_id, self.source).to_string(), location(type_id));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if let Some(primary_ctor) = find_child(node, "primary_constructor") {
                    self.visit_children(primary_ctor, owner, Some(&name), depth + 1);
                }
                if let Some(body) = find_child(node, "class_body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "class_parameter" => {
                if is_promoted_property(node) {
                    if let Some(name_node) = class_parameter_name(node) {
                        self.push_symbol(text(name_node, self.source).to_string(), SymbolKind::Field, location(node), type_name.map(str::to_string));
                    }
                }
                // Default-value expressions (`= mutableListOf()`) may still
                // contain calls worth indexing, promoted property or not.
                self.visit_children(node, owner, type_name, depth + 1);
            }
            "function_declaration" => {
                let name = node.child_by_field_name("name").map(|n| text(n, self.source).to_string()).unwrap_or_default();
                let receiver = extension_receiver(node, self.source);
                let parent = receiver.or_else(|| type_name.map(str::to_string));
                let id = self.push_symbol(name, SymbolKind::Method, location(node), parent);
                if let Some(params) = find_child(node, "function_value_parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(body) = find_child(node, "function_body") {
                    self.visit_children(body, id, type_name, depth + 1);
                }
            }
            "property_declaration" => {
                if type_name.is_some() {
                    if let Some(var_decl) = find_child(node, "variable_declaration") {
                        if let Some(name_node) = var_decl.named_child(0) {
                            self.push_symbol(text(name_node, self.source).to_string(), SymbolKind::Field, location(node), type_name.map(str::to_string));
                        }
                    }
                }
                // Always recurse: initializer expressions (`val x = Foo()`)
                // may contain calls even for locals we don't index as Field.
                self.visit_children(node, owner, type_name, depth + 1);
            }
            "import" => {
                let mut cursor = node.walk();
                let named: Vec<Node> = node.named_children(&mut cursor).collect();
                let imported = if named.len() >= 2 {
                    // `import a.b.Foo as Bar` — the alias is what call sites use.
                    Some(named[1])
                } else {
                    named.first().and_then(|qid| {
                        let mut c = qid.walk();
                        let idents: Vec<Node> = qid.named_children(&mut c).filter(|n| n.kind() == "identifier").collect();
                        idents.last().copied()
                    })
                };
                if let Some(name_node) = imported {
                    self.push_relation(owner, RelationKind::Imports, text(name_node, self.source).to_string(), location(node));
                }
            }
            "call_expression" => {
                if let Some(name_node) = callee_identifier(node) {
                    // location(name_node), not location(node): a chained call
                    // (`a.f(x).f(y)`) would otherwise collide two distinct
                    // calls into one indistinguishable row.
                    self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
                }
                self.visit_children(node, owner, type_name, depth + 1);
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations, ..Default::default() }
    }
}
