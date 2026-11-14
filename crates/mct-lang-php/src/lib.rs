//! `LanguageParser` implementation for PHP, via `tree-sitter-php`.
//!
//! Node/field names below (`class_declaration`, `scoped_call_expression`,
//! `property_promotion_parameter`, etc.) come from inspecting
//! `tree-sitter-php` 0.24's actual parse trees and `node-types.json`
//! (`tree_sitter::Node::to_sexp()`), not from grammar docs — same
//! methodology used for `mct-lang-bash`/`mct-lang-powershell`.
//!
//! Deliberate scope and mapping decisions (AST-only, no type resolution —
//! consistent with every other language plugin in this workspace):
//! - `trait_declaration` reuses `SymbolKind::Trait` (the only other user is
//!   `mct-lang-rust`). PHP traits are a mixin/code-inclusion mechanism, not
//!   an interface-like contract as in Rust — same tag, different semantics.
//!   Documented as a known limitation, not modeled as a new `SymbolKind`.
//! - `use TraitName;` inside a class/enum body (trait composition) is
//!   recorded as `RelationKind::Implements` on the enclosing type — the
//!   closest existing relation, though not a perfect fit (it doesn't
//!   require a contract, it inlines concrete code). No `RelationKind::Uses`
//!   exists in `mct-core`; adding one is a `mct-core`-level decision, out of
//!   scope for this crate. A trait-use adaptation block (`{ A::foo
//!   insteadof B; ... }`) is a distinct `use_list` child and is skipped —
//!   only the direct `name`/`qualified_name`/`relative_name` children of
//!   `use_declaration` are trait names.
//! - `const` declarations (top-level, or inside a class/interface/trait/
//!   enum) become `SymbolKind::Constant`, matching `mct-lang-rust`'s
//!   top-level-const convention rather than `Field` (a constant isn't a
//!   mutable per-instance value).
//! - `namespace ...;`/`namespace ... { }` does not create an extra `Module`
//!   symbol — the file-derived module (matching `mct-lang-java`/
//!   `mct-lang-python`/`mct-lang-rust`) is the only one. The namespace is
//!   carried as evidence instead: the file's `Module` symbol is given the
//!   alias `\Ns\Sub` for every namespace the file declares (`\` for the
//!   global one) and the short name of every type it declares
//!   ([`ParsedFile::symbol_aliases`]), and a reference to a class sets
//!   `RelationTarget::target_module` to the namespace it resolves to under
//!   PHP's rules (`namespace`, `use`, `use ... as`, `\Fully\Qualified`,
//!   `namespace\Relative`; see `names.rs`). The index therefore resolves a
//!   class to the file that declares exactly that namespace, wherever
//!   Composer's PSR-4 map puts it, and two classes with the same short name
//!   in different namespaces are told apart. `to_name` stays the class's real
//!   short name, so an aliased import (`use A\B as C`) is recorded as `B`.
//! - `extends`/`implements` are unambiguous in this grammar (`base_clause`
//!   vs `class_interface_clause` are distinct node kinds, unlike C#'s
//!   unified `base_list`) — no positional heuristic needed.
//! - Three distinct call-expression node kinds all become `Calls`:
//!   `function_call_expression` (`foo()`), `member_call_expression`
//!   (`$obj->foo()`) and `scoped_call_expression` (`Class::foo()`). A static
//!   call is qualified by the class it names (`qualifier` plus its
//!   namespace); `self::`/`static::`/`$this->` by the enclosing class, `parent::`
//!   by its `extends`. A trait's `$this`, an anonymous class's `$this`, a
//!   `$class::foo()` and any other receiver have no static class: they are
//!   `member` calls (candidates kept, never resolved). A function is
//!   qualified only when imported (`use function`) or written with a
//!   namespace; a bare `foo()` may be the namespaced or the global function.
//! - A class named in a type position (parameter, return, property, `catch`),
//!   an `instanceof`, `X::class`, `X::CONST`, `X::$prop` or a static call's
//!   scope is a `References` from the enclosing function or class.
//!   `extends`, `implements`, trait `use`, `new` and attributes pin the kind
//!   of the target the syntax proves (class, interface, trait).
//! - Constructor property promotion (`function __construct(private string
//!   $x) {}`) emits a `Field` symbol parented to the class, in addition to
//!   being an ordinary constructor parameter — easy to silently miss since
//!   it's parameter syntax, not a separate declaration node.
//! - An anonymous function or arrow function assigned to a variable
//!   (`$cb = function () {}` / `$cb = fn() => ...`) is named after that
//!   variable and indexed as a top-level `Function` (no `parent`, even
//!   inside a method — it's a local, not a class member), matching `mct-lang-js-ts`'s
//!   handling of the same pattern — otherwise every PHP callback (the
//!   idiomatic way to use `array_map`, etc.) would be invisible. A bare
//!   `$x = <non-callable>;` is not indexed, matching `mct-lang-python`'s
//!   choice not to track plain module-level variables.
//! - `require`/`require_once`/`include`/`include_once` become `Imports`
//!   only when their argument is a literal string; a computed path
//!   (`require __DIR__ . '/x.php';`) has nothing static to record and is
//!   skipped, same precedent as `mct-lang-bash`'s `source`.
//! - `new Foo(...)` is a `Calls` relation to `Foo` (the class name, as in
//!   `mct-lang-csharp`); `new self`/`new static`/`new $class` have no static
//!   name and record nothing. `$a?->f()` is a call like `$a->f()`.
//! - `#[Attr(...)]` on a class, enum, method, property or parameter is a
//!   `References` relation from the decorated declaration (the enclosing
//!   function for a parameter) to `Attr`.
//! - An anonymous class (`new class (...) extends B implements I { ... }`)
//!   is an expression: its members are not indexed (they have no named
//!   owner), its bases are `References` from the enclosing function/module,
//!   and the calls in its methods are attributed to that enclosing owner.
//! - Laravel-specific extraction (routes, Blade, migrations) lives in
//!   `libraries/laravel/`; see its module docs for the supported scope. A
//!   `*.blade.php` file never goes through the PHP grammar.
//! - `define('NAME', value)` is recorded as an ordinary `Calls` relation to
//!   `define` — it is syntactically a function call, not a declaration, and
//!   no other parser in this workspace synthesizes a symbol from a call's
//!   arguments.

use std::collections::HashSet;

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolAlias, SymbolDetail, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
    MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_child, first_error, location, module_location, Supertype};
use tree_sitter::{Node, Parser};

use libraries::laravel;
use names::{namespace_alias, ImportKind, ResolvedName, Scope};

mod builtins;
mod details;
mod libraries;
mod literals;
mod names;
mod tokens;

pub use tokens::PhpTokenSource;

pub struct PhpParser;

impl LanguageParser for PhpParser {
    fn language_id(&self) -> &'static str {
        "php"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["php"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        // A Blade template is HTML with directives, not PHP: it has its own
        // extractor and cannot be a syntax error.
        if laravel::is_blade_template(&file.relative_path) {
            return Ok(laravel::blade::parse(file));
        }
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
        let module_id =
            walker.push_symbol(module_name, SymbolKind::Module, module_location(root), None);
        walker.module_id = module_id;
        collect_function_names(root, &file.contents, &mut walker.declared_functions, 0);
        walker.visit_children(root, module_id, None, 0);
        let mut parsed = walker.finish();
        parsed.literals = literals::collect(root, &file.contents);
        Ok(parsed)
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

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// The bare identifier inside a `variable_name` node (its one required
/// `name` child) — the grammar already excludes the leading `$`.
pub(crate) fn variable_text<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    if node.kind() != "variable_name" {
        return None;
    }
    node.named_child(0).map(|n| text(n, source))
}

/// Last segment of a `name` / `qualified_name` / `relative_name`, i.e. the
/// unqualified identifier — `\App\Models\User` and `User` both resolve to
/// `User`, matching `mct-lang-java`'s `last_identifier` convention so a
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
    relation_targets: Vec<RelationTarget>,
    details: Vec<SymbolDetail>,
    /// This file's repository-relative path, the base of relative includes.
    path: &'a str,
    next_id: SymbolId,
    /// The file's `Module` symbol, which carries the namespace and
    /// declared-type aliases the index resolves references through.
    module_id: SymbolId,
    /// Namespace and imports in effect at the node being visited.
    scope: Scope,
    /// Every namespace this file declares, in source order.
    namespaces: Vec<String>,
    /// Short names of the types this file declares.
    declared_types: Vec<String>,
    /// Lowercase names of the functions this file declares anywhere,
    /// including inside an `if (!function_exists(…))` polyfill guard.
    declared_functions: HashSet<String>,
    /// The class-like declarations enclosing the node being visited,
    /// innermost last.
    classes: Vec<ClassContext>,
    /// The controllers bound by the `Route::controller(...)->group(...)`
    /// chains around the node being visited, innermost last.
    route_controllers: Vec<ResolvedName>,
    /// Classes a framework may load by convention (see `libraries::laravel`).
    entry_points: Vec<mct_core::EntryPointHint>,
    /// The supertypes a method of each enclosing class-like declaration can
    /// override (D8), innermost last.
    supertypes: Vec<Vec<Supertype>>,
    /// Closures (`function () {}`, `fn () =>`) enclosing the node being
    /// visited that are not bound to a variable. Their body keeps the outer
    /// `owner`, but has its own variable scope.
    closure_depth: u32,
}

/// What `self::`, `static::`, `parent::` and `$this->` mean at a call site.
enum ClassContext {
    Named {
        name: String,
        namespace: String,
        parent: Option<ResolvedName>,
        /// A trait's `$this` is whichever class uses it.
        is_trait: bool,
    },
    /// An anonymous class has no name to qualify a call with.
    Anonymous,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
            details: Vec::new(),
            next_id: 0,
            module_id: 0,
            scope: Scope::default(),
            namespaces: Vec::new(),
            declared_types: Vec::new(),
            declared_functions: HashSet::new(),
            classes: Vec::new(),
            route_controllers: Vec::new(),
            entry_points: Vec::new(),
            supertypes: Vec::new(),
            closure_depth: 0,
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

    /// A relation together with the evidence the source gives about its
    /// target, when there is any.
    fn push_targeted(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        to_name: String,
        loc: Location,
        target: Option<RelationTarget>,
    ) {
        let relation = self.relations.len();
        self.push_relation(from, kind, to_name, loc);
        if self.relations.len() == relation {
            return;
        }
        if let Some(target) = target {
            self.relation_targets
                .push(RelationTarget { relation, ..target });
        }
    }

    /// A reference to the class-like type written at `name_node`
    /// (`User`, `Models\User`, `\App\User`), resolved through the namespace
    /// and imports in effect. `self`, `static`, `parent` and the built-in
    /// type names are not references to a declaration. `declared_as` is the
    /// kind the syntax proves the target has (a class after `extends`, an
    /// interface after `implements`); without it the file's own `Module`
    /// symbol, which shares the class's name, stays a candidate. Returns the
    /// resolved name when a relation was recorded.
    fn push_class_reference(
        &mut self,
        owner: SymbolId,
        kind: RelationKind,
        declared_as: Option<SymbolKind>,
        name_node: Node,
    ) -> Option<ResolvedName> {
        let seg = last_segment(name_node)?;
        let written = text(name_node, self.source).trim();
        if is_reserved_type_name(written) {
            return None;
        }
        let resolved = self.scope.resolve_class(written)?;
        let target = RelationTarget {
            kind: declared_as,
            target_module: Some(namespace_alias(&resolved.namespace)),
            ..Default::default()
        };
        self.push_targeted(
            owner,
            kind,
            resolved.name.clone(),
            location(seg),
            Some(target),
        );
        Some(resolved)
    }

    /// `extends` and `implements` of a class, interface or trait, resolved
    /// through the namespace in effect. Returns the class's parent, which a
    /// `parent::` call inside it is qualified with.
    fn visit_type_clauses(&mut self, node: Node, id: SymbolId) -> Option<ResolvedName> {
        // A class extends a class; an interface extends interfaces.
        let extended = if node.kind() == "interface_declaration" {
            SymbolKind::Interface
        } else {
            SymbolKind::Class
        };
        let mut parent = None;
        if let Some(base_clause) = find_child(node, "base_clause") {
            for base in named_children(base_clause) {
                if let Some(resolved) =
                    self.push_class_reference(id, RelationKind::Extends, Some(extended), base)
                {
                    parent.get_or_insert(resolved);
                }
            }
        }
        if let Some(interfaces) = find_child(node, "class_interface_clause") {
            for iface in named_children(interfaces) {
                self.push_class_reference(
                    id,
                    RelationKind::Implements,
                    Some(SymbolKind::Interface),
                    iface,
                );
            }
        }
        parent
    }

    /// D8: the parent class and the interfaces of a class-like declaration
    /// (an interface's: those it extends), resolved through the namespace
    /// in effect. PHP has no root class.
    fn override_supertypes(&self, node: Node) -> Vec<Supertype> {
        ["base_clause", "class_interface_clause"]
            .into_iter()
            .filter_map(|clause| find_child(node, clause))
            .flat_map(named_children)
            .filter(|name| last_segment(*name).is_some())
            .filter_map(|name| {
                let written = text(name, self.source).trim();
                if is_reserved_type_name(written) {
                    return None;
                }
                self.scope.resolve_class(written)
            })
            .map(|resolved| Supertype {
                name: resolved.name,
                external: false,
            })
            .collect()
    }

    /// Whether a method carries PHP 8.3's `#[\Override]`: an attribute
    /// whose name resolves to the global `Override` (attribute names
    /// resolve like class names; a bare `Override` in a namespace needs a
    /// `use Override;`).
    fn has_override_attribute(&self, method: Node) -> bool {
        let Some(attributes) = method.child_by_field_name("attributes") else {
            return false;
        };
        let mut found = false;
        let mut stack = vec![attributes];
        while let Some(node) = stack.pop() {
            if node.kind() == "attribute" {
                let name = named_children(node)
                    .into_iter()
                    .find(|c| last_segment(*c).is_some());
                found |= name
                    .and_then(|n| self.scope.resolve_class(text(n, self.source).trim()))
                    .is_some_and(|r| {
                        r.namespace.is_empty() && r.name.eq_ignore_ascii_case("Override")
                    });
                continue;
            }
            stack.extend(named_children(node));
        }
        found
    }

    fn declare_type(&mut self, name: &str) {
        if !name.is_empty() && !self.declared_types.iter().any(|t| t == name) {
            self.declared_types.push(name.to_string());
        }
    }

    fn note_namespace(&mut self, namespace: &str) {
        if !self.namespaces.iter().any(|n| n == namespace) {
            self.namespaces.push(namespace.to_string());
        }
    }

    /// The class `self::`/`static::` and `$this->` denote, as call evidence.
    /// A trait's `$this` is whichever class uses it and an anonymous class
    /// has no name, so neither can be qualified.
    fn current_class_target(&self) -> RelationTarget {
        match self.classes.last() {
            Some(ClassContext::Named {
                name,
                namespace,
                is_trait: false,
                ..
            }) => RelationTarget {
                qualifier: Some(name.clone()),
                target_module: Some(namespace_alias(namespace)),
                ..Default::default()
            },
            _ => RelationTarget {
                member: true,
                ..Default::default()
            },
        }
    }

    fn parent_class_target(&self) -> RelationTarget {
        match self.classes.last() {
            Some(ClassContext::Named {
                parent: Some(parent),
                ..
            }) => RelationTarget {
                qualifier: Some(parent.name.clone()),
                target_module: Some(namespace_alias(&parent.namespace)),
                ..Default::default()
            },
            _ => RelationTarget {
                member: true,
                ..Default::default()
            },
        }
    }

    /// Evidence for `Scope::method()`: the class named at the call site, or
    /// the enclosing/parent class for `self`/`static`/`parent`. A scope that
    /// is not a name (`$class::create()`) has no static class.
    fn scoped_call_target(&self, scope: Option<Node>) -> RelationTarget {
        let unknown = RelationTarget {
            member: true,
            ..Default::default()
        };
        let Some(scope) = scope else {
            return unknown;
        };
        let written = text(scope, self.source).trim();
        if scope.kind() == "relative_scope" || last_segment(scope).is_some() {
            if written.eq_ignore_ascii_case("self") || written.eq_ignore_ascii_case("static") {
                return self.current_class_target();
            }
            if written.eq_ignore_ascii_case("parent") {
                return self.parent_class_target();
            }
        }
        if last_segment(scope).is_none() {
            return unknown;
        }
        match self.scope.resolve_class(written) {
            Some(resolved) => RelationTarget {
                qualifier: Some(resolved.name),
                target_module: Some(namespace_alias(&resolved.namespace)),
                ..Default::default()
            },
            None => unknown,
        }
    }

    /// Evidence for `$receiver->method()`: only `$this` has a known class.
    fn member_call_target(&self, receiver: Option<Node>) -> RelationTarget {
        match receiver {
            Some(r) if r.kind() == "variable_name" && text(r, self.source) == "$this" => {
                self.current_class_target()
            }
            _ => RelationTarget {
                member: true,
                ..Default::default()
            },
        }
    }

    /// Evidence for `f()`: a function (a variable is called as `$f()`), and
    /// its namespace when the name is imported or qualified. A bare name in
    /// a namespace may be the namespaced function or the global one, which
    /// only a run can tell.
    ///
    /// A call that can only reach the global function of one of PHP's core
    /// functions (`\strlen()`, or `strlen()` in a file with no namespace) is
    /// external, unless this file declares that name itself (a polyfill).
    fn function_target(&self, function: Node) -> RelationTarget {
        let written = text(function, self.source);
        let resolved = self.scope.resolve_function(written);
        let global_name = match &resolved {
            Some(r) if r.namespace.is_empty() => Some(r.name.as_str()),
            Some(_) => None,
            None if self.scope.namespace().is_empty() => Some(written.trim()),
            None => None,
        };
        if global_name.is_some_and(|name| {
            builtins::is_core_function(name)
                && !self.declared_functions.contains(&name.to_ascii_lowercase())
        }) {
            return RelationTarget {
                external: true,
                ..Default::default()
            };
        }
        RelationTarget {
            kind: Some(SymbolKind::Function),
            target_module: resolved.map(|r| namespace_alias(&r.namespace)),
            ..Default::default()
        }
    }

    /// The variable in `$cb()` when this file already bound `$cb` to a
    /// closure (which the `assignment_expression` arm indexes as a
    /// top-level `function` named `cb`). Any other `$var()` may hold an
    /// arbitrary callable, so it names no symbol.
    fn bound_closure_name(&self, function: Node<'a>) -> Option<&'a str> {
        let var = variable_text(function, self.source)?;
        self.symbols
            .iter()
            .any(|s| s.name == var && s.kind == SymbolKind::Function && s.parent.is_none())
            .then_some(var)
    }

    /// `owner` is the innermost enclosing function/method/module (calls,
    /// imports, and trait-use relations attach to it); `type_name` is the
    /// innermost enclosing class/interface/trait/enum name, used as
    /// `parent` for members declared directly inside it. Unlike
    /// `mct-lang-java`, a class/interface/trait/enum body is visited with
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
            "namespace_definition" => {
                let namespace = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).trim().trim_matches('\\').to_string())
                    .unwrap_or_default();
                self.scope.enter_namespace(&namespace);
                self.note_namespace(&namespace);
                // A braced block ends the namespace; an unbraced one lasts
                // until the next `namespace` statement or the end of file.
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, type_name, depth + 1);
                    self.scope.enter_namespace("");
                }
            }
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
                let id = self.push_symbol(
                    name.clone(),
                    kind,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.details
                    .push(details::type_detail(node, id, self.source));
                self.declare_type(&name);
                if kind == SymbolKind::Class {
                    self.entry_points
                        .extend(libraries::laravel::listeners::hint(
                            self.path,
                            node,
                            self.source,
                            id,
                        ));
                }
                self.visit_attributes(node, id, depth);
                let parent = self.visit_type_clauses(node, id);
                self.classes.push(ClassContext::Named {
                    name: name.clone(),
                    namespace: self.scope.namespace().to_string(),
                    parent,
                    is_trait: kind == SymbolKind::Trait,
                });
                // A trait's `#[\Override]` is checked in the class that
                // uses it, so it has no supertype of its own.
                let supertypes = if kind == SymbolKind::Trait {
                    Vec::new()
                } else {
                    self.override_supertypes(node)
                };
                self.supertypes.push(supertypes);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
                self.supertypes.pop();
                self.classes.pop();
            }
            "enum_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Enum,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.details
                    .push(details::type_detail(node, id, self.source));
                self.declare_type(&name);
                self.visit_attributes(node, id, depth);
                self.visit_type_clauses(node, id);
                self.classes.push(ClassContext::Named {
                    name: name.clone(),
                    namespace: self.scope.namespace().to_string(),
                    parent: None,
                    is_trait: false,
                });
                self.supertypes.push(self.override_supertypes(node));
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
                self.supertypes.pop();
                self.classes.pop();
            }
            "enum_case" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(
                    name,
                    SymbolKind::Field,
                    location(node),
                    type_name.map(str::to_string),
                );
            }
            // Trait composition (`use A, B;`, optionally with an
            // `{ A::foo insteadof B; ... }` adaptation block). Only the
            // direct name-like children are trait names — the adaptation
            // block is a separate `use_list` child, skipped here.
            "use_declaration" => {
                for child in named_children(node) {
                    self.push_class_reference(
                        owner,
                        RelationKind::Implements,
                        Some(SymbolKind::Trait),
                        child,
                    );
                }
            }
            "function_definition" | "method_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if node.kind() == "method_declaration" {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let parent = if kind == SymbolKind::Method {
                    type_name.map(str::to_string)
                } else {
                    None
                };
                let id = self.push_symbol(name, kind, location(node), parent);
                self.details
                    .push(details::function_detail(node, id, self.source));
                if kind == SymbolKind::Method && self.has_override_attribute(node) {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let supertypes = self.supertypes.last().cloned().unwrap_or_default();
                        mct_tree_sitter::push_overrides(
                            &mut self.relations,
                            &mut self.relation_targets,
                            id,
                            text(name_node, self.source),
                            location(name_node),
                            &supertypes,
                            &[],
                        );
                    }
                }
                self.visit_attributes(node, id, depth);
                for field in ["parameters", "return_type", "body"] {
                    if let Some(part) = node.child_by_field_name(field) {
                        if field == "return_type" {
                            self.visit(part, id, type_name, depth + 1);
                        } else {
                            self.visit_children(part, id, type_name, depth + 1);
                        }
                    }
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
                        let id = self.push_symbol(
                            var.to_string(),
                            SymbolKind::Field,
                            location(node),
                            type_name.map(str::to_string),
                        );
                        self.details
                            .push(details::promoted_detail(node, id, self.source));
                    }
                }
                self.visit_attributes(node, owner, depth);
                if let Some(ty) = node.child_by_field_name("type") {
                    self.visit(ty, owner, type_name, depth + 1);
                }
                if let Some(default) = node.child_by_field_name("default_value") {
                    self.visit(default, owner, type_name, depth + 1);
                }
            }
            "property_declaration" => {
                self.visit_attributes(node, owner, depth);
                for child in named_children(node) {
                    if child.kind() == "attribute_list" {
                        continue;
                    }
                    if child.kind() != "property_element" {
                        // The declared type.
                        self.visit(child, owner, type_name, depth + 1);
                        continue;
                    }
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Some(var) = variable_text(name_node, self.source) {
                            let id = self.push_symbol(
                                var.to_string(),
                                SymbolKind::Field,
                                location(child),
                                type_name.map(str::to_string),
                            );
                            self.details.push(details::property_detail(
                                node,
                                child,
                                id,
                                self.source,
                            ));
                        }
                    }
                    for initializer in named_children(child).into_iter().skip(1) {
                        self.visit(initializer, owner, type_name, depth + 1);
                    }
                }
            }
            "const_declaration" => {
                for element in named_children(node) {
                    if element.kind() != "const_element" {
                        continue;
                    }
                    if let Some(name_node) = find_child(element, "name") {
                        self.push_symbol(
                            text(name_node, self.source).to_string(),
                            SymbolKind::Constant,
                            location(element),
                            type_name.map(str::to_string),
                        );
                    }
                    for value in named_children(element).into_iter().skip(1) {
                        self.visit(value, owner, type_name, depth + 1);
                    }
                }
            }
            "namespace_use_declaration" => self.visit_namespace_use(node, owner),
            "require_expression"
            | "require_once_expression"
            | "include_expression"
            | "include_once_expression" => {
                if let Some(target) = node.named_child(0) {
                    self.push_include_target(owner, target, location(node));
                    if let Some(path) = literal_string_text(target, self.source) {
                        self.push_relation(owner, RelationKind::Imports, path, location(node));
                    }
                    self.visit(target, owner, type_name, depth + 1);
                }
            }
            "function_call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "name" && text(function, self.source) == "view" {
                        if let Some(view) = node
                            .child_by_field_name("arguments")
                            .and_then(|a| a.named_child(0))
                            .and_then(|a| a.named_child(0))
                        {
                            self.push_view_reference(owner, view, location(node));
                        }
                    }
                    if let Some(seg) = last_segment(function) {
                        let target = Some(self.function_target(function));
                        self.push_targeted(
                            owner,
                            RelationKind::Calls,
                            text(seg, self.source).to_string(),
                            location(seg),
                            target,
                        );
                    } else if let Some(var) = self.bound_closure_name(function) {
                        // `$cb()` after `$cb = function () {}`: a call to the
                        // closure symbol the assignment created.
                        self.push_targeted(
                            owner,
                            RelationKind::Calls,
                            var.to_string(),
                            location(function),
                            None,
                        );
                    }
                    self.visit(function, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression" => {
                let scoped = node.kind() == "scoped_call_expression";
                let receiver = node
                    .child_by_field_name("object")
                    .or_else(|| node.child_by_field_name("scope"));
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.kind() == "name" {
                        let target = if scoped {
                            self.scoped_call_target(receiver)
                        } else {
                            self.member_call_target(receiver)
                        };
                        // location(name_node), not location(node): a chained
                        // call (`$a->f($x)->f($y)`) has its outer and inner
                        // call both start at `$a`, which would make two
                        // same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_targeted(
                            owner,
                            RelationKind::Calls,
                            text(name_node, self.source).to_string(),
                            location(name_node),
                            Some(target),
                        );
                    }
                }
                if let Some(receiver) = receiver {
                    if scoped && last_segment(receiver).is_some() {
                        self.push_class_reference(owner, RelationKind::References, None, receiver);
                    } else {
                        self.visit(receiver, owner, type_name, depth + 1);
                    }
                }
                if scoped {
                    self.laravel_scoped_call(node, owner);
                } else {
                    self.laravel_member_call(node, owner);
                }
                let controller = if scoped {
                    None
                } else {
                    self.controller_group_context(node)
                };
                let bound = controller.is_some();
                self.route_controllers.extend(controller);
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
                if bound {
                    self.route_controllers.pop();
                }
            }
            // `Post::class`, `Post::STATUS`, `Post::$registry`: the class
            // named in the scope position is referenced.
            "class_constant_access_expression" | "scoped_property_access_expression" => {
                let scope = node
                    .child_by_field_name("scope")
                    .or_else(|| node.named_child(0));
                if let Some(scope) = scope {
                    if last_segment(scope).is_some() {
                        self.push_class_reference(owner, RelationKind::References, None, scope);
                    } else {
                        self.visit(scope, owner, type_name, depth + 1);
                    }
                }
            }
            // `$x instanceof Post`.
            "binary_expression" => {
                let is_instanceof = {
                    let mut cursor = node.walk();
                    let found = node.children(&mut cursor).any(|c| c.kind() == "instanceof");
                    found
                };
                let right = node.child_by_field_name("right");
                match right {
                    Some(right) if is_instanceof && last_segment(right).is_some() => {
                        if let Some(left) = node.child_by_field_name("left") {
                            self.visit(left, owner, type_name, depth + 1);
                        }
                        self.push_class_reference(owner, RelationKind::References, None, right);
                    }
                    _ => self.visit_children(node, owner, type_name, depth + 1),
                }
            }
            // A declared type in a parameter, return, property or `catch`
            // position. Built-in and relative (`self`) types are skipped by
            // `push_class_reference`.
            "named_type" => {
                if let Some(name) = named_children(node)
                    .into_iter()
                    .find(|c| last_segment(*c).is_some())
                {
                    self.push_class_reference(owner, RelationKind::References, None, name);
                }
            }
            // `#[Name(args)]` on a class, member, parameter or enum: a
            // reference from the declaration it decorates to the attribute
            // class. Class/method/enum attributes are routed here by
            // `visit_attributes` with `owner` set to the new symbol; on a
            // parameter, `owner` is already the function.
            "attribute" => {
                if let Some(name) = named_children(node)
                    .into_iter()
                    .find(|c| last_segment(*c).is_some())
                {
                    self.push_class_reference(
                        owner,
                        RelationKind::References,
                        Some(SymbolKind::Class),
                        name,
                    );
                }
                if let Some(arguments) = node.child_by_field_name("parameters") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            // `new Foo(...)` calls Foo's constructor: recorded as a call to
            // the class name, like `mct-lang-csharp`'s `new T()`. `new
            // self`/`new static`/`new $class` have no static class name.
            "object_creation_expression" => {
                for child in named_children(node) {
                    if last_segment(child).is_some() {
                        self.push_class_reference(
                            owner,
                            RelationKind::Calls,
                            Some(SymbolKind::Class),
                            child,
                        );
                    } else {
                        self.visit(child, owner, type_name, depth + 1);
                    }
                }
            }
            // `new class (...) extends B implements I { ... }` is an
            // expression, not a declaration: its members are not indexed
            // as symbols (they'd have no owning type), its bases become
            // references from the enclosing owner, and every call in its
            // methods, arguments and attributes is attributed to that owner.
            "anonymous_class" => {
                for (clause, declared_as) in [
                    ("base_clause", SymbolKind::Class),
                    ("class_interface_clause", SymbolKind::Interface),
                ] {
                    if let Some(clause) = find_child(node, clause) {
                        for base in named_children(clause) {
                            self.push_class_reference(
                                owner,
                                RelationKind::References,
                                Some(declared_as),
                                base,
                            );
                        }
                    }
                }
                self.visit_attributes(node, owner, depth);
                if let Some(arguments) = find_child(node, "arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
                self.classes.push(ClassContext::Anonymous);
                if let Some(body) = node.child_by_field_name("body") {
                    if self.is_anonymous_migration(node) {
                        self.visit_migration_body(body, depth + 1);
                    } else {
                        self.visit_anonymous_class_body(body, owner, depth + 1);
                    }
                }
                self.classes.pop();
            }
            // `$cb = function () {}` / `$cb = fn() => ...`: name the
            // closure after the variable it's bound to, same reasoning as
            // `mct-lang-js-ts`'s handling of the same pattern — otherwise
            // every PHP callback would be invisible to the index.
            "assignment_expression" => {
                let left = node.child_by_field_name("left");
                let right = node.child_by_field_name("right");
                match (left, right) {
                    (Some(l), Some(r))
                        if matches!(r.kind(), "anonymous_function" | "arrow_function") =>
                    {
                        if let Some(var) = variable_text(l, self.source) {
                            // `parent: None`: a closure bound to a local
                            // variable inside a method is not a member of
                            // the method's class.
                            let id = self.push_symbol(
                                var.to_string(),
                                SymbolKind::Function,
                                location(r),
                                None,
                            );
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
                    (Some(l), _) => {
                        self.declare_global(l, owner);
                        self.visit_children(node, owner, type_name, depth + 1);
                    }
                    _ => self.visit_children(node, owner, type_name, depth + 1),
                }
            }
            "anonymous_function" | "arrow_function" => {
                self.closure_depth += 1;
                self.visit_children(node, owner, type_name, depth + 1);
                self.closure_depth -= 1;
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    /// `$x = …` at file scope (`owner` is the file's module): the first one
    /// in the file declares `x` a `variable`, named without `$` as a
    /// PowerShell variable is. A class or an unbound closure around it has
    /// its own scope; a superglobal, `$$x`, a property, an element or a
    /// destructuring pattern declares nothing.
    fn declare_global(&mut self, left: Node, owner: SymbolId) {
        if owner != self.module_id || !self.classes.is_empty() || self.closure_depth > 0 {
            return;
        }
        let Some(name) = variable_text(left, self.source) else {
            return;
        };
        if name == "this" || builtins::is_superglobal(name) {
            return;
        }
        let declared = self
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Variable && s.name == name);
        if !declared {
            self.push_symbol(name.to_string(), SymbolKind::Variable, location(left), None);
        }
    }

    /// Visits a declaration's `attributes` field (its `#[...]` list) with
    /// `owner` as the source of each attribute's reference.
    fn visit_attributes(&mut self, node: Node, owner: SymbolId, depth: u32) {
        if let Some(attributes) = node.child_by_field_name("attributes") {
            self.visit(attributes, owner, None, depth + 1);
        }
    }

    /// Walks an anonymous class's members without indexing them: method
    /// bodies, parameters (promotion defaults, attributes) and property
    /// initializers are visited for their relations only, all owned by
    /// `owner`. Named declarations nested deeper (a closure bound to a
    /// variable) are still indexed, top-level like any closure.
    fn visit_anonymous_class_body(&mut self, body: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        for member in named_children(body) {
            match member.kind() {
                "method_declaration" => {
                    self.visit_attributes(member, owner, depth);
                    for field in ["parameters", "return_type", "body"] {
                        if let Some(part) = member.child_by_field_name(field) {
                            self.visit_anonymous_member_part(part, owner, depth + 1);
                        }
                    }
                }
                "property_declaration" | "const_declaration" => {
                    self.visit_anonymous_member_part(member, owner, depth + 1);
                }
                _ => self.visit(member, owner, None, depth + 1),
            }
        }
    }

    /// Visits `node`'s subtree for relations, except that promoted
    /// constructor parameters are not turned into fields.
    fn visit_anonymous_member_part(&mut self, node: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "property_promotion_parameter" => {
                    self.visit_attributes(child, owner, depth);
                    if let Some(ty) = child.child_by_field_name("type") {
                        self.visit(ty, owner, None, depth + 1);
                    }
                    if let Some(default) = child.child_by_field_name("default_value") {
                        self.visit(default, owner, None, depth + 1);
                    }
                }
                "property_element" | "const_element" => {
                    self.visit_anonymous_member_part(child, owner, depth + 1);
                }
                _ => self.visit(child, owner, None, depth + 1),
            }
        }
    }

    /// `use A\B\Type;`, `use A\B\{C, D as E};`, `use function A\f;`,
    /// `use const A\C;`: registers each import in the scope and records one
    /// `Imports` relation per clause, to the name the clause really imports
    /// (not its local alias, which no other file uses).
    ///
    /// A class import depends on the file declaring that class: it targets
    /// the `Module` symbol of the file whose declared namespace is the
    /// clause's namespace and that declares the class (or is named after
    /// it). A function or constant import targets that symbol kind.
    fn visit_namespace_use(&mut self, node: Node, owner: SymbolId) {
        let declaration_kind = import_kind(node);
        let prefix = find_child(node, "namespace_name")
            .map(|n| text(n, self.source).trim().trim_matches('\\').to_string());
        let clauses: Vec<Node> = match node.child_by_field_name("body") {
            Some(group) => named_children(group)
                .into_iter()
                .filter(|c| c.kind() == "namespace_use_clause")
                .collect(),
            None => find_child(node, "namespace_use_clause")
                .into_iter()
                .collect(),
        };
        for clause in clauses {
            let kind = import_kind(clause)
                .or(declaration_kind)
                .unwrap_or(ImportKind::Class);
            let Some(name_node) = named_children(clause)
                .into_iter()
                .find(|c| last_segment(*c).is_some())
            else {
                continue;
            };
            let written = text(name_node, self.source).trim().trim_matches('\\');
            let full = match &prefix {
                Some(prefix) => format!("{prefix}\\{written}"),
                None => written.to_string(),
            };
            let alias = clause
                .child_by_field_name("alias")
                .map(|a| text(a, self.source).to_string());
            self.scope.import(kind, &full, alias.as_deref());

            let (namespace, name) = match full.rsplit_once('\\') {
                Some((namespace, name)) => (namespace.to_string(), name.to_string()),
                None => (String::new(), full.clone()),
            };
            let target = RelationTarget {
                kind: Some(match kind {
                    ImportKind::Class => SymbolKind::Module,
                    ImportKind::Function => SymbolKind::Function,
                    ImportKind::Constant => SymbolKind::Constant,
                }),
                target_module: Some(namespace_alias(&namespace)),
                ..Default::default()
            };
            self.push_targeted(
                owner,
                RelationKind::Imports,
                name,
                location(clause),
                Some(target),
            );
        }
    }

    fn finish(self) -> ParsedFile {
        // The file's `Module` symbol answers to every namespace the file
        // declares (the global one when it declares none) and to the name of
        // each type it declares, which is what lets an import find the file
        // that declares a class even when the file is not named after it.
        let mut aliases: Vec<SymbolAlias> = Vec::new();
        let module = self.module_id;
        if self.namespaces.is_empty() {
            aliases.push(SymbolAlias {
                symbol: module,
                name: namespace_alias(""),
            });
        }
        for namespace in &self.namespaces {
            aliases.push(SymbolAlias {
                symbol: module,
                name: namespace_alias(namespace),
            });
        }
        for declared in &self.declared_types {
            aliases.push(SymbolAlias {
                symbol: module,
                name: declared.clone(),
            });
        }
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            symbol_aliases: aliases,
            entry_points: self.entry_points,
            symbol_details: self.details,
            ..Default::default()
        }
    }

    /// `require`/`include` of a path built from a literal, either alone or
    /// after `__DIR__`: an `Imports` relation to the target file's `Module`
    /// symbol, pinned to its exact path. A bare relative literal is read
    /// against the including file's directory (PHP also tries `include_path`
    /// and the working directory, so a file elsewhere stays unresolved); an
    /// absolute or computed path records nothing.
    fn push_include_target(&mut self, owner: SymbolId, target: Node, loc: Location) {
        let spec = match target.kind() {
            "binary_expression" => {
                let left = target.child_by_field_name("left");
                let right = target.child_by_field_name("right");
                match (left, right) {
                    (Some(l), Some(r)) if text(l, self.source) == "__DIR__" => {
                        literal_string_text(r, self.source).map(|lit| format!(".{lit}"))
                    }
                    _ => None,
                }
            }
            _ => literal_string_text(target, self.source)
                .filter(|lit| !lit.starts_with('/'))
                .map(|lit| {
                    if lit.starts_with("./") || lit.starts_with("../") {
                        lit
                    } else {
                        format!("./{lit}")
                    }
                }),
        };
        let Some(resolved) =
            spec.and_then(|spec| mct_core::resolve_reference_path(self.path, &spec))
        else {
            return;
        };
        let file = resolved.rsplit('/').next().unwrap_or(&resolved);
        let name = file.trim_end_matches(".php").to_string();
        if name.is_empty() || !file.ends_with(".php") {
            return;
        }
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name, loc);
        self.relation_targets.push(RelationTarget {
            relation,
            path: Some(resolved),
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
    }
}

/// `self`, `static`, `parent` and the built-in type names are written like a
/// class but name no declaration.
fn is_reserved_type_name(written: &str) -> bool {
    matches!(
        written.to_ascii_lowercase().as_str(),
        "self"
            | "static"
            | "parent"
            | "array"
            | "callable"
            | "iterable"
            | "object"
            | "mixed"
            | "void"
            | "never"
            | "null"
            | "false"
            | "true"
            | "int"
            | "float"
            | "string"
            | "bool"
    )
}

/// The `function` / `const` keyword of a `use` declaration or clause.
fn import_kind(node: Node) -> Option<ImportKind> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find_map(|c| match c.kind() {
        "function" => Some(ImportKind::Function),
        "const" => Some(ImportKind::Constant),
        _ => None,
    });
    found
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

/// The lowercase name of every `function` declared anywhere under `node`.
fn collect_function_names(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if node.kind() == "function_definition" {
        if let Some(name) = node.child_by_field_name("name") {
            out.insert(text(name, source).to_ascii_lowercase());
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_function_names(child, source, out, depth + 1);
    }
}
