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
//!
//! A nested rule (CSS Nesting) is named after its selector resolved against
//! its parent rule's (#115): every `&` (a `nesting_selector` node — never an
//! `&` inside a string or an escaped `\&`) becomes the parent selector, and a
//! selector without `&` is prefixed by the parent and a space (`.a { .b {} }`
//! is `.a .b`, `.a { > .c {} }` is `.a > .c`). Since `&` stands for
//! `:is(<parent list>)` (CSS Nesting §4), comma lists expand to one name per
//! combination, never a `:is()` list: one per (child, parent) pair, and with
//! several `&` in one selector every `&` takes every parent independently
//! (`.a, .b { & + & {} }` is `.a + .a`, `.a + .b`, `.b + .a`, `.b + .b`). A
//! resolved selector written with `&` is indexed under its resolved names
//! only, never also as the literal `&…`; one written without `&` also keeps
//! its name as written. The parent context crosses any at-rule block
//! (`@media`, `@supports`, `@layer`, `@container`, …) except `@scope`, whose
//! `&` is its scoping root. Where there is no parent selector (a top-level
//! `&`, `@utility`/`@custom-variant` blocks) the selector is indexed as
//! written and so are its descendants.
//!
//! A nested rule over [`MAX_RESOLVED_SELECTORS_PER_RULE`] (or the per-file
//! budget, or the depth guard) is not expanded, partially or at all, and the
//! file still parses: each of its selectors is indexed as written followed by
//! an [`UNEXPANDED_MARKER`] comment naming the reason
//! (`&.c0 /* mct: not expanded, over 256 selectors */`), and so is every
//! selector nested in it (`… /* mct: not expanded, parent not expanded */`).
//! A selector without `&` also keeps its plain written name, as above.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location};
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// The most resolved names one nested rule may expand to: the sum, over its
/// selectors, of the parent count raised to the selector's `&` count (one
/// for a selector without `&`). Exactly this many is expanded; one more and
/// the rule is indexed unexpanded, with [`UNEXPANDED_MARKER`].
const MAX_RESOLVED_SELECTORS_PER_RULE: usize = 256;

/// Start of the comment appended to a selector that was indexed without
/// being expanded against its parent: search for it to find such rules.
pub const UNEXPANDED_MARKER: &str = "/* mct: not expanded";

/// Safety bound for adversarial input: every resolved name costs its length
/// plus one `SymbolRecord` against a per-file budget of this base plus
/// [`RESOLVED_BUDGET_PER_SOURCE_BYTE`] per source byte. Once spent, later
/// nested rules are indexed unexpanded. It stops `& + &` chains, whose length
/// doubles per level, and many children of a wide parent list from growing
/// memory far beyond the file's own size.
const RESOLVED_BUDGET_BASE: usize = 1 << 20;
const RESOLVED_BUDGET_PER_SOURCE_BYTE: usize = 8;

#[cfg(test)]
mod libraries;

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
        walker.visit_children(root, module_id, 0, Parents::Root);
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

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// The selector context a rule is nested in.
#[derive(Clone, Copy)]
enum Parents<'p> {
    /// No enclosing style rule: the top level, or a block (such as
    /// `@scope`'s or `@utility`'s) whose `&` is not a parent selector.
    Root,
    /// The enclosing rule's resolved selectors (never empty).
    Resolved(&'p [String]),
    /// An enclosing rule had no parent for its `&`: names here are indexed
    /// as written.
    Unresolved,
    /// An enclosing nested rule was not expanded: names here are indexed as
    /// written, marked with [`Unexpanded::ParentNotExpanded`].
    Unexpanded,
}

/// What a rule passes down to the rules nested in it.
enum Inner {
    Resolved(Vec<String>),
    Unresolved,
    Unexpanded,
}

/// Why a nested selector was indexed without being expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unexpanded {
    /// Over [`MAX_RESOLVED_SELECTORS_PER_RULE`].
    TooMany,
    /// Over the per-file budget (see [`RESOLVED_BUDGET_BASE`]).
    Budget,
    /// The depth guard cut the search for `&` nodes short.
    TooDeep,
    /// An enclosing nested rule was not expanded.
    ParentNotExpanded,
}

impl Unexpanded {
    /// `name` followed by [`UNEXPANDED_MARKER`] and this reason.
    fn mark(self, name: &str) -> String {
        let reason = match self {
            Unexpanded::TooMany => {
                format!("over {MAX_RESOLVED_SELECTORS_PER_RULE} selectors")
            }
            Unexpanded::Budget => "over the file budget".to_string(),
            Unexpanded::TooDeep => "nested too deep".to_string(),
            Unexpanded::ParentNotExpanded => "parent not expanded".to_string(),
        };
        format!("{name} {UNEXPANDED_MARKER}, {reason} */")
    }
}

/// One selector split around its `nesting_selector` (`&`) nodes.
struct SelectorParts {
    /// The unescaped text between the `&` nodes: one more than `amps`.
    pieces: Vec<String>,
    /// Start byte of each `&` node, ascending.
    amps: Vec<usize>,
    /// `false` when the depth guard stopped the search for `&` nodes.
    complete: bool,
}

impl SelectorParts {
    fn literal(&self) -> String {
        self.pieces.join("&")
    }

    /// How many names this selector resolves to against `parents` parent
    /// selectors (each `&` takes each parent independently), or `None` on
    /// overflow.
    fn expansion_count(&self, parents: usize) -> Option<usize> {
        if self.amps.is_empty() {
            return Some(parents);
        }
        parents.checked_pow(u32::try_from(self.amps.len()).ok()?)
    }

    /// Total length of every name this selector resolves to against
    /// `parents`, or `None` on overflow. With `k` `&` and `p` parents of total
    /// length `s`, each parent fills each `&` in `p^(k-1)` of the `p^k`
    /// names.
    fn resolved_bytes(&self, parents: &[String]) -> Option<usize> {
        let own = self
            .pieces
            .iter()
            .try_fold(0usize, |sum, piece| sum.checked_add(piece.len()))?;
        let parent_bytes = parents
            .iter()
            .try_fold(0usize, |sum, parent| sum.checked_add(parent.len()))?;
        let count = self.expansion_count(parents.len())?;
        if self.amps.is_empty() {
            return count
                .checked_mul(own.checked_add(1)?)?
                .checked_add(parent_bytes);
        }
        let per_parent = count / parents.len().max(1);
        let fills = per_parent
            .checked_mul(self.amps.len())?
            .checked_mul(parent_bytes)?;
        count.checked_mul(own)?.checked_add(fills)
    }

    /// Every name this selector resolves to against the non-empty `parents`,
    /// in source order of the parents with the last `&` varying fastest.
    fn resolve(&self, parents: &[String]) -> Vec<String> {
        if self.amps.is_empty() {
            let own = self.pieces.concat();
            return parents
                .iter()
                .map(|parent| format!("{parent} {own}"))
                .collect();
        }
        let mut choice = vec![0usize; self.amps.len()];
        let mut names = Vec::new();
        loop {
            let mut name = String::new();
            for (i, piece) in self.pieces.iter().enumerate() {
                name.push_str(piece);
                if let Some(parent) = choice.get(i).and_then(|&c| parents.get(c)) {
                    name.push_str(parent);
                }
            }
            names.push(name);
            // Advance `choice` like an odometer; done once it wraps around.
            let mut advanced = false;
            for slot in choice.iter_mut().rev() {
                *slot += 1;
                if *slot < parents.len() {
                    advanced = true;
                    break;
                }
                *slot = 0;
            }
            if !advanced {
                return names;
            }
        }
    }
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
    /// What is left of the per-file budget for resolved nested names.
    resolved_budget: usize,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
            resolved_budget: RESOLVED_BUDGET_BASE
                .saturating_add(source.len().saturating_mul(RESOLVED_BUDGET_PER_SOURCE_BYTE)),
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

    fn visit_children(&mut self, node: Node, owner: SymbolId, depth: u32, parents: Parents) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, depth + 1, parents);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, depth: u32, parents: Parents) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "rule_set" => {
                // The block may nest rules (CSS Nesting), directly or inside
                // at-rules; they resolve against this rule's selectors.
                match self.index_rule_set(node, parents, depth) {
                    Inner::Resolved(names) if !names.is_empty() => {
                        self.visit_children(node, owner, depth + 1, Parents::Resolved(&names));
                    }
                    Inner::Resolved(_) | Inner::Unresolved => {
                        self.visit_children(node, owner, depth + 1, Parents::Unresolved);
                    }
                    Inner::Unexpanded => {
                        self.visit_children(node, owner, depth + 1, Parents::Unexpanded);
                    }
                }
            }
            // Inside `@scope`, `&` is the scoping root, not a parent rule.
            "scope_statement" => self.visit_children(node, owner, depth + 1, Parents::Root),
            "import_statement" => {
                if let Some(target) = import_target(node, self.source) {
                    self.push_relation(owner, RelationKind::Imports, target, location(node));
                }
            }
            _ => self.visit_children(node, owner, depth + 1, parents),
        }
    }

    /// Indexes every selector of `rule_set` and returns what its nested
    /// rules resolve against (see [`Parents`]).
    fn index_rule_set(&mut self, rule_set: Node, parents: Parents, depth: u32) -> Inner {
        let Some(selectors) = find_child(rule_set, "selectors") else {
            return match parents {
                Parents::Unexpanded => Inner::Unexpanded,
                _ => Inner::Unresolved,
            };
        };
        let mut cursor = selectors.walk();
        let nodes: Vec<Node> = selectors.named_children(&mut cursor).collect();
        let parts: Vec<SelectorParts> = nodes
            .iter()
            .map(|node| selector_parts(*node, self.source, depth))
            .collect();
        let resolved = match parents {
            Parents::Resolved(parent_names) => self.resolve_nested(parent_names, &parts),
            Parents::Unexpanded => Err(Unexpanded::ParentNotExpanded),
            Parents::Root | Parents::Unresolved => {
                for (node, part) in nodes.iter().zip(&parts) {
                    self.index_selector(*node, rule_set, vec![part.literal()], &[]);
                }
                let resolvable = matches!(parents, Parents::Root)
                    && parts
                        .iter()
                        .all(|part| part.complete && part.amps.is_empty());
                return if resolvable {
                    Inner::Resolved(parts.iter().map(SelectorParts::literal).collect())
                } else {
                    Inner::Unresolved
                };
            }
        };
        match resolved {
            Ok(per_selector) => {
                let mut all = Vec::new();
                for ((node, part), mut names) in nodes.iter().zip(&parts).zip(per_selector) {
                    all.extend(names.iter().cloned());
                    if part.amps.is_empty() {
                        names.push(part.literal());
                    }
                    self.index_selector(*node, rule_set, names, &part.amps);
                }
                Inner::Resolved(all)
            }
            Err(reason) => {
                for (node, part) in nodes.iter().zip(&parts) {
                    let literal = part.literal();
                    let mut names = vec![reason.mark(&literal)];
                    if part.amps.is_empty() {
                        names.push(literal);
                    }
                    self.index_selector(*node, rule_set, names, &part.amps);
                }
                Inner::Unexpanded
            }
        }
    }

    /// Every selector of a nested rule resolved against `parents`, one list
    /// per selector, or why it is not expanded: over
    /// [`MAX_RESOLVED_SELECTORS_PER_RULE`] in total, over the per-file
    /// budget, or a selector's `&` search was cut short. Sizes are checked
    /// before any name is built.
    fn resolve_nested(
        &mut self,
        parents: &[String],
        parts: &[SelectorParts],
    ) -> Result<Vec<Vec<String>>, Unexpanded> {
        if !parts.iter().all(|part| part.complete) {
            return Err(Unexpanded::TooDeep);
        }
        let mut count = 0usize;
        for part in parts {
            count = part
                .expansion_count(parents.len())
                .and_then(|n| count.checked_add(n))
                .filter(|&total| total <= MAX_RESOLVED_SELECTORS_PER_RULE)
                .ok_or(Unexpanded::TooMany)?;
        }
        let record = std::mem::size_of::<SymbolRecord>();
        let mut cost = count.checked_mul(record).ok_or(Unexpanded::Budget)?;
        for part in parts {
            cost = part
                .resolved_bytes(parents)
                .and_then(|bytes| cost.checked_add(bytes))
                .ok_or(Unexpanded::Budget)?;
        }
        if cost > self.resolved_budget {
            return Err(Unexpanded::Budget);
        }
        self.resolved_budget -= cost;
        Ok(parts.iter().map(|part| part.resolve(parents)).collect())
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            ..Default::default()
        }
    }

    /// Indexes one selector from a rule's (possibly comma-separated)
    /// selector list: its full names (as written, or resolved against the
    /// parent rule), plus any atomic class/id/tag components nested inside
    /// it, except those containing a `&` that starts at a byte in `hidden`.
    /// See the module doc. `rule_set` is the enclosing rule, whose end is
    /// every full name's.
    fn index_selector(
        &mut self,
        selector: Node,
        rule_set: Node,
        full_names: Vec<String>,
        hidden: &[usize],
    ) {
        if full_names.iter().all(String::is_empty) {
            return;
        }
        let mut atoms: Vec<(String, Location)> = Vec::new();
        collect_selector_atoms(selector, self.source, hidden, &mut atoms);
        let mut rule_location = location(selector);
        rule_location.end_line = Some(rule_set.end_position().row as u32 + 1);
        rule_location.byte_len = (rule_set.end_byte() - selector.start_byte()) as u32;
        let fulls = full_names.into_iter().map(|name| (name, rule_location));
        let mut seen: HashSet<String> = HashSet::new();
        for (name, loc) in fulls.chain(atoms) {
            if name.is_empty() || seen.contains(&name) {
                continue;
            }
            seen.insert(name.clone());
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

/// Decodes CSS escapes the way CSS Syntax 3 §4.3.7 does, so escaped
/// utility-class names (Tailwind's `md\:flex`, `w-1\/2`, `\33xl\:…`) match
/// the plain-text form an HTML `class` attribute uses (`md:flex`, `w-1/2`,
/// `3xl:…`): `\` + 1–6 hex digits (plus one optional whitespace) is that
/// code point — U+FFFD for zero, a surrogate or past U+10FFFF — and `\`
/// before any other character is that character.
fn unescape_css(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 {
            match chars.next_if(char::is_ascii_hexdigit) {
                Some(d) => hex.push(d),
                None => break,
            }
        }
        if hex.is_empty() {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        chars.next_if(|c| matches!(c, ' ' | '\t' | '\n'));
        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
        out.push(match char::from_u32(code) {
            Some(ch) if code != 0 => ch,
            _ => char::REPLACEMENT_CHARACTER,
        });
    }
    out
}

/// Splits `selector` around its `nesting_selector` nodes. `depth` is the
/// walker's depth at the rule, so the search stays within the same guard.
fn selector_parts(selector: Node, source: &str, depth: u32) -> SelectorParts {
    let mut amps: Vec<(usize, usize)> = Vec::new();
    let complete = collect_nesting_selectors(selector, &mut amps, depth);
    let mut pieces = Vec::with_capacity(amps.len() + 1);
    let mut from = selector.start_byte();
    for &(start, end) in &amps {
        pieces.push(unescape_css(source.get(from..start).unwrap_or_default()));
        from = end;
    }
    pieces.push(unescape_css(
        source.get(from..selector.end_byte()).unwrap_or_default(),
    ));
    SelectorParts {
        pieces,
        amps: amps.into_iter().map(|(start, _)| start).collect(),
        complete,
    }
}

/// Pushes the byte range of every `nesting_selector` under `node`, in source
/// order. Returns `false` when the depth guard cut the search short.
fn collect_nesting_selectors(node: Node, out: &mut Vec<(usize, usize)>, depth: u32) -> bool {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return false;
    }
    if node.kind() == "nesting_selector" {
        out.push((node.start_byte(), node.end_byte()));
        return true;
    }
    let mut complete = true;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        complete &= collect_nesting_selectors(child, out, depth + 1);
    }
    complete
}

/// Whether one of the ascending `amps` byte offsets falls inside `node`.
fn contains_any(node: Node, amps: &[usize]) -> bool {
    let first = amps.partition_point(|&byte| byte < node.start_byte());
    amps.get(first).is_some_and(|&byte| byte < node.end_byte())
}

/// Recursively walks a selector node, collecting one `(name, location)`
/// pair for every atomic class/id/tag/pseudo/attribute component nested
/// inside it — e.g. `div.container` yields `div` and `.container`;
/// `.navbar .nav-link` yields `.navbar` and `.nav-link`; `.btn:hover`
/// yields `.btn` and `.btn:hover`. A pseudo/attribute component whose text
/// contains a `&` starting at a byte in `hidden` is skipped (its resolved
/// form is a full name already). See the module doc for how these
/// combine with the full-selector name in `Walker::index_selector`.
fn collect_selector_atoms(
    node: Node,
    source: &str,
    hidden: &[usize],
    out: &mut Vec<(String, Location)>,
) {
    collect_selector_atoms_at_depth(node, source, hidden, out, 0);
}

fn collect_selector_atoms_at_depth(
    node: Node,
    source: &str,
    hidden: &[usize],
    out: &mut Vec<(String, Location)>,
    depth: u32,
) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
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
                    collect_selector_atoms_at_depth(child, source, hidden, out, depth + 1);
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
                    collect_selector_atoms_at_depth(child, source, hidden, out, depth + 1);
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
                collect_selector_atoms_at_depth(base, source, hidden, out, depth + 1);
            }
            if !contains_any(node, hidden) {
                out.push((unescape_css(text(node, source)), location(node)));
            }
        }
        "pseudo_class_selector" | "pseudo_element_selector" => {
            // Children are [base?, name-token]: a lone child is the
            // pseudo name itself (`::before`), not a base to recurse
            // into; two-or-more means the first child is a real base
            // selector (`.btn` in `.btn:hover`) and the rest is the name.
            if node.named_child_count() >= 2 {
                if let Some(base) = node.named_child(0) {
                    collect_selector_atoms_at_depth(base, source, hidden, out, depth + 1);
                }
            }
            if !contains_any(node, hidden) {
                out.push((unescape_css(text(node, source)), location(node)));
            }
        }
        "namespace_selector" => {
            // `svg|text`: the first child is the namespace prefix, not an
            // element — only what follows the `|` is a selector.
            let skip = usize::from(node.named_child_count() >= 2);
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor).skip(skip) {
                collect_selector_atoms_at_depth(child, source, hidden, out, depth + 1);
            }
        }
        "descendant_selector"
        | "child_selector"
        | "sibling_selector"
        | "adjacent_sibling_selector" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_selector_atoms_at_depth(child, source, hidden, out, depth + 1);
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

#[cfg(test)]
mod nesting_limit_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    const MAX: usize = MAX_RESOLVED_SELECTORS_PER_RULE;

    fn rule_names(src: &str) -> Vec<String> {
        CssParser
            .parse(&SourceFile {
                relative_path: "limit.css".to_string(),
                contents: src.to_string(),
            })
            .expect("valid CSS source should parse")
            .symbols
            .into_iter()
            .filter(|s| s.kind == SymbolKind::Rule)
            .map(|s| s.name)
            .collect()
    }

    fn list(prefix: &str, n: usize) -> String {
        (0..n)
            .map(|i| format!("{prefix}{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `.p0, … { &.c0, &.c1, … { .leaf {} } }` with `parents` × `children`
    /// resolved names for the middle rule.
    fn nested(parents: usize, children: usize) -> String {
        format!(
            "{} {{ {} {{ .leaf {{}} }} }}",
            list(".p", parents),
            list("&.c", children)
        )
    }

    fn over_limit(name: &str) -> String {
        format!("{name} /* mct: not expanded, over {MAX} selectors */")
    }

    fn parent_unexpanded(name: &str) -> String {
        format!("{name} /* mct: not expanded, parent not expanded */")
    }

    fn assert_resolved(parents: usize, children: usize) {
        let names = rule_names(&nested(parents, children));
        for p in 0..parents {
            for c in 0..children {
                let name = format!(".p{p}.c{c}");
                assert!(names.contains(&name), "missing {name}");
            }
        }
        assert!(!names.iter().any(|n| n.starts_with('&')), "literal kept");
        assert!(!names.iter().any(|n| n.contains(UNEXPANDED_MARKER)));
        let last = format!(".p{}.c{} .leaf", parents - 1, children - 1);
        assert!(names.contains(&last), "descendant not resolved: {last}");
    }

    fn assert_unexpanded(src: &str, literals: &[&str]) {
        let names = rule_names(src);
        // The file still parses; every selector of the rule is kept as
        // written and marked, with no partial expansion…
        for literal in literals {
            assert!(names.contains(&over_limit(literal)), "{literal}: {src}");
            assert!(!names.contains(&literal.to_string()), "bare {literal}");
        }
        assert!(!names.iter().any(|n| n.starts_with(".p0.c")), "partial");
        assert!(!names.iter().any(|n| n.starts_with(".p0 + ")), "partial");
        // …and what is nested in it is marked too, keeping its plain name.
        assert!(names.contains(&".leaf".to_string()));
        assert!(names.contains(&parent_unexpanded(".leaf")));
        assert!(!names.iter().any(|n| n.ends_with(" .leaf")));
    }

    #[test]
    fn expansion_just_under_the_limit_is_resolved() {
        assert_resolved(1, MAX - 1);
    }

    #[test]
    fn expansion_at_the_limit_is_resolved() {
        assert_resolved(1, MAX);
        assert_resolved(2, MAX / 2);
    }

    #[test]
    fn expansion_over_the_limit_is_kept_unexpanded_and_marked() {
        assert_unexpanded(&nested(1, MAX + 1), &["&.c0", "&.c256"]);
        assert_unexpanded(&nested(2, MAX / 2 + 1), &["&.c0", "&.c128"]);
    }

    #[test]
    fn repeated_ampersands_count_the_full_product_against_the_limit() {
        // Two parents and eight `&`: 2^8 = 256 names, exactly the limit.
        let amps = ["&"; 8].join(" + ");
        let src = format!(".p0, .p1 {{ {amps} {{ .leaf {{}} }} }}");
        let names = rule_names(&src);
        let resolved: Vec<&String> = names
            .iter()
            .filter(|n| n.matches(" + ").count() == 7 && !n.ends_with(" .leaf"))
            .collect();
        assert_eq!(resolved.len(), MAX, "{}", resolved.len());
        assert!(names.contains(&[".p1"; 8].join(" + ")));
        assert!(!names.iter().any(|n| n.contains(UNEXPANDED_MARKER)));
        // One more selector makes 257: the whole rule stays unexpanded.
        let src = format!(".p0, .p1 {{ {amps}, &.x {{ .leaf {{}} }} }}");
        assert_unexpanded(&src, &[&amps, "&.x"]);
    }

    #[test]
    fn a_child_without_ampersand_over_the_limit_keeps_its_plain_name() {
        let src = format!("{} {{ .x {{}} }}", list(".p", MAX + 1));
        let names = rule_names(&src);
        assert!(names.contains(&".x".to_string()));
        assert!(names.contains(&over_limit(".x")));
        assert!(!names.iter().any(|n| n.ends_with(" .x")));
    }

    #[test]
    fn the_per_file_budget_stops_resolution_without_failing_the_file() {
        // Every `& + &` level doubles the resolved length; the budget stops
        // it long before the 2^40-fold name and keeps the deeper levels as
        // written, marked.
        let src = format!(".a {{ {}{} }}", "& + & { ".repeat(40), "} ".repeat(40));
        let names = rule_names(&src);
        let bytes: usize = names.iter().map(String::len).sum();
        let budget = RESOLVED_BUDGET_BASE + src.len() * RESOLVED_BUDGET_PER_SOURCE_BYTE;
        assert!(bytes <= budget + src.len() * 2 + 40 * 64, "{bytes} bytes");
        assert!(names.contains(&".a + .a".to_string()));
        assert!(names.contains(&"& + & /* mct: not expanded, over the file budget */".to_string()));
        assert!(names.contains(&parent_unexpanded("& + &")));
        assert!(!names.contains(&"& + &".to_string()), "unmarked literal");
    }
}
