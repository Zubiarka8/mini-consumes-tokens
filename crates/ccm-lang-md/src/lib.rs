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
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
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
        walker.visit_children(root, None, None, 0);
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

/// Extracts `[[WikiLink]]` targets from raw text. A `|alias` display
/// suffix is stripped (the alias is presentation, not the reference
/// identity); a `#Heading` anchor is kept verbatim as part of the target
/// (anchor-aware resolution is deferred, see the module doc comment). Not
/// grammar-aware — this scans raw node text directly, since
/// `tree-sitter-md`'s inline grammar has no concept of this
/// Obsidian-specific syntax.
fn scan_wikilinks(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut i = 0;
    while let Some(start) = text[i..].find("[[") {
        let open = i + start + 2;
        let Some(rel_end) = text[open..].find("]]") else {
            break;
        };
        let close = open + rel_end;
        let target = text[open..close].split('|').next().unwrap_or("").trim();
        if !target.is_empty() {
            targets.push(target.to_string());
        }
        i = close + 2;
    }
    targets
}

/// Extracts `#tag` occurrences from raw text, each returned as its bare
/// name (the `tag:` prefix is applied by the caller). A `#` only starts a
/// tag when preceded by start-of-text or whitespace — this is what keeps a
/// URL fragment like `.../page#section` from being mistaken for a tag (the
/// character before its `#` is `/`, never whitespace). A tag-shaped token
/// inside inline code (`` `#notatag` ``) is still matched — excluding code
/// spans needs an inline-grammar reparse, deferred (see the module doc
/// comment).
fn scan_tags(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_boundary = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '#' && at_boundary {
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric() || matches!(chars[end], '_' | '/' | '-'))
            {
                end += 1;
            }
            if end > start {
                tags.push(chars[start..end].iter().collect());
                i = end;
                continue;
            }
        }
        i += 1;
    }
    tags
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

    /// Scans `text` for `[[WikiLink]]` targets and records each as a
    /// `References` relation from `from`. `loc` is the whole containing
    /// heading/paragraph block's location — relations are block-granular,
    /// not exact-span (see the module doc comment).
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        for to_name in scan_wikilinks(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name,
                location: loc,
            });
        }
        for tag in scan_tags(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name: format!("tag:{tag}"),
                location: loc,
            });
        }
    }

    fn visit_children(
        &mut self,
        node: Node,
        parent_name: Option<String>,
        parent_id: Option<SymbolId>,
        depth: u32,
    ) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, parent_name.clone(), parent_id, depth + 1);
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
    fn visit(
        &mut self,
        node: Node,
        parent_name: Option<String>,
        parent_id: Option<SymbolId>,
        depth: u32,
    ) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    let heading_loc = location(heading);
                    let id =
                        self.push_symbol(name.clone(), SymbolKind::Element, heading_loc, parent_name);
                    self.push_relations_from_text(id, &name, heading_loc);
                    self.visit_children(node, Some(name), Some(id), depth + 1);
                }
                None => self.visit_children(node, parent_name, parent_id, depth + 1),
            },
            "paragraph" => {
                if let Some(id) = parent_id {
                    let text = node.utf8_text(self.source.as_bytes()).unwrap_or_default();
                    self.push_relations_from_text(id, text, location(node));
                }
                self.visit_children(node, parent_name, parent_id, depth + 1);
            }
            _ => self.visit_children(node, parent_name, parent_id, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
}
