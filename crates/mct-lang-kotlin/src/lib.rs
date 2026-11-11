//! `LanguageParser` implementation for Kotlin, via `tree-sitter-kotlin-ng`.
//!
//! Node kinds below were confirmed against the real compiled grammar (a
//! throwaway `to_sexp`/full-child dump, same method documented in
//! `checklist.md` for PHP/Bash/PowerShell), not assumed from grammar docs.
//! `class_declaration` covers both `class` and `interface` — the grammar
//! distinguishes them only by an anonymous `interface` token, never a named
//! field, so `is_interface` below checks the raw child list. `object`
//! declarations get their own `object_declaration` node; this parser reuses
//! `SymbolKind::Class` for them (Kotlin's singleton class), the same kind of
//! documented reuse as PHP's `SymbolKind::Trait`. None of
//! `call_expression`/`navigation_expression`/`property_declaration`/
//! `class_parameter`'s children are field-tagged in this grammar, so they're
//! read positionally/by-kind rather than via `child_by_field_name`.
//!
//! Known limits: infix and operator calls (`a percentOf b`, `a + b`) are no
//! `Calls` relation; inside a function body nothing is a member, so the
//! functions of a local class or an `object : T { … }` expression are
//! top-level `Function`s.

use std::borrow::Cow;

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_child, first_error, location, Supertype};
use tree_sitter::{Node, ParseOptions, ParseState, Parser};

/// Parse work budget, in tree-sitter progress-callback calls (one per 100
/// parser operations): this floor plus one call per this many input bytes.
const MIN_PARSE_PROGRESS_CHECKS: usize = 1_000;
const BYTES_PER_PARSE_PROGRESS_CHECK: usize = 4;

pub struct KotlinParser;

impl LanguageParser for KotlinParser {
    fn language_id(&self) -> &'static str {
        "kotlin"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_kotlin_ng::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .expect("tree-sitter-kotlin-ng grammar is statically valid");

        // tree-sitter's error recovery on some malformed Kotlin grows without
        // useful bound (a 205-byte fuzz input took over 1 GB and a minute),
        // so the parse gets a work budget proportional to the input. The
        // progress callback runs once per 100 parser operations; real Kotlin
        // (kotlinx.coroutines, okhttp, this repo's corpus: 1,710 files)
        // needs at most 0.02 calls per byte, far under this budget. Returning
        // `true` cancels the parse, which then yields no tree.
        // The pinned grammar's external annotation scanner loops over
        // non-whitespace without checking EOF. Its loop also bypasses the
        // progress callback. A final LF gives it a terminator, including for
        // malformed annotations; keep the original source for all locations
        // and extracted text. Files already ending in LF need no allocation.
        let parse_source = if file.contents.ends_with('\n') {
            Cow::Borrowed(file.contents.as_str())
        } else {
            Cow::Owned(format!("{}\n", file.contents))
        };
        let source = parse_source.as_bytes();
        let budget = MIN_PARSE_PROGRESS_CHECKS + source.len() / BYTES_PER_PARSE_PROGRESS_CHECK;
        let mut checks = 0usize;
        let mut over_budget = |_: &ParseState| {
            checks += 1;
            checks > budget
        };
        let tree = parser
            .parse_with_options(
                &mut |offset, _| source.get(offset..).unwrap_or_default(),
                None,
                Some(ParseOptions::new().progress_callback(&mut over_budget)),
            )
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree (parse work budget exceeded)"
                    .to_string(),
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
        let module_id = walker.push_symbol(
            module_name,
            SymbolKind::Module,
            module_location(&file.contents),
            None,
        );

        // `package foo.bar` — one Module-kind symbol per occurrence, not
        // deduplicated across files, same convention as Go's package clause
        // and C#'s namespace_declaration.
        if let Some(package_header) = find_child(root, "package_header") {
            if let Some(qid) = package_header.named_child(0) {
                // Spans the file, like Go's package clause: everything in it
                // belongs to the package.
                walker.push_symbol(
                    text(qid, &file.contents).to_string(),
                    SymbolKind::Module,
                    module_location(&file.contents),
                    None,
                );
            }
        }

        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    mct_tree_sitter::module_name(relative_path, &[".kts", ".kt"])
}

/// Span the original file, excluding any synthetic LF supplied to the
/// scanner. Empty input still has a file-level module at line one.
fn module_location(source: &str) -> Location {
    Location {
        line: 1,
        column: 1,
        byte_len: source.len() as u32,
        end_line: Some(source.lines().count().max(1) as u32),
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// `class_declaration` covers `class`/`data class`/`interface` alike; the
/// grammar's only signal is the anonymous `interface` token (no field, no
/// dedicated node kind), so it's found by scanning all children (not just
/// named ones — the token itself is unnamed).
/// The functions of `kotlin.Any` a subclass can override.
const ANY_METHODS: [&str; 3] = ["equals", "hashCode", "toString"];

/// The direct supertypes a class, object or object expression names in
/// its delegation specifiers: `Base()` (a constructor call: the
/// superclass), `Shape` or `Shape by impl`. One that names no superclass
/// also has the root, external: `Any`, or `Enum` for an enum class.
fn direct_supertypes(node: Node, source: &str) -> Vec<Supertype> {
    let mut named = Vec::new();
    let mut has_superclass = false;
    if let Some(delegations) = find_child(node, "delegation_specifiers") {
        let mut cursor = delegations.walk();
        for spec in delegations.named_children(&mut cursor) {
            let mut parts = spec.walk();
            let target = spec
                .named_children(&mut parts)
                .find(|part| part.kind() != "annotation");
            let Some(target) = target else {
                continue;
            };
            let user_type = match target.kind() {
                "constructor_invocation" => {
                    has_superclass = true;
                    target.named_child(0)
                }
                "explicit_delegation" => target.named_child(0),
                "user_type" => Some(target),
                _ => None,
            };
            if let Some(type_id) = user_type
                .filter(|t| t.kind() == "user_type")
                .and_then(|t| t.named_child(0))
            {
                named.push(Supertype {
                    name: text(type_id, source).to_string(),
                    external: false,
                });
            }
        }
    }
    if !has_superclass {
        let root = if find_child(node, "enum_class_body").is_some() {
            "Enum"
        } else {
            "Any"
        };
        named.push(Supertype {
            name: root.to_string(),
            external: true,
        });
    }
    named
}

/// Whether `declaration`'s modifiers include `override`.
fn has_override_modifier(declaration: Node, source: &str) -> bool {
    let Some(modifiers) = find_child(declaration, "modifiers") else {
        return false;
    };
    let mut cursor = modifiers.walk();
    let found = modifiers
        .children(&mut cursor)
        .any(|m| m.kind() == "member_modifier" && text(m, source) == "override");
    found
}

fn is_interface(class_declaration: Node) -> bool {
    let mut cursor = class_declaration.walk();
    let children: Vec<Node> = class_declaration.children(&mut cursor).collect();
    children.into_iter().any(|c| c.kind() == "interface")
}

/// The last `identifier` child of a `navigation_expression`
/// (`receiver.member` → `member`), i.e. the callee name for a
/// `receiver.method(...)` call — same "outermost field wins" precedent as
/// Java's `method_invocation`/Go's `selector_expression`.
fn last_identifier_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let idents: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "identifier")
        .collect();
    idents.last().copied()
}

/// The name being invoked by a `call_expression`: its first child is the
/// callee expression (no field name in this grammar), either a bare
/// `identifier` (`foo()`) or a `navigation_expression` (`a.b.foo()`, where
/// the last identifier segment is the call name).
fn callee_identifier(call_expression: Node) -> Option<Node> {
    let callee = call_expression.child(0)?;
    match callee.kind() {
        "identifier" => Some(callee),
        "navigation_expression" => last_identifier_child(callee),
        _ => None,
    }
}

/// A `function_declaration`'s extension receiver type (`fun String.shout()`
/// → `"String"`), if any: the grammar puts a bare `user_type` positionally
/// before the `name` field only for extension functions — generics
/// (`type_parameters`) and modifiers use different node kinds, so this can't
/// collide with them.
fn extension_receiver(function_declaration: Node, source: &str) -> Option<String> {
    let name = function_declaration.child_by_field_name("name")?;
    let mut cursor = function_declaration.walk();
    for child in function_declaration.children(&mut cursor) {
        if child.start_byte() >= name.start_byte() {
            break;
        }
        if child.kind() == "user_type" {
            // `Collection<Order>` → `Collection`: the type the function
            // extends, as other symbols name it.
            let name = text(child, source);
            return Some(name.split('<').next().unwrap_or(name).trim().to_string());
        }
    }
    None
}

/// The parameter name inside a primary-constructor `class_parameter`
/// (`val name: String`, `private val items: ...`): the first plain
/// `identifier` child — modifiers/`val`/`var` are distinct node kinds, so
/// this can't pick up the wrong token.
fn class_parameter_name(class_parameter: Node) -> Option<Node> {
    let mut cursor = class_parameter.walk();
    let children: Vec<Node> = class_parameter.children(&mut cursor).collect();
    children.into_iter().find(|c| c.kind() == "identifier")
}

/// Whether a `class_parameter` is promoted to a property (`val`/`var`
/// before the name) versus a plain constructor-only parameter.
fn is_promoted_property(class_parameter: Node) -> bool {
    let mut cursor = class_parameter.walk();
    let children: Vec<Node> = class_parameter.children(&mut cursor).collect();
    children
        .into_iter()
        .any(|c| c.kind() == "val" || c.kind() == "var")
}

mod details;
mod receivers;

#[derive(Default)]
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
    /// Declared types of the enclosing functions' locals (see `receivers`).
    locals: receivers::Types,
    /// What each enclosing class or object body declares, innermost last.
    classes: Vec<receivers::ClassScope>,
    /// The class `this` is, an index into `classes`; `None` inside a
    /// lambda, whose `this` may be its receiver (`apply { … }`).
    this_class: Option<usize>,
    /// Declaration details of the symbols recorded so far (ADR-034).
    details: Vec<mct_core::SymbolDetail>,
    /// Direct supertypes of each enclosing class, object or enum entry
    /// body, innermost last: what an `override fun` there can override (D8).
    supertypes: Vec<Vec<Supertype>>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }

    fn push_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        location: Location,
        parent: Option<String>,
    ) -> SymbolId {
        mct_tree_sitter::push_symbol(
            &mut self.symbols,
            &mut self.next_id,
            name,
            kind,
            location,
            parent,
        )
    }

    fn push_relation(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        to_name: String,
        loc: Location,
    ) {
        mct_tree_sitter::push_relation(&mut self.relations, from, kind, to_name, loc);
    }

    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    /// `owner` is the innermost enclosing function/method (calls attach to
    /// it) or the file-level module for top-level statements; `type_name` is
    /// the innermost enclosing class/interface/object, used as `parent` for
    /// members declared directly inside it.
    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "class_declaration" | "object_declaration" => {
                let kind = if node.kind() == "class_declaration" && is_interface(node) {
                    SymbolKind::Interface
                } else {
                    // `object_declaration` reuses Class — see module doc.
                    SymbolKind::Class
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    kind,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.details
                    .push(details::type_detail(node, id, self.source));

                if let Some(delegations) = find_child(node, "delegation_specifiers") {
                    let mut cursor = delegations.walk();
                    for spec in delegations.named_children(&mut cursor) {
                        if spec.kind() != "delegation_specifier" {
                            continue;
                        }
                        if let Some(target) = spec.named_child(0) {
                            match target.kind() {
                                // `Shape(4)` — a constructor call, so `Shape` is the superclass.
                                "constructor_invocation" => {
                                    if let Some(user_type) = target.named_child(0) {
                                        if let Some(type_id) = user_type.named_child(0) {
                                            self.push_relation(
                                                id,
                                                RelationKind::Extends,
                                                text(type_id, self.source).to_string(),
                                                location(type_id),
                                            );
                                        }
                                    }
                                }
                                // A bare type name, no call — an interface.
                                "user_type" => {
                                    if let Some(type_id) = target.named_child(0) {
                                        self.push_relation(
                                            id,
                                            RelationKind::Implements,
                                            text(type_id, self.source).to_string(),
                                            location(type_id),
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                self.supertypes.push(direct_supertypes(node, self.source));
                self.classes
                    .push(receivers::class_scope(&name, node, self.source));
                let outer_this = self.this_class.replace(self.classes.len() - 1);
                let outer_locals = std::mem::take(&mut self.locals);
                if let Some(primary_ctor) = find_child(node, "primary_constructor") {
                    self.visit_children(primary_ctor, owner, Some(&name), depth + 1);
                }
                if let Some(body) =
                    find_child(node, "class_body").or_else(|| find_child(node, "enum_class_body"))
                {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
                self.locals = outer_locals;
                self.this_class = outer_this;
                self.classes.pop();
                self.supertypes.pop();
            }
            // `object : Runnable { … }`: an anonymous class whose direct
            // supertypes are its delegation specifiers.
            "object_literal" => {
                self.supertypes.push(direct_supertypes(node, self.source));
                self.visit_children(node, owner, type_name, depth + 1);
                self.supertypes.pop();
            }
            // `NEW { override fun next() = … }`: a field of the enum; its
            // body overrides members of the enum, so they keep its name.
            "enum_entry" => {
                if let Some(name_node) = find_child(node, "identifier") {
                    self.push_symbol(
                        text(name_node, self.source).to_string(),
                        SymbolKind::Field,
                        location(node),
                        type_name.map(str::to_string),
                    );
                }
                // The entry's body is an anonymous subclass of the enum.
                let enum_type = type_name.map(|name| Supertype {
                    name: name.to_string(),
                    external: false,
                });
                self.supertypes.push(enum_type.into_iter().collect());
                self.visit_children(node, owner, type_name, depth + 1);
                self.supertypes.pop();
            }
            "class_parameter" => {
                if is_promoted_property(node) {
                    if let Some(name_node) = class_parameter_name(node) {
                        let id = self.push_symbol(
                            text(name_node, self.source).to_string(),
                            SymbolKind::Field,
                            location(node),
                            type_name.map(str::to_string),
                        );
                        self.details
                            .push(details::property_detail(node, id, self.source));
                    }
                }
                // Default-value expressions (`= mutableListOf()`) may still
                // contain calls worth indexing, promoted property or not.
                self.visit_children(node, owner, type_name, depth + 1);
            }
            "function_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let receiver = extension_receiver(node, self.source);
                let parent = receiver.or_else(|| type_name.map(str::to_string));
                // A member or an extension is a method of its type; a
                // top-level or local `fun` is a function.
                let kind = if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let id = self.push_symbol(name, kind, location(node), parent);
                self.details
                    .push(details::function_detail(node, id, self.source));
                if has_override_modifier(node, self.source) {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let supertypes = self.supertypes.last().cloned().unwrap_or_default();
                        mct_tree_sitter::push_overrides(
                            &mut self.relations,
                            &mut self.relation_targets,
                            id,
                            text(name_node, self.source),
                            location(name_node),
                            &supertypes,
                            &ANY_METHODS,
                        );
                    }
                }
                let mut locals = self.locals.clone();
                let mut cursor = node.walk();
                for part in node.named_children(&mut cursor) {
                    receivers::locals(part, self.source, &mut locals, depth + 1);
                }
                let outer_locals = std::mem::replace(&mut self.locals, locals);
                // Inside the body no declaration is a member of the
                // enclosing type: a local `val` is no field, a local `fun`
                // no method.
                if let Some(params) = find_child(node, "function_value_parameters") {
                    self.visit_children(params, id, None, depth + 1);
                }
                if let Some(body) = find_child(node, "function_body") {
                    self.visit_children(body, id, None, depth + 1);
                }
                self.locals = outer_locals;
            }
            "lambda_literal" | "anonymous_function" => {
                let outer_this = self.this_class.take();
                self.visit_children(node, owner, type_name, depth + 1);
                self.this_class = outer_this;
            }
            "property_declaration" => {
                if type_name.is_some() {
                    if let Some(var_decl) = find_child(node, "variable_declaration") {
                        if let Some(name_node) = var_decl.named_child(0) {
                            let id = self.push_symbol(
                                text(name_node, self.source).to_string(),
                                SymbolKind::Field,
                                location(node),
                                type_name.map(str::to_string),
                            );
                            self.details
                                .push(details::property_detail(node, id, self.source));
                        }
                    }
                }
                // Always recurse: initializer expressions (`val x = Foo()`)
                // may contain calls even for locals we don't index as Field.
                self.visit_children(node, owner, type_name, depth + 1);
            }
            "import" => {
                let mut cursor = node.walk();
                let named: Vec<Node> = node.named_children(&mut cursor).collect();
                if let Some(path) = named.first() {
                    self.push_type_import(owner, text(*path, self.source), location(node));
                }
                let imported = if named.len() >= 2 {
                    // `import a.b.Foo as Bar` — the alias is what call sites use.
                    Some(named[1])
                } else {
                    named.first().and_then(|qid| {
                        let mut c = qid.walk();
                        let idents: Vec<Node> = qid
                            .named_children(&mut c)
                            .filter(|n| n.kind() == "identifier")
                            .collect();
                        idents.last().copied()
                    })
                };
                if let Some(name_node) = imported {
                    self.push_relation(
                        owner,
                        RelationKind::Imports,
                        text(name_node, self.source).to_string(),
                        location(node),
                    );
                }
            }
            "call_expression" => {
                if let Some(name_node) = callee_identifier(node) {
                    if let Some(receiver) = node
                        .child(0)
                        .filter(|c| c.kind() == "navigation_expression")
                        .and_then(|c| c.named_child(0))
                    {
                        self.push_receiver(receiver, text(name_node, self.source));
                    }
                    // location(name_node), not location(node): a chained call
                    // (`a.f(x).f(y)`) would otherwise collide two distinct
                    // calls into one indistinguishable row.
                    self.push_relation(
                        owner,
                        RelationKind::Calls,
                        text(name_node, self.source).to_string(),
                        location(name_node),
                    );
                }
                self.visit_children(node, owner, type_name, depth + 1);
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    /// Evidence for `receiver.method(…)` about to be pushed: the
    /// receiver's declared type; a capitalized name that is no local or
    /// property (an `object`, a companion, a type); or a member call when
    /// the receiver is a local, `it` or an expression of unproven type.
    /// `this.m()` is qualified by the enclosing class only when its body
    /// declares `m` (otherwise `m` may be inherited), and a lowercase name
    /// that is no local or property is left as it was.
    fn push_receiver(&mut self, receiver: Node, method: &str) {
        let this_class = self.this_class.and_then(|i| self.classes.get(i));
        let typed = |ty: Option<String>| {
            let member = ty.is_none();
            (ty, member)
        };
        let (qualifier, member) = match receiver.kind() {
            "this_expression" => match this_class {
                Some(class) if class.methods.contains(method) => (Some(class.name.clone()), false),
                _ => return,
            },
            "super_expression" => return,
            "identifier" => {
                let name = text(receiver, self.source);
                match self
                    .locals
                    .get(name)
                    .or_else(|| this_class.and_then(|c| c.fields.get(name)))
                {
                    Some(ty) => typed(ty.clone()),
                    None if name == "it" => (None, true),
                    None if name.starts_with(char::is_uppercase) => (Some(name.to_string()), false),
                    None => return,
                }
            }
            "navigation_expression" => {
                let on_this = receiver
                    .named_child(0)
                    .is_some_and(|o| o.kind() == "this_expression");
                let field = last_identifier_child(receiver).map(|f| text(f, self.source));
                match (on_this, field, this_class) {
                    (true, Some(field), Some(class)) => {
                        typed(class.fields.get(field).cloned().flatten())
                    }
                    _ => (None, true),
                }
            }
            "call_expression" => typed(receivers::constructed(receiver, self.source)),
            _ => (None, true),
        };
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            qualifier,
            member,
            ..Default::default()
        });
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            symbol_details: self.details,
            ..Default::default()
        }
    }

    /// `import a.b.Type [as T]` depends on the file declaring `Type`
    /// (Kotlin convention names it `Type.kt`): an `Imports` relation to that
    /// file's `Module` symbol, scoped to a path under the package segment
    /// `b`. A file that holds the type under another name is still reached
    /// through the plain name relation. Kotlin/JDK packages are external; a
    /// lowercase last segment (a top-level function) or a wildcard names no
    /// type and records nothing here.
    fn push_type_import(&mut self, owner: SymbolId, path: &str, loc: Location) {
        let segments: Vec<&str> = path
            .split('.')
            .map(|s| s.trim().trim_matches('`'))
            .collect();
        let Some(at) = segments
            .iter()
            .position(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))
        else {
            return;
        };
        let external = matches!(segments.first(), Some(&"kotlin" | &"java" | &"javax"));
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, segments[at].to_string(), loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module: at
                .checked_sub(1)
                .map(|i| segments[i].to_string())
                .filter(|m| !m.is_empty()),
            kind: Some(SymbolKind::Module),
            external,
            ..Default::default()
        });
    }
}
