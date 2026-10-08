//! `LanguageParser` implementation for Rust, via `tree-sitter-rust`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for Rust support.

use mct_core::{
    LanguageParser, LiteralCollector, Location, ParseError, ParsedFile, RelationKind,
    RelationTarget, SourceFile, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
    MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location};
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

pub struct RustParser;

impl LanguageParser for RustParser {
    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_rust::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("tree-sitter-rust grammar is statically valid");

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
        let mut walker = Walker::new(&file.contents, &file.relative_path);
        let mut module_location = location(root);
        // The root node of a file ending in a newline ends at column 0 of
        // the (empty) line after the last one; the file's last line is the
        // one before that.
        let end = root.end_position();
        if end.column == 0 && end.row > 0 {
            module_location.end_line = Some(end.row as u32);
        }
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location, None);
        collect_free_fn_names(root, &file.contents, &mut walker.free_fns, 0);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".rs")
        .to_string()
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

struct Walker<'a> {
    source: &'a str,
    /// This file's relative path: the lexical scope of a free-function call.
    path: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    literals: LiteralCollector,
    next_id: SymbolId,
    /// Names of the free (non-`impl`/`trait`) functions declared anywhere in
    /// this file — the only names a bare identifier is resolved against when
    /// it's used as a value rather than called (see the `identifier` arm).
    free_fns: HashSet<String>,
    /// Names bound as locals (parameters, `let`/`for`/`match`/closure
    /// patterns) in the function currently being walked: a bare identifier
    /// with one of these names is the local, never the same-named function.
    bound: HashSet<String>,
    /// Evidence for some of `relations`, by index (see `RelationTarget`).
    relation_targets: Vec<RelationTarget>,
    /// Base name of the `impl`/`trait` type whose method body is being
    /// walked — what `self.f()` and `Self::f()` are qualified by.
    self_type: Option<String>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            symbols: Vec::new(),
            relations: Vec::new(),
            literals: LiteralCollector::default(),
            next_id: 0,
            free_fns: HashSet::new(),
            bound: HashSet::new(),
            relation_targets: Vec::new(),
            self_type: None,
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
    ) -> Option<usize> {
        if to_name.is_empty() {
            return None;
        }
        self.relations.push(SymbolRelation {
            from,
            kind,
            to_name,
            location: loc,
        });
        Some(self.relations.len() - 1)
    }

    /// Attaches `target` (with its `relation` index filled in) to the
    /// relation `push_relation` just returned, unless it carries no evidence.
    fn qualify(&mut self, relation: Option<usize>, target: RelationTarget) {
        if let Some(relation) = relation {
            if target != RelationTarget::default() {
                self.relation_targets
                    .push(RelationTarget { relation, ..target });
            }
        }
    }

    /// Walks every child of `node`, attributing calls/imports found along the
    /// way to `owner` (the innermost enclosing function/method/module) and
    /// labeling methods with `impl_type` (the enclosing `impl Type` name, if
    /// any) as their parent.
    ///
    /// A `#[test]`-style attribute (see [`is_harness_attribute`]) directly
    /// above a function is recorded as a `References` relation from `owner`
    /// to that function: the test harness is what invokes it, so it is not an
    /// unreferenced function even though nothing in the repo calls it.
    fn visit_children(&mut self, node: Node, owner: SymbolId, impl_type: Option<&str>, depth: u32) {
        let mut harness_attribute: Option<Location> = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "attribute_item" => {
                    if is_harness_attribute(child, self.source) {
                        harness_attribute = Some(location(child));
                    }
                }
                "line_comment" | "block_comment" => {}
                "function_item" => {
                    if let Some(loc) = harness_attribute.take() {
                        let name = child
                            .child_by_field_name("name")
                            .map(|n| text(n, self.source).to_string())
                            .unwrap_or_default();
                        self.push_relation(owner, RelationKind::References, name, loc);
                    }
                }
                _ => harness_attribute = None,
            }
            self.visit(child, owner, impl_type, depth + 1);
        }
    }

    /// `depth` bounds native stack usage against adversarially deep/nested
    /// input (see `MAX_TRAVERSAL_DEPTH`) — every recursive call below passes
    /// `depth + 1`, and this early-return prunes the subtree instead of
    /// recursing further once the ceiling is hit.
    fn visit(&mut self, node: Node, owner: SymbolId, impl_type: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_item" | "function_signature_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if impl_type.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let id =
                    self.push_symbol(name, kind, location(node), impl_type.map(str::to_string));
                // Locals are per function: a nested `fn` starts from an empty
                // scope (it can't see its parent's locals) and the parent's
                // scope is restored once it's done.
                let mut locals = HashSet::new();
                collect_bound_names(node, self.source, &mut locals, 0);
                let outer = std::mem::replace(&mut self.bound, locals);
                let outer_self = std::mem::replace(&mut self.self_type, impl_type.map(type_base));
                // A function body is not an impl/trait body: an `fn` or
                // `const` nested inside a method belongs to the method, not
                // to the enclosing type, so the impl type stops here.
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, None, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
                self.bound = outer;
                self.self_type = outer_self;
            }
            // A bare identifier reaching here is in value position — a call's
            // callee, an item's own name and a `use` path are all handled by
            // their own arms and never visited as one. It's recorded only when
            // it names a free function of this same file that no local
            // shadows: `let f = helper;`, `.map(helper)`, or `helper(x)` inside
            // a macro's token tree (`params![helper(x)]`), which tree-sitter
            // leaves unparsed and so can't be told apart from a value use —
            // hence `References`, never `Calls`. Any other identifier (a
            // local, a field, a name from another file) is left alone.
            "identifier" => {
                let name = text(node, self.source);
                if self.free_fns.contains(name) && !self.bound.contains(name) {
                    self.push_relation(
                        owner,
                        RelationKind::References,
                        name.to_string(),
                        location(node),
                    );
                }
            }
            // Attribute arguments (`#[cfg(test)]`, `#[serde(default)]`) are
            // not value uses of anything; only their string literals are
            // kept, as before.
            "attribute_item" | "inner_attribute_item" => self.visit_literals(node, depth + 1),
            // A macro's arguments are a flat token stream: an identifier right
            // after `.` or `::` (`s.product`, `other::helper`) is a field,
            // method or path segment, never this file's free function.
            "token_tree" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    let after_accessor = child.kind() == "identifier"
                        && child
                            .prev_sibling()
                            .is_some_and(|p| matches!(p.kind(), "." | "::"));
                    if !after_accessor {
                        self.visit(child, owner, impl_type, depth + 1);
                    }
                }
            }
            // The macro's own name (`vec`, `params`) is not a value use.
            "macro_invocation" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "token_tree" {
                        self.visit(child, owner, impl_type, depth + 1);
                    }
                }
            }
            "struct_item" => {
                self.push_named(node, SymbolKind::Struct, None);
            }
            "enum_item" => {
                self.push_named(node, SymbolKind::Enum, None);
            }
            // Associated types/consts of an `impl`/`trait` hang off the type,
            // like its methods do.
            "type_item" | "associated_type" => {
                self.push_named(node, SymbolKind::TypeAlias, impl_type.map(str::to_string));
            }
            "const_item" | "static_item" => {
                let id = self.push_named(node, SymbolKind::Constant, impl_type.map(str::to_string));
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_children(value, id, impl_type, depth + 1);
                }
            }
            "trait_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), SymbolKind::Trait, location(node), None);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
            }
            "impl_item" => {
                let type_name = node
                    .child_by_field_name("type")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                if let Some(trait_node) = node.child_by_field_name("trait") {
                    let trait_name = text(trait_node, self.source).to_string();
                    self.push_relation(owner, RelationKind::Implements, trait_name, location(node));
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&type_name), depth + 1);
                }
            }
            "mod_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Module, location(node), None);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
            }
            "use_declaration" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    let mut names = Vec::new();
                    collect_use_names(argument, self.source, &mut names);
                    let external = is_std_path(text(argument, self.source));
                    for (name, loc) in names {
                        let relation = self.push_relation(owner, RelationKind::Imports, name, loc);
                        self.qualify(
                            relation,
                            RelationTarget {
                                external,
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some((name, name_node)) = call_target(function, self.source) {
                        let target = self.call_evidence(function, &name);
                        let relation = self.push_relation(
                            owner,
                            RelationKind::Calls,
                            name,
                            location(name_node),
                        );
                        self.qualify(relation, target);
                    }
                    // A bare callee is already the `Calls` above; visiting it
                    // would record it a second time as a value use. A
                    // turbofish (`helper::<T>`, `it.sum::<T>`) is looked
                    // through, so a method's receiver chain is still walked.
                    let callee = if function.kind() == "generic_function" {
                        function.child_by_field_name("function").unwrap_or(function)
                    } else {
                        function
                    };
                    if callee.kind() != "identifier" {
                        self.visit(callee, owner, impl_type, depth + 1);
                    }
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, impl_type, depth + 1);
                }
            }
            // `other::helper` may well be another module's `helper`, and a
            // label/lifetime (`'helper:`) is no use of a function at all:
            // none of their identifiers is resolved against this file.
            "scoped_identifier" | "scoped_type_identifier" | "label" | "lifetime" => {}
            "string_literal" | "raw_string_literal" => self.push_literal(node),
            _ => self.visit_children(node, owner, impl_type, depth + 1),
        }
    }

    /// What the callee expression proves about the called symbol: `Type::f`
    /// and `Self::f`/`self.f()` name the type it is declared in, a
    /// `std`/`core`/`alloc` path is external, and a bare call to a free
    /// function of this file that no local shadows stays in this file. A
    /// module path (`rand::f`, `crate::m::f`) names the module the target
    /// must live under, and another receiver (`x.f()`) can only reach a
    /// method, never provably which one.
    fn call_evidence(&self, function: Node, name: &str) -> RelationTarget {
        let function = if function.kind() == "generic_function" {
            function.child_by_field_name("function").unwrap_or(function)
        } else {
            function
        };
        match function.kind() {
            "identifier" if self.free_fns.contains(name) && !self.bound.contains(name) => {
                RelationTarget {
                    path: Some(self.path.to_string()),
                    ..Default::default()
                }
            }
            "scoped_identifier" => {
                let Some(path) = function.child_by_field_name("path") else {
                    return RelationTarget::default();
                };
                let path = text(path, self.source);
                if is_std_path(path) {
                    return RelationTarget {
                        external: true,
                        ..Default::default()
                    };
                }
                let mut segments = path_segments(path);
                let qualifier = match segments.last().map(String::as_str) {
                    Some("Self") => {
                        segments.pop();
                        self.self_type.clone()
                    }
                    Some(last) if last.starts_with(|c: char| c.is_ascii_uppercase()) => {
                        segments.pop()
                    }
                    _ => None,
                };
                // What's left is the module path: `self::f` stays in this
                // file, `rand::f`/`crate::m::f` must live under `rand`/`m`;
                // a bare `crate::`/`super::` proves nothing.
                let (path, module) = match segments.last().map(String::as_str) {
                    Some("self") => (Some(self.path.to_string()), None),
                    Some("crate" | "super") | None => (None, None),
                    Some(_) => (None, segments.pop()),
                };
                RelationTarget {
                    qualifier,
                    path,
                    module,
                    ..Default::default()
                }
            }
            "field_expression" => {
                let on_self = function
                    .child_by_field_name("value")
                    .is_some_and(|v| v.kind() == "self");
                let qualifier = on_self.then(|| self.self_type.clone()).flatten();
                RelationTarget {
                    member: qualifier.is_none(),
                    qualifier,
                    ..Default::default()
                }
            }
            _ => RelationTarget::default(),
        }
    }

    /// Records the string literals under `node` and nothing else.
    fn visit_literals(&mut self, node: Node, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        if matches!(node.kind(), "string_literal" | "raw_string_literal") {
            self.push_literal(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_literals(child, depth + 1);
        }
    }

    /// Records a string literal's fixed fragments — the text between
    /// `format!`-style `{...}` holes — each on the line it starts on.
    fn push_literal(&mut self, node: Node) {
        let raw = text(node, self.source);
        let Some(inner) = literal_body(raw) else {
            return;
        };
        let line = node.start_position().row as u32 + 1;
        let is_raw = node.kind() == "raw_string_literal";
        for (offset, fragment) in format_fragments(inner) {
            // Line of the fragment's first visible char, not of whitespace
            // (a newline) it may start with.
            let leading = fragment.len() - fragment.trim_start().len();
            let before = inner.get(..offset + leading).unwrap_or_default();
            let fragment_line = line + before.matches('\n').count() as u32;
            self.literals
                .push(&unescape(fragment, is_raw), fragment_line);
        }
    }

    fn push_named(&mut self, node: Node, kind: SymbolKind, parent: Option<String>) -> SymbolId {
        let name = node
            .child_by_field_name("name")
            .map(|n| text(n, self.source).to_string())
            .unwrap_or_default();
        self.push_symbol(name, kind, location(node), parent)
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            literals: self.literals.finish(),
            relation_targets: self.relation_targets,
        }
    }
}

/// Whether a path is rooted in the standard library (`std::`, `core::`,
/// `alloc::`, optionally with a leading `::`) — the one case this parser can
/// prove a target lies outside the repository.
fn is_std_path(path: &str) -> bool {
    let root = path
        .trim_start_matches("::")
        .split("::")
        .next()
        .unwrap_or_default()
        .trim();
    matches!(root, "std" | "core" | "alloc")
}

/// The segments of a type or path, without generic arguments:
/// `crate::m::Foo<T>` is `[crate, m, Foo]`, `Foo::<T>` is `[Foo]`.
fn path_segments(path: &str) -> Vec<String> {
    let mut depth = 0u32;
    let bare: String = path
        .chars()
        .filter(|&c| {
            match c {
                '<' => depth += 1,
                '>' => depth = depth.saturating_sub(1),
                _ => return depth == 0,
            }
            false
        })
        .collect();
    bare.split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The last segment of a type or path, without generic arguments — how an
/// `impl`'s type is matched against a qualifier.
fn type_base(path: &str) -> String {
    path_segments(path).pop().unwrap_or_default()
}

/// The callee's name *and* the specific node it came from — never the whole
/// `call_expression`, whose start position is shared by every call in a
/// chain (`a.f(x).f(y)`'s outer and inner `call_expression` both start at
/// `a`), which would otherwise make two same-named chained calls collide
/// into one indistinguishable relation row.
fn call_target<'a>(node: Node<'a>, source: &str) -> Option<(String, Node<'a>)> {
    match node.kind() {
        "identifier" => Some((text(node, source).to_string(), node)),
        "field_expression" => node
            .child_by_field_name("field")
            .map(|n| (text(n, source).to_string(), n)),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .map(|n| (text(n, source).to_string(), n)),
        "generic_function" => node
            .child_by_field_name("function")
            .and_then(|n| call_target(n, source)),
        _ => None,
    }
}

/// Names of every `fn` item under `node` that isn't an `impl`/`trait`
/// member — those are methods, reached as `Type::f`/`x.f()`, never by a bare
/// name.
fn collect_free_fn_names(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match node.kind() {
        "impl_item" | "trait_item" => return,
        "function_item" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.insert(text(name, source).to_string());
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_free_fn_names(child, source, out, depth + 1);
    }
}

/// Every name a pattern binds anywhere in `node` — parameters and
/// `let`/`if let`/`for`/`match` arm patterns (all a `pattern` field) plus
/// closure parameters — without descending into a nested `fn`, which has its
/// own scope. Over-approximates on purpose: an enum variant in a pattern
/// (`Some(x)` binds `x` but also yields `Some`) or a binding in a sibling
/// block only makes the value-use check skip that name.
fn collect_bound_names(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_item" {
            continue;
        }
        let is_pattern = node
            .child_by_field_name("pattern")
            .is_some_and(|p| p.id() == child.id())
            || node.kind() == "closure_parameters";
        if is_pattern {
            collect_identifiers(child, source, out, depth + 1);
        } else {
            collect_bound_names(child, source, out, depth + 1);
        }
    }
}

fn collect_identifiers(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    // A struct pattern's shorthand field (`Config { root, .. }`) binds a local
    // named after the field, just like a plain identifier pattern.
    if matches!(node.kind(), "identifier" | "shorthand_field_identifier") {
        out.insert(text(node, source).to_string());
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, source, out, depth + 1);
    }
}

/// `#[test]`, `#[bench]`, `#[tokio::test]`/`#[async_std::test]` (any path
/// ending in `test`), `#[rstest]`, `#[test_case(..)]`: attributes that hand
/// the function to a test/bench harness. `#[cfg(test)]` is not one — it only
/// gates compilation, and a helper under it can still be genuinely unused.
fn is_harness_attribute(attribute_item: Node, source: &str) -> bool {
    let mut cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .named_children(&mut cursor)
        .find(|c| c.kind() == "attribute")
    else {
        return false;
    };
    let mut cursor = attribute.walk();
    let Some(path) = attribute.named_children(&mut cursor).next() else {
        return false;
    };
    let last = match path.kind() {
        "identifier" => text(path, source),
        "scoped_identifier" => path
            .child_by_field_name("name")
            .map(|n| text(n, source))
            .unwrap_or_default(),
        _ => return false,
    };
    matches!(last, "test" | "bench" | "rstest" | "test_case")
}

fn collect_use_names(node: Node, source: &str, out: &mut Vec<(String, Location)>) {
    match node.kind() {
        "identifier" | "type_identifier" => {
            out.push((text(node, source).to_string(), location(node)))
        }
        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.push((text(name, source).to_string(), location(name)));
            }
        }
        "use_as_clause" => {
            if let Some(alias) = node.child_by_field_name("alias") {
                out.push((text(alias, source).to_string(), location(alias)));
            } else if let Some(path) = node.child_by_field_name("path") {
                collect_use_names(path, source, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_use_names(child, source, out);
            }
        }
        "scoped_use_list" => {
            if let Some(list) = node.child_by_field_name("list") {
                collect_use_names(list, source, out);
            }
        }
        _ => {}
    }
}

/// The text between a literal's quotes: `"..."`, `b"..."`, `r#"..."#`.
fn literal_body(raw: &str) -> Option<&str> {
    let start = raw.find('"')? + 1;
    let end = raw.rfind('"')?;
    raw.get(start..end)
}

/// `body` split at `format!`-style holes (`{}`, `{name:?}`), with each fixed
/// fragment's byte offset in `body`. `{{`/`}}` are escaped braces, not holes.
fn format_fragments(body: &str) -> Vec<(usize, &str)> {
    let mut fragments = Vec::new();
    let mut start = 0;
    let mut in_hole = false;
    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match (in_hole, c) {
            (false, '{') if chars.peek().is_some_and(|&(_, n)| n == '{') => {
                chars.next();
            }
            (false, '}') if chars.peek().is_some_and(|&(_, n)| n == '}') => {
                chars.next();
            }
            (false, '{') => {
                fragments.push((start, body.get(start..i).unwrap_or_default()));
                in_hole = true;
            }
            (true, '}') => {
                start = i + 1;
                in_hole = false;
            }
            _ => {}
        }
    }
    if !in_hole {
        fragments.push((start, body.get(start..).unwrap_or_default()));
    }
    fragments
}

/// A literal fragment's source text with escapes resolved as far as matters
/// for search: quotes and backslashes kept, every other escape (`\n`,
/// `\u{..}`, a line continuation) read as a word break — except in a raw
/// string, which has none. `{{`/`}}` collapse to one brace.
fn unescape(fragment: &str, is_raw: bool) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut chars = fragment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !is_raw => match chars.next() {
                Some(q @ ('"' | '\'' | '\\')) => out.push(q),
                Some('u') => {
                    for n in chars.by_ref() {
                        if n == '}' {
                            break;
                        }
                    }
                    out.push(' ');
                }
                Some('x') => {
                    chars.next();
                    chars.next();
                    out.push(' ');
                }
                _ => out.push(' '),
            },
            '{' | '}' if chars.peek() == Some(&c) => {
                chars.next();
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod error_traversal_regressions {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn tree(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn an_error_beyond_the_depth_budget_falls_back_to_the_root() {
        let depth = MAX_TRAVERSAL_DEPTH as usize + 64;
        let source = format!(
            "fn nested() {{ let _ = {}1 +{}; }}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let tree = tree(&source);
        let root = tree.root_node();
        assert!(root.has_error(), "fixture must contain a syntax error");
        assert!(
            first_error(root).is_none(),
            "a deep missing expression must not bypass the traversal budget"
        );
        let file = SourceFile {
            relative_path: "nested.rs".into(),
            contents: source,
        };
        assert!(matches!(
            RustParser.parse(&file),
            Err(ParseError::Syntax { line: 1, .. })
        ));
    }

    #[test]
    fn skipping_a_deep_subtree_still_finds_a_later_shallow_error() {
        let depth = MAX_TRAVERSAL_DEPTH as usize + 64;
        let source = format!(
            "fn nested() {{ let _ = {}1{};\nlet _ = ; }}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let tree = tree(&source);
        let error = first_error(tree.root_node()).expect("the shallow error remains reachable");
        assert_eq!(error.start_position().row, 1);
    }

    #[test]
    fn deep_valid_input_has_no_error_node() {
        let depth = MAX_TRAVERSAL_DEPTH as usize + 64;
        let source = format!(
            "fn nested() {{ let _ = {}1{}; }}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let tree = tree(&source);
        assert!(!tree.root_node().has_error());
        assert!(first_error(tree.root_node()).is_none());
    }
}
