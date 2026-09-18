//! `LanguageParser` implementation for CSS, via `tree-sitter-css`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for CSS support.
//!
//! Only **simple selectors** (a bare `.class` or `#id`, nothing else) become
//! `Rule` symbols, named exactly as written (`.foo`, `#foo`) so they match an
//! HTML `Element`'s outgoing `References` by plain name equality — the same
//! name-based matching every other relation in this project uses. Compound
//! selectors (`.a.b`, `div.foo`) and combinators (`.a .b`, `#id > .c`) are
//! deliberately not indexed: resolving which elements they actually match
//! needs a DOM, not an AST, and is out of scope (same kind of deliberate
//! limitation as Go's deferred `find_implementations`). CSS properties/values
//! inside a rule's block are not modeled at all — only the selector.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct CssParser;

impl LanguageParser for CssParser {
    fn language_id(&self) -> &'static str {
        "css"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["css"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_css::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_css::LANGUAGE.into())
            .expect("tree-sitter-css grammar is statically valid");

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
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, location(root));
        walker.visit_children(root, module_id, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".css")
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
        Self { source, symbols: Vec::new(), relations: Vec::new(), next_id: 0 }
    }

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord { id, name, kind, location, parent: None });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    fn visit_children(&mut self, node: Node, owner: SymbolId, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "rule_set" => {
                if let Some(selectors) = find_child(node, "selectors") {
                    let mut cursor = selectors.walk();
                    for selector in selectors.named_children(&mut cursor) {
                        if let Some(name) = simple_selector_name(selector, self.source) {
                            self.push_symbol(name, SymbolKind::Rule, location(selector));
                        }
                    }
                }
                // Recurse into the block too: a rule's declarations never
                // nest another rule_set in plain CSS, but this keeps the
                // walk uniform and costs nothing.
                self.visit_children(node, owner, depth + 1);
            }
            "import_statement" => {
                if let Some(target) = import_target(node, self.source) {
                    self.push_relation(owner, RelationKind::Imports, target, location(node));
                }
            }
            _ => self.visit_children(node, owner, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|child| child.kind() == kind);
    found
}

/// A selector is "simple" when it's a bare `.class` or `#id` with no other
/// combinator/compound structure — i.e. its only named child is the
/// `class_name`/`id_name` leaf. Anything else (`div.foo`, `.a.b`, `.a .b`,
/// `#id > .c`, ...) returns `None` and is not indexed; see the module doc.
fn simple_selector_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "class_selector" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            if children.len() == 1 && children[0].kind() == "class_name" {
                Some(format!(".{}", text(children[0], source)))
            } else {
                None
            }
        }
        "id_selector" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            if children.len() == 1 && children[0].kind() == "id_name" {
                Some(format!("#{}", text(children[0], source)))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The imported path from an `@import` statement: a plain string
/// (`@import "reset.css";`) or a `url(...)` call (`@import url("theme.css");`,
/// unquoted `url(theme.css)` included).
fn import_target(node: Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_value" => {
                if let Some(content) = find_child(child, "string_content") {
                    return Some(text(content, source).to_string());
                }
            }
            "call_expression" => {
                if let Some(args) = find_child(child, "arguments") {
                    let mut acur = args.walk();
                    for arg in args.named_children(&mut acur) {
                        if let Some(target) = import_target_value(arg, source) {
                            return Some(target);
                        }
                    }
                }
            }
            "plain_value" => return Some(text(child, source).to_string()),
            _ => {}
        }
    }
    None
}

fn import_target_value(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "string_value" => find_child(node, "string_content").map(|c| text(c, source).to_string()),
        "plain_value" => Some(text(node, source).to_string()),
        _ => None,
    }
}
