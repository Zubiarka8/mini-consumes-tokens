//! `LanguageParser` implementation for Markdown, via `tree-sitter-md`.
//!
//! Every parsed file is a note: one `SymbolKind::Module` symbol named after
//! the file stem (identity is the file path, carried by the index's `files`
//! row — a title, alias or basename never replaces it) spanning the whole
//! file, even when the file is empty or has no heading. ATX and Setext
//! headings become `Element` symbols nested by heading level (the grammar's
//! own `section` nesting covers ATX headings only), with `level` as written.
//! A top-level heading has no `parent`: its file path already scopes it to its
//! note, and tools treat parentless symbols as a file's top level. A heading's
//! range runs to the line before the next heading of equal or shallower level,
//! so its descendants are inside it.
//!
//! Relations come from a heading's text and from `paragraph` blocks, never
//! from fenced/indented code blocks or inline code spans (masked before
//! scanning). Text before the first heading, and every link in a heading-less
//! note, belongs to the note symbol. `[[Note]]` and `![[Note]]` are
//! `References` and `Imports` relations respectively (an embed transcludes
//! the target, a link only mentions it). A `|alias` suffix is dropped.
//!
//! Targets carry `RelationTarget` evidence so the index never picks by name:
//! a note part is a `Module` named by its stem, constrained to an exact path
//! when written as one (`folder/Note`, `./Note`, `../Note`, a `.md` suffix is
//! dropped; a path escaping the repository root is kept as its raw spelling
//! and stays unresolved). `[[Note#Heading]]` adds an `Element` relation
//! scoped to that note's file; `[[#Heading]]` scopes to the source note. A
//! heading is matched by its exact text (no slug or case folding), and only
//! the last segment of `A#B#C` is used; `#^block` anchors are not indexed.
//!
//! A leading YAML frontmatter block yields `tag:<t>`, `alias:<a>` and
//! `title:<t>` references from the note — only `title`, `aliases` and `tags`
//! in scalar, inline-list and `- item` forms; anything else is ignored and
//! never evaluated. Wikilink resolution by alias or title is not done.
//! Standard `[text](url)` links, tables and `#tag`-like text in code are not
//! indexed. Scanning is hand-rolled text scanning, since `tree-sitter-md`'s
//! inline grammar has no concept of this Obsidian-specific syntax.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
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

        let mut walker = Walker::new(&file.relative_path, &file.contents);
        let note = walker.push_note();
        walker.push_frontmatter_relations(note);
        walker.walk_document(root, note);
        Ok(walker.finish())
    }
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

/// An ATX heading's visible text is its `heading_content` field (the `inline`
/// node `tree-sitter-md` wraps the text in), a Setext heading's is its
/// `paragraph` content (one or more lines, joined by single spaces), both as
/// raw source text. Inline Markdown formatting (`**bold**`) is not stripped.
fn heading_text(heading: Node, source: &str) -> String {
    let text = heading
        .child_by_field_name("heading_content")
        .map(|n| n.utf8_text(source.as_bytes()).unwrap_or_default())
        .unwrap_or_default();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A heading node's declared level (1..6): the `atx_hN_marker` child of an ATX
/// heading, or the `setext_h1/h2_underline` child of a Setext one.
fn heading_level(heading: Node) -> Option<u32> {
    let mut cursor = heading.walk();
    let marker = heading.named_children(&mut cursor).find(|child| {
        let kind = child.kind();
        (kind.starts_with("atx_h") && kind.ends_with("_marker"))
            || (kind.starts_with("setext_h") && kind.ends_with("_underline"))
    })?;
    marker
        .kind()
        .trim_start_matches("atx_h")
        .trim_start_matches("setext_h")
        .trim_end_matches("_marker")
        .trim_end_matches("_underline")
        .parse()
        .ok()
}

/// One `[[...]]`/`![[...]]` match: whether it was an embed, its
/// alias-stripped note part (`.md` dropped, empty when absent) and its
/// heading/anchor part.
struct WikiLink {
    embed: bool,
    note: String,
    heading: Option<String>,
}

/// Extracts `[[WikiLink]]` and `![[Embed]]` links from raw text. A `|alias`
/// display suffix is stripped before splitting on the first `#`. A candidate
/// span that itself contains a nested `[[` is rejected as malformed and
/// skipped, resuming just past the *outer* `[[` so a well-formed inner link
/// is still found (`a [[ b [[Real]] c` yields `Real`).
fn scan_wikilinks(text: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut i = 0;
    while let Some(start) = text[i..].find("[[") {
        let open_at = i + start;
        let open = open_at + 2;
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
            let (note, heading) = match identity.split_once('#') {
                Some((note, heading)) => (note, Some(heading)),
                None => (identity, None),
            };
            links.push(WikiLink {
                embed: text[..open_at].ends_with('!'),
                note: strip_md_extension(note.trim()),
                heading: heading.and_then(anchor_heading),
            });
        }
        i = close + 2;
    }
    links
}

/// The heading an anchor names: the last `#`-separated segment (`A#B#C` is
/// heading `C` of the note), trimmed. A `^block` id is not a heading.
fn anchor_heading(anchor: &str) -> Option<String> {
    let last = anchor.rsplit('#').next().unwrap_or("").trim();
    (!last.is_empty() && !last.starts_with('^')).then(|| last.to_string())
}

/// Strips a trailing `.md`/`.MD`/... suffix (case-insensitive) from a
/// wikilink's note part, so `[[Note]]` and `[[Note.md]]` are the same target.
/// Uses `str::get` rather than slicing by byte offset, which would panic on a
/// non-char boundary.
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

/// Lexically normalizes a `/`-separated path: drops empty and `.` segments
/// and resolves `..`. `None` when it climbs above the root.
fn normalize_path(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// What the note part of a link names, as far as the text alone proves.
enum NoteRef {
    /// No note part (`[[#Heading]]`): the source note itself.
    Source,
    /// A bare name: the note's stem, whichever folder it lives in.
    Bare(String),
    /// A path (`folder/Note`, `./Note`, `../Note`) normalized to a file path.
    Path { stem: String, path: String },
    /// A path that climbs above the repository root; kept as raw spelling.
    Escapes(String),
}

fn note_ref(note: &str, source_dir: &str) -> NoteRef {
    if note.is_empty() {
        return NoteRef::Source;
    }
    let relative = note.starts_with("./") || note.starts_with("../");
    if !relative && !note.contains('/') {
        return NoteRef::Bare(note.to_string());
    }
    let joined = if relative && !source_dir.is_empty() {
        format!("{source_dir}/{note}")
    } else {
        note.to_string()
    };
    match normalize_path(&joined) {
        Some(path) if !path.is_empty() => {
            let stem = path.rsplit('/').next().unwrap_or(&path).to_string();
            NoteRef::Path {
                stem,
                path: format!("{path}.md"),
            }
        }
        _ => NoteRef::Escapes(note.to_string()),
    }
}

/// Blanks out inline code spans (a run of N backticks through the next run of
/// exactly N) with spaces, keeping the text's length, so a link or tag inside
/// code is never scanned. An unclosed run is literal text.
fn mask_code_spans(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut masked = chars.clone();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| **c == '`').count();
        let mut j = i + run;
        let mut close = None;
        while j < chars.len() {
            if chars[j] == '`' {
                let r = chars[j..].iter().take_while(|c| **c == '`').count();
                if r == run {
                    close = Some(j + r);
                    break;
                }
                j += r;
            } else {
                j += 1;
            }
        }
        match close {
            Some(end) => {
                masked[i..end].iter_mut().for_each(|c| *c = ' ');
                i = end;
            }
            None => i += run,
        }
    }
    masked.into_iter().collect()
}

/// Extracts `#tag` occurrences from raw text, each returned as its bare name.
/// A `#` only starts a tag when preceded by start-of-text or whitespace — this
/// keeps a URL fragment like `.../page#section` from being a tag. A candidate
/// that is entirely ASCII digits (`#123`) is rejected (issue/PR references,
/// not tags). The character class is Unicode-broad (`char::is_alphanumeric`,
/// `_`, `/`, `-`), like real Obsidian tagging.
fn scan_tags(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_boundary = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '#' && at_boundary {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && is_tag_char(chars[end]) {
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

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '/' | '-')
}

/// The safe frontmatter subset: `title`, `aliases` and `tags`, each with the
/// 1-based line it was read from.
#[derive(Default)]
struct Frontmatter {
    /// 1-based line of the closing delimiter; 0 when there is no frontmatter.
    end_line: u32,
    title: Option<(String, u32)>,
    aliases: Vec<(String, u32)>,
    tags: Vec<(String, u32)>,
}

const MAX_FRONTMATTER_ITEMS: usize = 64;

/// Reads an optional `---` ... `---` (or `...`) block at the very start of the
/// document. Never evaluates anything: a key outside the subset, a nested map
/// or an unreadable value is skipped, and an unterminated block is not
/// frontmatter at all.
fn parse_frontmatter(source: &str) -> Frontmatter {
    let mut lines = source.lines().enumerate();
    if lines.next().map(|(_, l)| l.trim_end()) != Some("---") {
        return Frontmatter::default();
    }
    let body: Vec<(usize, &str)> = lines.collect();
    let Some(end) = body
        .iter()
        .position(|(_, l)| matches!(l.trim_end(), "---" | "..."))
    else {
        return Frontmatter::default();
    };
    let mut fm = Frontmatter {
        end_line: body[end].0 as u32 + 1,
        ..Default::default()
    };
    let mut i = 0;
    while i < end {
        let (idx, line) = body[i];
        i += 1;
        let line_no = idx as u32 + 1;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.starts_with([' ', '\t', '-']) {
            continue;
        }
        let key = key.trim();
        if !matches!(key, "title" | "aliases" | "tags") {
            continue;
        }
        let value = value.trim();
        let mut items: Vec<String> = Vec::new();
        if value.is_empty() {
            while i < end {
                let item = body[i].1.trim_start();
                match item.strip_prefix('-') {
                    Some(rest) => {
                        items.push(unquote(rest.trim()));
                        i += 1;
                    }
                    _ => break,
                }
            }
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            items.extend(inner.split(',').map(|s| unquote(s.trim())));
        } else {
            items.push(unquote(value));
        }
        items.retain(|s| !s.is_empty());
        match key {
            "title" => {
                if let Some(first) = items.into_iter().next() {
                    fm.title = Some((first, line_no));
                }
            }
            "aliases" => fm.aliases.extend(
                items
                    .into_iter()
                    .map(|a| (a, line_no))
                    .take(MAX_FRONTMATTER_ITEMS),
            ),
            _ => {
                let tags = items
                    .into_iter()
                    .flat_map(|t| {
                        t.split([',', ' '])
                            .map(|p| p.trim_start_matches('#').to_string())
                            .collect::<Vec<_>>()
                    })
                    .filter(|t| {
                        !t.is_empty()
                            && t.chars().all(is_tag_char)
                            && !t.chars().all(|c| c.is_ascii_digit())
                    });
                fm.tags
                    .extend(tags.map(|t| (t, line_no)).take(MAX_FRONTMATTER_ITEMS));
            }
        }
    }
    fm
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(quote).and_then(|r| r.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    s.to_string()
}

/// Appends, in document order, every block-level node under `node`, looking
/// through `section` wrappers. `tree-sitter-md` nests sections for ATX
/// headings only (a Setext heading does not open one), so the heading
/// hierarchy is rebuilt from heading levels instead of from that nesting.
fn flatten_blocks<'t>(node: Node<'t>, depth: u32, out: &mut Vec<Node<'t>>) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "section" {
            flatten_blocks(child, depth + 1, out);
        } else {
            out.push(child);
        }
    }
}

struct Walker<'a> {
    source: &'a str,
    /// Repository-relative path of the note being parsed (also its identity).
    note_path: String,
    note_dir: String,
    note_name: String,
    frontmatter: Frontmatter,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(path: &str, source: &'a str) -> Self {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        Self {
            source,
            note_path: path.to_string(),
            note_dir: dir.to_string(),
            note_name: strip_md_extension(file),
            frontmatter: parse_frontmatter(source),
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
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

    /// The note's own `Module` symbol, spanning the whole file.
    fn push_note(&mut self) -> SymbolId {
        let location = Location {
            line: 1,
            column: 1,
            byte_len: self.source.len() as u32,
            end_line: Some(self.source.lines().count().max(1) as u32),
        };
        self.push_symbol(
            self.note_name.clone(),
            SymbolKind::Module,
            location,
            None,
            None,
        )
    }

    fn push_relation(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        to_name: String,
        location: Location,
        target: Option<RelationTarget>,
    ) {
        if let Some(mut target) = target {
            target.relation = self.relations.len();
            self.relation_targets.push(target);
        }
        self.relations.push(SymbolRelation {
            from,
            kind,
            to_name,
            location,
        });
    }

    /// `tag:`/`alias:`/`title:` references from the note for its frontmatter.
    fn push_frontmatter_relations(&mut self, note: SymbolId) {
        let fm = std::mem::take(&mut self.frontmatter);
        let site = |line: u32| Location {
            line,
            column: 1,
            byte_len: 0,
            end_line: None,
        };
        let entries = fm
            .title
            .iter()
            .map(|(v, l)| (format!("title:{v}"), *l))
            .chain(fm.aliases.iter().map(|(v, l)| (format!("alias:{v}"), *l)))
            .chain(fm.tags.iter().map(|(v, l)| (format!("tag:{v}"), *l)));
        for (to_name, line) in entries {
            self.push_relation(note, RelationKind::References, to_name, site(line), None);
        }
        self.frontmatter.end_line = fm.end_line;
    }

    /// Scans `text` (code spans masked) for links, embeds and `#tag`s and
    /// records each as a relation from `from`. `loc` is the whole containing
    /// block's location — relations are block-granular, not exact-span.
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        let text = mask_code_spans(text);
        for link in scan_wikilinks(&text) {
            let kind = if link.embed {
                RelationKind::Imports
            } else {
                RelationKind::References
            };
            let (note_target, heading_scope) = match note_ref(&link.note, &self.note_dir) {
                NoteRef::Source => (None, (Some(self.note_path.clone()), None)),
                NoteRef::Bare(name) => (Some((name.clone(), None)), (None, Some(name))),
                NoteRef::Path { stem, path } => {
                    (Some((stem, Some(path.clone()))), (Some(path), None))
                }
                NoteRef::Escapes(raw) => {
                    (Some((raw.clone(), Some(raw.clone()))), (Some(raw), None))
                }
            };
            if let Some((name, path)) = note_target {
                let target = RelationTarget {
                    path,
                    kind: Some(SymbolKind::Module),
                    ..Default::default()
                };
                self.push_relation(from, kind, name, loc, Some(target));
            }
            if let Some(heading) = link.heading {
                let target = RelationTarget {
                    path: heading_scope.0,
                    target_module: heading_scope.1,
                    kind: Some(SymbolKind::Element),
                    ..Default::default()
                };
                self.push_relation(from, kind, heading, loc, Some(target));
            }
        }
        for tag in scan_tags(&text) {
            self.push_relation(
                from,
                RelationKind::References,
                format!("tag:{tag}"),
                loc,
                None,
            );
        }
    }

    /// Walks the document's blocks in order. A heading closes every open
    /// heading of equal or shallower level and becomes a child of the nearest
    /// shallower one (or of the note); other blocks belong to the innermost
    /// open heading, or to the note before the first heading.
    fn walk_document(&mut self, root: Node, note: SymbolId) {
        let mut blocks = Vec::new();
        flatten_blocks(root, 0, &mut blocks);
        // (level, symbol id, name) of the open headings, shallowest first.
        let mut open: Vec<(u32, SymbolId, String)> = Vec::new();
        // Per heading symbol: (id, level, start row, start byte), for its range.
        let mut starts: Vec<(SymbolId, u32, usize, usize)> = Vec::new();
        for block in blocks {
            match block.kind() {
                "atx_heading" | "setext_heading" => {
                    let level = heading_level(block).unwrap_or(1);
                    while open.last().is_some_and(|(l, _, _)| *l >= level) {
                        open.pop();
                    }
                    let parent = open.last().map(|(_, _, n)| n.clone());
                    let name = heading_text(block, self.source);
                    let heading_loc = location(block);
                    let id = self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        heading_loc,
                        parent,
                        Some(level),
                    );
                    self.push_relations_from_text(id, &name, heading_loc);
                    starts.push((id, level, block.start_position().row, block.start_byte()));
                    open.push((level, id, name));
                }
                _ => {
                    let owner = open.last().map_or(note, |(_, id, _)| *id);
                    self.scan_paragraphs(block, owner, 0);
                }
            }
        }
        // A section runs to the line before the next heading of equal or
        // shallower level (so it contains its descendants), else to the end.
        let total_lines = self.source.lines().count().max(1) as u32;
        for (i, &(id, level, _, start_byte)) in starts.iter().enumerate() {
            let next = starts[i + 1..].iter().find(|(_, l, _, _)| *l <= level);
            let (end_line, end_byte) = match next {
                Some(&(_, _, row, byte)) => (row as u32, byte),
                None => (total_lines, self.source.len()),
            };
            if let Some(symbol) = self.symbols.iter_mut().find(|s| s.id == id) {
                symbol.location.end_line = Some(end_line.max(symbol.location.line));
                symbol.location.byte_len = end_byte.saturating_sub(start_byte) as u32;
            }
        }
    }

    /// Scans every `paragraph` under `node` (list items and blockquotes wrap
    /// theirs; table cells and code blocks hold none) as owned by `owner`.
    fn scan_paragraphs(&mut self, node: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        if node.kind() == "paragraph" {
            let in_frontmatter = (node.start_position().row as u32) < self.frontmatter.end_line;
            if !in_frontmatter {
                let text = node.utf8_text(self.source.as_bytes()).unwrap_or_default();
                self.push_relations_from_text(owner, text, location(node));
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.scan_paragraphs(child, owner, depth + 1);
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            ..Default::default()
        }
    }
}
