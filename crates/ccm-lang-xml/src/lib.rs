//! `LanguageParser` implementation for generic XML, via `tree-sitter-xml`.
//!
//! Nothing here is known to `ccm-core`, `ccm-index`, or `ccm-mcp-server` —
//! this crate is the entire integration surface for XML support.
//!
//! Unlike HTML, plain XML has no universal "meaningful name" or
//! cross-file-reference convention — different dialects (Maven POMs, MSBuild
//! project files, Android layouts, generic config/data) each invent their
//! own. So this crate is deliberately **structural only**: an element with an
//! `id`, `name`, or `Name` attribute (checked in that priority order) becomes
//! an `Element` symbol named after that attribute's value, nested under its
//! nearest such ancestor. No relations are ever emitted — there is nothing
//! dialect-generic to link to. For a dialect with real cross-file semantics,
//! see `ccm-lang-xaml`.

use ccm_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, SourceFile, SymbolId, SymbolKind,
    SymbolRecord,
};
use tree_sitter::{Node, Parser};

pub struct XmlParser;

impl LanguageParser for XmlParser {
    fn language_id(&self) -> &'static str {
        "xml"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["xml"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_xml::LANGUAGE_XML` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_xml::LANGUAGE_XML.into())
            .expect("tree-sitter-xml grammar is statically valid");

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
        walker.push_symbol(module_name, SymbolKind::Module, location(root), None);
        walker.visit_children(root, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path.rsplit('/').next().unwrap_or(relative_path).trim_end_matches(".xml").to_string()
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

/// An `AttValue` node's raw text includes its surrounding quotes (`"api"` or
/// `'api'`) — this strips exactly one matching pair, if present.
fn unquote(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, symbols: Vec::new(), next_id: 0 }
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
        self.symbols.push(SymbolRecord { id, name, kind, location, parent });
        id
    }

    fn visit_children(&mut self, node: Node, parent_name: Option<String>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, parent_name.clone(), depth + 1);
        }
    }

    fn visit(&mut self, node: Node, parent_name: Option<String>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "element" => {
                let tag = find_child(node, "STag").or_else(|| find_child(node, "EmptyElemTag"));
                let Some(tag) = tag else {
                    self.visit_children(node, parent_name, depth + 1);
                    return;
                };
                let attrs = collect_attributes(tag, self.source);
                let key = attr(&attrs, "id")
                    .or_else(|| attr(&attrs, "name"))
                    .or_else(|| attr(&attrs, "Name"))
                    .filter(|v| !v.is_empty());

                let new_parent = match key {
                    Some(name) => {
                        self.push_symbol(
                            name.to_string(),
                            SymbolKind::Element,
                            location(tag),
                            parent_name,
                        );
                        Some(name.to_string())
                    }
                    None => parent_name,
                };

                if let Some(content) = find_child(node, "content") {
                    self.visit_children(content, new_parent, depth + 1);
                }
            }
            _ => self.visit_children(node, parent_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: Vec::new() }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|child| child.kind() == kind);
    found
}

/// `(exact-case attribute name, unquoted value)` pairs for an `STag`/
/// `EmptyElemTag`. XML attribute names are case-sensitive by spec — `id`,
/// `name`, and `Name` are three distinct lookups, not one folded to lowercase.
fn collect_attributes(tag: Node, source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = tag.walk();
    for attribute in tag.named_children(&mut cursor) {
        if attribute.kind() != "Attribute" {
            continue;
        }
        let name = find_child(attribute, "Name").map(|n| text(n, source).to_string());
        let value =
            find_child(attribute, "AttValue").map(|n| unquote(text(n, source)).to_string());
        if let (Some(name), Some(value)) = (name, value) {
            out.push((name, value));
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == key).map(|(_, v)| v.as_str())
}
