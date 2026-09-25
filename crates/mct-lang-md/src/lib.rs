//! `LanguageParser` implementation for Markdown, via `tree-sitter-md`.
//!
//! Phase 1: ATX headings (`#`..`######`) become `Element` symbols, nested by
//! the block grammar's own `section` structure — no manual level-counting
//! stack is needed, `tree-sitter-md` already wraps each heading and its
//! lower-level content in a `section` node nested by level (verified against
//! a real parse before writing this walker: `# A / ## B / ## C / ### D / # E
//! / ## F` nests D under C's section, not B's, and E starts a sibling
//! section of A's, not a child of it).
//!
//! Phase 2: `[[WikiLink]]` and `#tag` occurrences in a heading's own text or
//! in a `paragraph` block are indexed as `RelationKind::References`
//! relations from the enclosing heading symbol (a `#tag` target is
//! `tag:<name>`, since `mct-core::SymbolRecord` has no attribute field — see
//! `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`).
//! A `|alias` suffix on a WikiLink is stripped; a `#Heading` anchor is kept
//! verbatim as part of the target. A link/tag with no enclosing heading (text
//! before the first heading, or in a heading-less file) is silently skipped.
//! Scanning is hand-rolled text scanning, not a second parse with the inline
//! grammar — `tree-sitter-md`'s inline grammar is CommonMark and has no
//! concept of this Obsidian-specific syntax. Links/tags inside list items and
//! blockquotes are not specially handled — `tree-sitter-md` wraps their text
//! in the same `paragraph` node kind as top-level text, so they are scanned
//! like any other paragraph; excluding them deliberately would require
//! detecting the containing block type, deferred to a future phase. Table
//! cells are a distinct block-grammar node with no `paragraph` child and are
//! never scanned. Code spans are not a separate node at all in this crate's
//! block-only grammar (the inline grammar that defines `code_span` is never
//! parsed) — inline-code text is embedded directly in the surrounding
//! paragraph, so a tag or link inside backticks is scanned like ordinary text
//! (see `scan_tags`'s doc comment). Anchor-aware target resolution and
//! exact-span relation locations (a relation's `Location` is its whole
//! containing block, not the bracket span) are out of scope. Standard Markdown
//! links (`[text](url)`) are not `[[WikiLinks]]` and are never indexed. Setext
//! headings, lists, tables, and code blocks remain out of scope for symbol
//! extraction, same as Phase 1.
//!
//! Phase 3: `[[Note#Heading]]` is no longer indexed as one verbatim target
//! (Phase 2's behavior). The alias-stripped identity is split on the first
//! `#` into a note-name part and a heading part; each non-empty part becomes
//! its own `RelationKind::References` relation from the same enclosing
//! symbol and location. `SymbolRelation` has exactly one `to_name` per
//! relation and this codebase has no composite or path-qualified relation
//! concept anywhere — every existing relation, in every language, resolves a
//! single unscoped name — so two independent relations is the closest fit
//! without inventing a new `RelationKind` or a new `SymbolRelation` field,
//! both out of scope. `[[#Heading]]` (no note part, e.g. a same-document
//! link) naturally emits only the heading relation, a side effect of "only
//! emit non-empty parts" rather than a deliberately built feature. A
//! trailing `.md`/`.MD` suffix on the note part (never the heading part) is
//! stripped case-insensitively before it becomes a relation, so `[[Note]]`
//! and `[[Note.md]]` resolve to the identical target `"Note"`. `![[Embed]]`
//! and `![[Embed#Heading]]` need no separate handling: the leading `!` sits
//! outside the `[[...]]` span `scan_wikilinks` matches, so an embed is
//! already scanned exactly like a plain wikilink through the same code path
//! — true since Phase 2 but untested and undocumented until now. Also fixed
//! in this phase: a malformed nested wikilink (e.g. `"a [[ b [[Real]] c"`) no
//! longer swallows a well-formed inner link into garbage text — see
//! `scan_wikilinks`'s doc comment. Whole-note resolution of the note-name
//! part still relies on the pre-existing convention that a target note's own
//! heading (typically its H1) is named like the note title — this crate has
//! no notion of a "file"/"note" separate from headings, and that limitation
//! is unchanged and explicitly accepted, not addressed by this phase.
//!
//! Unlike `mct-lang-xml`, this crate does not emit a synthetic root `Module`
//! symbol for the file: a heading-less document must produce zero symbols.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
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

/// An `atx_heading` node's declared level (1..6), read off its marker child
/// (`atx_h1_marker`..`atx_h6_marker` — `tree-sitter-md` emits exactly one of
/// these per heading, never the `#` count itself). `None` only if the grammar
/// ever changes shape underneath this; every heading that reaches this
/// function came from a successful parse, so this should always be `Some`.
fn heading_level(heading: Node) -> Option<u32> {
    let mut cursor = heading.walk();
    let marker = heading
        .named_children(&mut cursor)
        .find(|child| child.kind().starts_with("atx_h") && child.kind().ends_with("_marker"))?;
    marker
        .kind()
        .strip_prefix("atx_h")
        .and_then(|rest| rest.strip_suffix("_marker"))
        .and_then(|digit| digit.parse().ok())
}

/// One `[[...]]`/`![[...]]` match's alias-stripped identity, split into its
/// note-name and heading/anchor parts (see `split_note_and_heading`).
/// Returned as two independently-optional parts rather than one combined
/// string, since each becomes its own `RelationKind::References` relation —
/// `SymbolRelation` has no field for a composite "note + heading" target
/// (see the module doc comment's Phase 3 paragraph).
struct WikiLinkTarget {
    note: Option<String>,
    heading: Option<String>,
}

/// Extracts `[[WikiLink]]` and `![[Embed]]` targets from raw text, each
/// split into a note part and a heading/anchor part. An embed is scanned
/// identically to a plain wikilink — the leading `!` sits outside the
/// `[[...]]` span this function matches, so no special-casing is needed. A
/// `|alias` display suffix is stripped (the alias is presentation, not
/// reference identity) before splitting on `#`, so
/// `[[Page#Heading|shown text]]` still splits correctly. Not grammar-aware —
/// this scans raw node text directly, since `tree-sitter-md`'s inline
/// grammar has no concept of this Obsidian-specific syntax.
///
/// A candidate span that itself contains a nested `[[` is rejected as
/// malformed and skipped entirely, resuming the scan from just past the
/// *outer* `[[` (`i = open`, not `i = close + 2`) so a well-formed inner
/// wikilink is still found on a later pass instead of being swallowed into
/// the outer match's garbage text — e.g. `a [[ b [[Real]] c` must still
/// yield `Real`, not a garbage target like `b [[Real`.
fn scan_wikilinks(text: &str) -> Vec<WikiLinkTarget> {
    let mut targets = Vec::new();
    let mut i = 0;
    while let Some(start) = text[i..].find("[[") {
        let open = i + start + 2;
        let Some(rel_end) = text[open..].find("]]") else {
            break;
        };
        let close = open + rel_end;
        let raw = &text[open..close];
        if raw.contains("[[") {
            i = open;
            continue;
        }
        let identity = raw.split('|').next().unwrap_or("").trim();
        if !identity.is_empty() {
            targets.push(split_note_and_heading(identity));
        }
        i = close + 2;
    }
    targets
}

/// Splits a wikilink's alias-stripped identity text on the first `#` into a
/// note-name part and a heading/anchor part — `[[Note#Heading]]` must stop
/// being indexed as one verbatim string (Phase 2's behavior) and instead
/// point at both the note and the heading independently, since nothing else
/// in this codebase has any other way to resolve the heading half against a
/// real symbol. See the module doc comment's Phase 3 paragraph for why this
/// becomes two relations rather than one composite one.
fn split_note_and_heading(identity: &str) -> WikiLinkTarget {
    match identity.split_once('#') {
        Some((note, heading)) => WikiLinkTarget {
            note: non_empty(strip_md_extension(note.trim())),
            heading: non_empty(heading.trim().to_string()),
        },
        None => WikiLinkTarget {
            note: non_empty(strip_md_extension(identity.trim())),
            heading: None,
        },
    }
}

/// Strips a trailing `.md`/`.MD`/... suffix (case-insensitive) from a
/// wikilink's note-name part, so `[[Note]]` and `[[Note.md]]` resolve to the
/// identical target `"Note"` — Obsidian accepts both forms, and every
/// heading symbol in this index is named without a file extension. Uses
/// `str::get` to check the candidate suffix rather than slicing directly by
/// byte offset: a direct `note[note.len() - 3..]` slice would panic on
/// non-ASCII input where that byte offset isn't a char boundary; `get`
/// returns `None` instead, treated the same as "no `.md` suffix present".
/// Never applied to the heading part — `split_note_and_heading` calls this
/// only on `note`.
fn strip_md_extension(note: &str) -> String {
    let has_md_suffix = note
        .len()
        .checked_sub(3)
        .and_then(|i| note.get(i..))
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".md"));
    if has_md_suffix {
        note[..note.len() - 3].trim_end().to_string()
    } else {
        note.to_string()
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Extracts `#tag` occurrences from raw text, each returned as its bare
/// name (the `tag:` prefix is applied by the caller). A `#` only starts a
/// tag when preceded by start-of-text or whitespace — this is what keeps a
/// URL fragment like `.../page#section` from being mistaken for a tag (the
/// character before its `#` is `/`, never whitespace). A tag-shaped token
/// inside inline code (`` `#notatag` ``) is still matched — excluding code
/// spans needs an inline-grammar reparse, deferred (see the module doc
/// comment).
///
/// Deviates from the spec's literal `[A-Za-z0-9_/-]+` character class in two
/// ways: a candidate that is entirely ASCII digits (`#123`) is rejected —
/// real-world Markdown uses bare-numeric `#123`-style tokens for issue/PR
/// references, not PKM tags, so indexing them would be noise (a token with
/// at least one non-digit character, like `#v2` or `#2fa`, still matches).
/// The character class itself uses `char::is_alphanumeric`, which is
/// Unicode-broad (it already matches non-ASCII letters/digits like `é` or
/// `日本語`) rather than the spec's nominal ASCII-only class — kept
/// deliberately, since it matches real Obsidian tagging behavior more
/// closely than a strict ASCII class would.
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
                let candidate = &chars[start..end];
                if !candidate.iter().all(|c| c.is_ascii_digit()) {
                    tags.push(candidate.iter().collect());
                }
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
        level: Option<u32>,
    ) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord {
            id,
            name,
            kind,
            location,
            parent,
            level,
        });
        id
    }

    /// Scans `text` for `[[WikiLink]]`/`![[Embed]]` and `#tag` occurrences and
    /// records each as a `References` relation from `from`. A wikilink with
    /// an anchor emits up to two relations (see `split_note_and_heading`);
    /// `loc` is the whole containing heading/paragraph block's location for
    /// both — relations are block-granular, not exact-span (see the module
    /// doc comment).
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        for target in scan_wikilinks(text) {
            if let Some(note) = target.note {
                self.relations.push(SymbolRelation {
                    from,
                    kind: RelationKind::References,
                    to_name: note,
                    location: loc,
                });
            }
            if let Some(heading) = target.heading {
                self.relations.push(SymbolRelation {
                    from,
                    kind: RelationKind::References,
                    to_name: heading,
                    location: loc,
                });
            }
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
                    let id = self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        heading_loc,
                        parent_name,
                        heading_level(heading),
                    );
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
