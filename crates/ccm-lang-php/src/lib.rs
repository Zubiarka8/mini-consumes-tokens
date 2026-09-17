//! `LanguageParser` implementation for PHP, via `tree-sitter-php`.
//!
//! Node/field names below (`class_declaration`, `scoped_call_expression`,
//! `property_promotion_parameter`, etc.) come from inspecting
//! `tree-sitter-php` 0.24's actual parse trees and `node-types.json`
//! (`tree_sitter::Node::to_sexp()`), not from grammar docs — same
//! methodology used for `ccm-lang-bash`/`ccm-lang-powershell`.
//!
//! Deliberate scope and mapping decisions (AST-only, no type resolution —
//! consistent with every other language plugin in this workspace):
//! - `trait_declaration` reuses `SymbolKind::Trait` (the only other user is
//!   `ccm-lang-rust`). PHP traits are a mixin/code-inclusion mechanism, not
//!   an interface-like contract as in Rust — same tag, different semantics.
//!   Documented as a known limitation, not modeled as a new `SymbolKind`.
//! - `use TraitName;` inside a class/enum body (trait composition) is
//!   recorded as `RelationKind::Implements` on the enclosing type — the
//!   closest existing relation, though not a perfect fit (it doesn't
//!   require a contract, it inlines concrete code). No `RelationKind::Uses`
//!   exists in `ccm-core`; adding one is a `ccm-core`-level decision, out of
//!   scope for this crate. A trait-use adaptation block (`{ A::foo
//!   insteadof B; ... }`) is a distinct `use_list` child and is skipped —
//!   only the direct `name`/`qualified_name`/`relative_name` children of
//!   `use_declaration` are trait names.
//! - `const` declarations (top-level, or inside a class/interface/trait/
//!   enum) become `SymbolKind::Constant`, matching `ccm-lang-rust`'s
//!   top-level-const convention rather than `Field` (a constant isn't a
//!   mutable per-instance value).
//! - `namespace ...;`/`namespace ... { }` does not create an extra `Module`
//!   symbol — the file-derived module (matching `ccm-lang-java`/
//!   `ccm-lang-python`/`ccm-lang-rust`) is the only one. Namespaced names in
//!   `use`, `extends`/`implements`, and call targets are recorded by their
//!   last segment only, same convention as `ccm-lang-java`'s
//!   `last_identifier` for imports.
//! - `extends`/`implements` are unambiguous in this grammar (`base_clause`
//!   vs `class_interface_clause` are distinct node kinds, unlike C#'s
//!   unified `base_list`) — no positional heuristic needed.
//! - Three distinct call-expression node kinds all become `Calls`, with no
//!   attempt to resolve the receiver: `function_call_expression` (`foo()`),
//!   `member_call_expression` (`$obj->foo()`), and `scoped_call_expression`
//!   (`self::foo()`, `parent::foo()`, `static::foo()`, `Class::foo()` — the
//!   `scope` is ignored, only the `name` field is recorded).
//! - Constructor property promotion (`function __construct(private string
//!   $x) {}`) emits a `Field` symbol parented to the class, in addition to
//!   being an ordinary constructor parameter — easy to silently miss since
//!   it's parameter syntax, not a separate declaration node.
//! - An anonymous function or arrow function assigned to a variable
//!   (`$cb = function () {}` / `$cb = fn() => ...`) is named after that
//!   variable and indexed as a `Function`, matching `ccm-lang-js-ts`'s
//!   handling of the same pattern — otherwise every PHP callback (the
//!   idiomatic way to use `array_map`, etc.) would be invisible. A bare
//!   `$x = <non-callable>;` is not indexed, matching `ccm-lang-python`'s
//!   choice not to track plain module-level variables.
//! - `require`/`require_once`/`include`/`include_once` become `Imports`
//!   only when their argument is a literal string; a computed path
//!   (`require __DIR__ . '/x.php';`) has nothing static to record and is
//!   skipped, same precedent as `ccm-lang-bash`'s `source`.
//! - `define('NAME', value)` is recorded as an ordinary `Calls` relation to
//!   `define` — it is syntactically a function call, not a declaration, and
//!   no other parser in this workspace synthesizes a symbol from a call's
//!   arguments.

use ccm_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind,
    SourceFile, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct PhpParser;

impl LanguageParser for PhpParser {
    fn language_id(&self) -> &'static str {
        "php"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["php"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_php::LANGUAGE_PHP` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("tree-sitter-php grammar is statically valid");

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
        .trim_end_matches(".php")
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

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|n| n.kind() == kind)
}

/// The bare identifier inside a `variable_name` node (its one required
/// `name` child) — the grammar already excludes the leading `$`.
fn variable_text<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    if node.kind() != "variable_name" {
        return None;
    }
    node.named_child(0).map(|n| text(n, source))
}

/// Last segment of a `name` / `qualified_name` / `relative_name`, i.e. the
/// unqualified identifier — `\App\Models\User` and `User` both resolve to
/// `User`, matching `ccm-lang-java`'s `last_identifier` convention so a
/// same-named symbol elsewhere in the index can still match by name.
fn last_segment(node: Node) -> Option<Node> {
    match node.kind() {
        "name" => Some(node),
        "qualified_name" | "relative_name" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            children.into_iter().next_back().and_then(last_segment)
        }
        _ => None,
    }
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

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location, parent: Option<String>) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord { id, name, kind, location, parent });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    /// `owner` is the innermost enclosing function/method/module (calls,
    /// imports, and trait-use relations attach to it); `type_name` is the
    /// innermost enclosing class/interface/trait/enum name, used as
    /// `parent` for members declared directly inside it. Unlike
    /// `ccm-lang-java`, a class/interface/trait/enum body is visited with
    /// `owner` reset to that type's own symbol id (not the outer owner) so
    /// a body-level `use_declaration` (trait composition) attaches its
    /// relation to the type itself, not to a stale enclosing scope.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    /// `depth` bounds native stack usage against adversarially deep/nested
    /// input (see `MAX_TRAVERSAL_DEPTH`) — every recursive call below passes
    /// `depth + 1`, and this early-return prunes the subtree instead of
    /// recursing further once the ceiling is hit.
    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "class_declaration" | "interface_declaration" | "trait_declaration" => {
                let kind = match node.kind() {
                    "class_declaration" => SymbolKind::Class,
                    "interface_declaration" => SymbolKind::Interface,
                    _ => SymbolKind::Trait,
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), kind, location(node), type_name.map(str::to_string));

                if let Some(base_clause) = find_child(node, "base_clause") {
                    for base in named_children(base_clause) {
                        if let Some(seg) = last_segment(base) {
                            self.push_relation(id, RelationKind::Extends, text(seg, self.source).to_string(), location(seg));
                        }
                    }
                }
                if let Some(interfaces) = find_child(node, "class_interface_clause") {
                    for iface in named_children(interfaces) {
                        if let Some(seg) = last_segment(iface) {
                            self.push_relation(id, RelationKind::Implements, text(seg, self.source).to_string(), location(seg));
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
            }
            "enum_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), SymbolKind::Enum, location(node), type_name.map(str::to_string));
                if let Some(interfaces) = find_child(node, "class_interface_clause") {
                    for iface in named_children(interfaces) {
                        if let Some(seg) = last_segment(iface) {
                            self.push_relation(id, RelationKind::Implements, text(seg, self.source).to_string(), location(seg));
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
            }
            "enum_case" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(name, SymbolKind::Field, location(node), type_name.map(str::to_string));
            }
            // Trait composition (`use A, B;`, optionally with an
            // `{ A::foo insteadof B; ... }` adaptation block). Only the
            // direct name-like children are trait names — the adaptation
            // block is a separate `use_list` child, skipped here.
            "use_declaration" => {
                for child in named_children(node) {
                    if let Some(seg) = last_segment(child) {
                        self.push_relation(owner, RelationKind::Implements, text(seg, self.source).to_string(), location(seg));
                    }
                }
            }
            "function_definition" | "method_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if node.kind() == "method_declaration" { SymbolKind::Method } else { SymbolKind::Function };
                let parent = if kind == SymbolKind::Method { type_name.map(str::to_string) } else { None };
                let id = self.push_symbol(name, kind, location(node), parent);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, type_name, depth + 1);
                }
            }
            // Constructor property promotion: a parameter with a visibility
            // modifier is also, implicitly, a property declaration on the
            // enclosing class. `type_name` here is passed down unchanged
            // from the method's own `visit_children(params, id, type_name)`
            // call, so it's still the class, not the constructor.
            "property_promotion_parameter" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if let Some(var) = variable_text(name_node, self.source) {
                        self.push_symbol(var.to_string(), SymbolKind::Field, location(node), type_name.map(str::to_string));
                    }
                }
            }
            "property_declaration" => {
                for element in named_children(node) {
                    if element.kind() != "property_element" {
                        continue;
                    }
                    if let Some(name_node) = element.child_by_field_name("name") {
                        if let Some(var) = variable_text(name_node, self.source) {
                            self.push_symbol(var.to_string(), SymbolKind::Field, location(element), type_name.map(str::to_string));
                        }
                    }
                }
            }
            "const_declaration" => {
                for element in named_children(node) {
                    if element.kind() != "const_element" {
                        continue;
                    }
                    if let Some(name_node) = find_child(element, "name") {
                        self.push_symbol(text(name_node, self.source).to_string(), SymbolKind::Constant, location(element), type_name.map(str::to_string));
                    }
                }
            }
            "namespace_use_declaration" => {
                if let Some(group) = node.child_by_field_name("body") {
                    for clause in named_children(group) {
                        if clause.kind() != "namespace_use_clause" {
                            continue;
                        }
                        self.push_namespace_use_clause(owner, clause);
                    }
                } else if let Some(clause) = find_child(node, "namespace_use_clause") {
                    self.push_namespace_use_clause(owner, clause);
                }
            }
            "require_expression" | "require_once_expression" | "include_expression" | "include_once_expression" => {
                if let Some(target) = node.named_child(0) {
                    if let Some(path) = literal_string_text(target, self.source) {
                        self.push_relation(owner, RelationKind::Imports, path, location(node));
                    }
                    self.visit(target, owner, type_name, depth + 1);
                }
            }
            "function_call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(seg) = last_segment(function) {
                        self.push_relation(owner, RelationKind::Calls, text(seg, self.source).to_string(), location(seg));
                    }
                    self.visit(function, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "member_call_expression" | "scoped_call_expression" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.kind() == "name" {
                        // location(name_node), not location(node): a chained
                        // call (`$a->f($x)->f($y)`) has its outer and inner
                        // call both start at `$a`, which would make two
                        // same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
                    }
                }
                if let Some(receiver) = node.child_by_field_name("object").or_else(|| node.child_by_field_name("scope")) {
                    self.visit(receiver, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            // `$cb = function () {}` / `$cb = fn() => ...`: name the
            // closure after the variable it's bound to, same reasoning as
            // `ccm-lang-js-ts`'s handling of the same pattern — otherwise
            // every PHP callback would be invisible to the index.
            "assignment_expression" => {
                let left = node.child_by_field_name("left");
                let right = node.child_by_field_name("right");
                match (left, right) {
                    (Some(l), Some(r)) if matches!(r.kind(), "anonymous_function" | "arrow_function") => {
                        if let Some(var) = variable_text(l, self.source) {
                            let id = self.push_symbol(var.to_string(), SymbolKind::Function, location(r), type_name.map(str::to_string));
                            if let Some(params) = r.child_by_field_name("parameters") {
                                self.visit_children(params, id, type_name, depth + 1);
                            }
                            if let Some(body) = r.child_by_field_name("body") {
                                self.visit(body, id, type_name, depth + 1);
                            }
                        } else {
                            self.visit(r, owner, type_name, depth + 1);
                        }
                    }
                    _ => self.visit_children(node, owner, type_name, depth + 1),
                }
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    fn push_namespace_use_clause(&mut self, owner: SymbolId, clause: Node) {
        // Alias, when present, is recorded as the relation target instead of
        // the original name — same convention as `ccm-lang-python`'s
        // `aliased_import` handling (the alias is the identifier the rest of
        // this file actually uses).
        if let Some(alias) = clause.child_by_field_name("alias") {
            self.push_relation(owner, RelationKind::Imports, text(alias, self.source).to_string(), location(clause));
            return;
        }
        for child in named_children(clause) {
            if let Some(seg) = last_segment(child) {
                self.push_relation(owner, RelationKind::Imports, text(seg, self.source).to_string(), location(clause));
                return;
            }
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

fn named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Literal text of a plain `string` node (quotes trimmed), or `None` for
/// anything computed (concatenation, interpolation, a variable, ...) — only
/// a literal has a static path worth recording as an `Imports` target.
fn literal_string_text(node: Node, source: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let trimmed = text(node, source).trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
