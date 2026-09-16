//! `LanguageParser` implementation for Markdown headings, via `tree-sitter-md`.
//!
//! Phase 1 scope only: ATX headings (`#`..`######`) become `Element` symbols,
//! nested by the block grammar's own `section` structure — no manual
//! level-counting stack is needed, `tree-sitter-md` already wraps each
//! heading and its lower-level content in a `section` node nested by level
//! (verified against a real parse before writing this walker: `# A / ## B /
//! ## C / ### D / # E / ## F` nests D under C's section, not B's, and E
//! starts a sibling section of A's, not a child of it). No relations are
//! emitted yet — internal links, `RelationKind::Imports`, anchors, setext
//! headings, lists, tables, and code blocks are all deliberately out of
//! scope for this phase.
//!
//! Unlike `ccm-lang-xml`, this crate does not emit a synthetic root `Module`
//! symbol for the file: a heading-less document must produce zero symbols.

use ccm_core::{
    LanguageParser, Location, ParseError, ParsedFile, SourceFile, SymbolId, SymbolKind,
    SymbolRecord,
};
use tree_sitter::{Node, Parser};

pub struct MarkdownParser;

impl LanguageParser for MarkdownParser {
    fn language_id(&self) -> &'static str {
        "markdown"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["md"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_md::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_md::LANGUAGE.into())
            .expect("tree-sitter-md block grammar is statically valid");

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

        let mut walker = Walker::new(&file.contents);
        walker.visit_children(root, None);
        Ok(walker.finish())
    }
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

/// A heading's visible text is its `heading_content` field (the `inline`
/// node `tree-sitter-md` wraps the text in), taken as raw source text. This
/// deliberately does not strip inline Markdown formatting (`**bold**`,
/// `` `code` ``, etc.) — doing that correctly needs a second parse with the
/// inline grammar, which is out of scope for this phase (reserved for a
/// later link-extraction phase). A heading like `# Hello **World**` is
/// indexed verbatim as `"Hello **World**"`.
fn heading_text<'a>(heading: Node, source: &'a str) -> &'a str {
    heading
        .child_by_field_name("heading_content")
        .map(|n| n.utf8_text(source.as_bytes()).unwrap_or_default().trim())
        .unwrap_or_default()
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
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

    fn visit_children(&mut self, node: Node, parent_name: Option<String>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, parent_name.clone());
        }
    }

    /// A `section` node is `tree-sitter-md`'s own hierarchy container: it
    /// wraps one heading (if any) plus every node at a deeper level nested
    /// directly inside it, including child `section`s for lower-level
    /// headings. That nesting already IS the parent/child structure we
    /// want — no manual level-comparison stack is needed.
    ///
    /// Not every `section` contains a heading — content appearing before the
    /// first heading in a file (or a heading-less file entirely) is still
    /// wrapped in a `section`, just one with no `atx_heading` child. That
    /// case recurses with the parent unchanged and emits no symbol.
    fn visit(&mut self, node: Node, parent_name: Option<String>) {
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        location(heading),
                        parent_name,
                    );
                    self.visit_children(node, Some(name));
                }
                None => self.visit_children(node, parent_name),
            },
            _ => self.visit_children(node, parent_name),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: Vec::new(),
        }
    }
}
