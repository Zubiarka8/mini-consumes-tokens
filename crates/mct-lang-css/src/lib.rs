//! `LanguageParser` implementation for CSS, via `tree-sitter-css`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for CSS support.
//!
//! Every selector in a rule's comma-separated list becomes one or more
//! `Rule` symbols, named exactly as written (unescaped — see
//! [`unescape_css`]) so they match an HTML `Element`'s outgoing
//! `References` by plain name equality — the same name-based matching every
//! other relation in this project uses:
//!
//! - A simple selector (`.foo`, `#foo`, a bare tag `div`, `::before`,
//!   `[data-x]`) becomes exactly one `Rule` symbol.
//! - The full-selector symbol spans from its selector to the rule's closing
//!   `}`, so a rule's range covers its declaration block; the atomic
//!   components below keep the range of their own token.
//! - A compound or combinator selector (`.a.b`, `div.foo`, `.a .b`,
//!   `#id > .c`, `input[type="text"]`, `.btn:hover`) becomes one `Rule`
//!   symbol for the full selector text *plus* one for each atomic
//!   class/id/tag component it's built from, since resolving which
//!   elements the compound as a whole matches needs a DOM, not an AST
//!   (same kind of deliberate limitation as Go's deferred
//!   `find_implementations`) — the atomic components are what actually
//!   lines up with HTML `Element` references.
//!
//! CSS properties/values inside a rule's block are not modeled at all —
//! only the selector.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
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
        let mut module_location = location(root);
        // The root node of a file ending in a newline ends at column 0 of
        // the (empty) line after the last one; the file's last line is the
        // one before that.
        let end = root.end_position();
        if end.column == 0 && end.row > 0 {
            module_location.end_line = Some(end.row as u32);
        }
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location);
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
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
        }
    }

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord {
            id,
            name,
            kind,
            location,
            parent: None,
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
                        self.index_selector(selector, node);
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
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            ..Default::default()
        }
    }

    /// Indexes one selector from a rule's (possibly comma-separated)
    /// selector list: the full selector as written, plus any atomic
    /// class/id/tag components nested inside it. See the module doc.
    /// `rule_set` is the enclosing rule, whose end is the full selector's.
    fn index_selector(&mut self, selector: Node, rule_set: Node) {
        let full = unescape_css(text(selector, self.source));
        if full.is_empty() {
            return;
        }
        let mut atoms: Vec<(String, Location)> = Vec::new();
        collect_selector_atoms(selector, self.source, &mut atoms);
        let mut rule_location = location(selector);
        rule_location.end_line = Some(rule_set.end_position().row as u32 + 1);
        rule_location.byte_len = (rule_set.end_byte() - selector.start_byte()) as u32;
        atoms.retain(|(name, _)| *name != full);
        atoms.insert(0, (full, rule_location));
        let mut seen: Vec<String> = Vec::new();
        for (name, loc) in atoms {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            self.push_symbol(name, SymbolKind::Rule, loc);
        }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

/// Drops the backslash of any CSS escape sequence, keeping the escaped
/// character literally (`md\:flex` -> `md:flex`, `w-1\/2` -> `w-1/2`) —
/// exactly what's needed to make escaped utility-class names (Tailwind's
/// `md:`, `hover:`, `w-1/2`, ...) match the plain-text form an HTML
/// `class` attribute uses. Numeric/hex CSS escapes (`\1F600`) are not
/// unescaped to a literal character; this project only needs to undo the
/// single-character escapes used to make otherwise-reserved punctuation
/// legal inside an identifier.
fn unescape_css(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Recursively walks a selector node, collecting one `(name, location)`
/// pair for every atomic class/id/tag/pseudo/attribute component nested
/// inside it — e.g. `div.container` yields `div` and `.container`;
/// `.navbar .nav-link` yields `.navbar` and `.nav-link`; `.btn:hover`
/// yields `.btn` and `.btn:hover`. See the module doc for how these
/// combine with the full-selector name in `Walker::index_selector`.
fn collect_selector_atoms(node: Node, source: &str, out: &mut Vec<(String, Location)>) {
    match node.kind() {
        "class_selector" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "class_name" {
                    out.push((
                        format!(".{}", unescape_css(text(child, source))),
                        location(child),
                    ));
                } else {
                    collect_selector_atoms(child, source, out);
                }
            }
        }
        "id_selector" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "id_name" {
                    out.push((
                        format!("#{}", unescape_css(text(child, source))),
                        location(child),
                    ));
                } else {
                    collect_selector_atoms(child, source, out);
                }
            }
        }
        "tag_name" => {
            out.push((unescape_css(text(node, source)), location(node)));
        }
        "attribute_selector" => {
            // The base is any selector, not just a simple one: the grammar
            // parses `.a .b[x]` as `[x]` applied to `.a .b`. A bare `[x]`'s
            // first child is its `attribute_name`, which adds no atom.
            if let Some(base) = node.named_child(0) {
                collect_selector_atoms(base, source, out);
            }
            out.push((unescape_css(text(node, source)), location(node)));
        }
        "pseudo_class_selector" | "pseudo_element_selector" => {
            // Children are [base?, name-token]: a lone child is the
            // pseudo name itself (`::before`), not a base to recurse
            // into; two-or-more means the first child is a real base
            // selector (`.btn` in `.btn:hover`) and the rest is the name.
            if node.named_child_count() >= 2 {
                if let Some(base) = node.named_child(0) {
                    collect_selector_atoms(base, source, out);
                }
            }
            out.push((unescape_css(text(node, source)), location(node)));
        }
        "namespace_selector" => {
            // `svg|text`: the first child is the namespace prefix, not an
            // element — only what follows the `|` is a selector.
            let skip = usize::from(node.named_child_count() >= 2);
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor).skip(skip) {
                collect_selector_atoms(child, source, out);
            }
        }
        "descendant_selector"
        | "child_selector"
        | "sibling_selector"
        | "adjacent_sibling_selector" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_selector_atoms(child, source, out);
            }
        }
        _ => {}
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
