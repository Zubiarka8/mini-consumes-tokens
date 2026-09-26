//! `LanguageParser` implementation for HTML, via `tree-sitter-html`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for HTML support.
//!
//! Only elements with an `id` attribute become `Element` symbols, named after
//! that id — an element with only a `class` has no single stable name to
//! index it under, so it's skipped (its containing id'd ancestor, if any,
//! still gets indexed). An id'd element also emits a `References` relation to
//! its own `#id` selector and one per token in its `class` attribute, so
//! `find_references("#header")`/`find_references(".nav")` (resolved by
//! `mct-lang-css`, in the same index) show which markup a CSS rule affects.
//! `<link rel="stylesheet" href="...">` and `<script src="...">` emit an
//! `Imports` relation to the target path. Embedded `<script>`/`<style>`
//! *content* is not parsed by this crate (it stays raw text in
//! `tree-sitter-html`'s own grammar) — that's `mct-lang-js-ts`'s and
//! `mct-lang-css`'s job respectively, for their own file extensions, not this
//! one reaching into inline blocks.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct HtmlParser;

impl LanguageParser for HtmlParser {
    fn language_id(&self) -> &'static str {
        "html"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["html", "htm"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_html::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .expect("tree-sitter-html grammar is statically valid");

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
        .trim_end_matches(".html")
        .trim_end_matches(".htm")
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

    fn push_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        location: Location,
        parent: Option<String>,
    ) -> SymbolId {
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

    /// `owner` is where file-level relations (an unclosed `<link>`/`<script
    /// src>` outside any id'd element) attach; `parent_name` is the nearest
    /// enclosing id'd element's name, for nesting id'd elements under it.
    fn visit_children(&mut self, node: Node, owner: SymbolId, parent_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, owner, parent_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, parent_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "element" => {
                let Some(tag) = find_tag(node) else {
                    self.visit_children(node, owner, parent_name, depth + 1);
                    return;
                };
                let (new_owner, new_parent) = self.handle_tag(tag, owner, parent_name);
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.id() == tag.id() {
                        continue;
                    }
                    self.visit(child, new_owner, new_parent.as_deref(), depth + 1);
                }
            }
            "script_element" | "style_element" => {
                if let Some(tag) = find_child(node, "start_tag") {
                    self.handle_tag(tag, owner, parent_name);
                }
                // Raw content (JS/CSS text) is not parsed here — see module doc.
            }
            _ => self.visit_children(node, owner, parent_name, depth + 1),
        }
    }

    /// Extracts `id`/`class`/`href`/`src` from a `start_tag`/`self_closing_tag`
    /// and emits the Element symbol + References/Imports relations described
    /// in the module doc. Returns the (owner, parent_name) that this tag's
    /// children (if any) should attach to.
    fn handle_tag(
        &mut self,
        tag: Node,
        owner: SymbolId,
        parent_name: Option<&str>,
    ) -> (SymbolId, Option<String>) {
        let tag_name = tag_name_of(tag, self.source);
        let attrs = collect_attributes(tag, self.source);
        // location of the specific attribute, not the whole tag: two
        // attributes on the same element (e.g. `id` and `class`) would
        // otherwise emit relations sharing one identical location.
        let attr = |key: &str| attrs.iter().find(|(n, _, _)| n == key).map(|(_, v, n)| (v.as_str(), *n));

        let (new_owner, new_parent) = match attr("id").filter(|(id, _)| !id.is_empty()) {
            Some((id, id_node)) => {
                let sym_id = self.push_symbol(
                    id.to_string(),
                    SymbolKind::Element,
                    location(tag),
                    parent_name.map(str::to_string),
                );
                self.push_relation(sym_id, RelationKind::References, format!("#{id}"), location(id_node));
                if let Some((classes, class_node)) = attr("class") {
                    for token in classes.split_whitespace() {
                        self.push_relation(
                            sym_id,
                            RelationKind::References,
                            format!(".{token}"),
                            location(class_node),
                        );
                    }
                }
                (sym_id, Some(id.to_string()))
            }
            None => (owner, parent_name.map(str::to_string)),
        };

        if tag_name.eq_ignore_ascii_case("link")
            && attr("rel").is_some_and(|(r, _)| r.eq_ignore_ascii_case("stylesheet"))
        {
            if let Some((href, href_node)) = attr("href").filter(|(h, _)| !h.is_empty()) {
                self.push_relation(new_owner, RelationKind::Imports, href.to_string(), location(href_node));
            }
        } else if tag_name.eq_ignore_ascii_case("script") {
            if let Some((src, src_node)) = attr("src").filter(|(s, _)| !s.is_empty()) {
                self.push_relation(new_owner, RelationKind::Imports, src.to_string(), location(src_node));
            }
        }

        (new_owner, new_parent)
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations, ..Default::default() }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|child| child.kind() == kind);
    found
}

fn find_tag(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "start_tag" || child.kind() == "self_closing_tag");
    found
}

fn tag_name_of(tag: Node, source: &str) -> String {
    find_child(tag, "tag_name").map(|n| text(n, source).to_string()).unwrap_or_default()
}

/// `(lowercased attribute name, value, the attribute's own node)` triples for
/// a `start_tag`/`self_closing_tag`. An unquoted value (`id=foo`) and a
/// quoted one (`id="foo"`) are both handled; an empty-quoted value (`id=""`)
/// yields "". The node is returned (not just the value) so a relation built
/// from it can be located at this specific attribute, not the whole tag.
fn collect_attributes<'a>(tag: Node<'a>, source: &str) -> Vec<(String, String, Node<'a>)> {
    let mut out = Vec::new();
    let mut cursor = tag.walk();
    for attribute in tag.named_children(&mut cursor) {
        if attribute.kind() != "attribute" {
            continue;
        }
        let mut name = String::new();
        let mut value = String::new();
        let mut acur = attribute.walk();
        for part in attribute.named_children(&mut acur) {
            match part.kind() {
                "attribute_name" => name = text(part, source).to_string(),
                "attribute_value" => value = text(part, source).to_string(),
                "quoted_attribute_value" => {
                    if let Some(inner) = find_child(part, "attribute_value") {
                        value = text(inner, source).to_string();
                    }
                }
                _ => {}
            }
        }
        if !name.is_empty() {
            out.push((name.to_lowercase(), value, attribute));
        }
    }
    out
}
