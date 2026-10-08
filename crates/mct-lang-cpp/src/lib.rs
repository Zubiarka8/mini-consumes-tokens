//! `LanguageParser` implementation for C++, via `tree-sitter-cpp`.
//!
//! The one case this parser is built around: a class/function is routinely
//! **declared** in a `.h`/`.hpp` header and **defined** in a separate `.cpp`
//! file (`ReturnType ClassName::method(...) { ... }`). Each file is parsed in
//! isolation (see [`mct_core::LanguageParser::parse`]), so there is no way to
//! merge them into one database row here — that only happens if both sides
//! emit a [`SymbolRecord`] with the *exact same* `name` and `parent`.
//! `declarator_name_and_parent` below exists specifically to pull `name` and
//! `parent` apart from a `qualified_identifier` (`ClassName::method`) the
//! same way the in-class declaration would produce them, so
//! `find_symbol`/`find_references` correlate the two by name+parent — see
//! `tests/index_integration.rs` for the header/definition-split assertions.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use std::collections::HashSet;

use mct_tree_sitter::{first_error, location};
use tree_sitter::{Node, Parser};

pub struct CppParser;

impl LanguageParser for CppParser {
    fn language_id(&self) -> &'static str {
        "cpp"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["cpp", "cc", "cxx", "hpp", "hh", "h"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_cpp::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("tree-sitter-cpp grammar is statically valid");

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
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location, None);
        walker.visit_children(root, Ctx::file(module_id), 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    let file_name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    // Strip whichever of the 6 registered extensions is present, in one
    // pass (a header and its source share this file-root pseudo-symbol
    // naming, though only the real name/parent match on symbols matters
    // for header/definition correlation, not this one).
    for ext in [".cpp", ".cxx", ".hpp", ".cc", ".hh", ".h"] {
        if let Some(stripped) = file_name.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    file_name.to_string()
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// Where the walk currently is.
#[derive(Clone, Copy)]
struct Ctx<'s> {
    /// Innermost enclosing function/method, or the file's module: calls
    /// and includes attach to it.
    owner: SymbolId,
    /// Innermost enclosing class/struct/union/enum or namespace, used as
    /// `parent` for what is declared directly inside it.
    scope: Option<Scope<'s>>,
    /// Inside a function body: declarations here are locals (not symbols),
    /// only their initializers matter.
    local: bool,
}

#[derive(Clone, Copy)]
struct Scope<'s> {
    name: &'s str,
    /// A class/struct/union (members are methods/fields) rather than a
    /// namespace (members are free functions/variables).
    is_type: bool,
}

impl<'s> Ctx<'s> {
    fn file(module_id: SymbolId) -> Self {
        Ctx {
            owner: module_id,
            scope: None,
            local: false,
        }
    }

    fn parent(&self) -> Option<String> {
        self.scope.map(|s| s.name.to_string())
    }

    fn in_type(&self) -> bool {
        self.scope.is_some_and(|s| s.is_type)
    }

    fn function_kind(&self) -> SymbolKind {
        if self.in_type() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        }
    }

    fn variable_kind(&self) -> SymbolKind {
        if self.in_type() {
            SymbolKind::Field
        } else {
            SymbolKind::Variable
        }
    }

    fn with_scope(self, name: &'s str, is_type: bool) -> Self {
        Ctx {
            scope: Some(Scope { name, is_type }),
            local: false,
            ..self
        }
    }

    fn in_body_of(self, function: SymbolId) -> Self {
        Ctx {
            owner: function,
            scope: None,
            local: true,
        }
    }
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
    /// Namespace names opened so far in this file (innermost segment), so
    /// an out-of-line `ns::fn()` definition is told apart from a
    /// `Class::method()` one.
    namespaces: HashSet<&'a str>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
            namespaces: HashSet::new(),
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

    fn visit_children(&mut self, node: Node<'a>, ctx: Ctx<'a>, depth: u32) {
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for child in children {
            self.visit(child, ctx, depth + 1);
        }
    }

    fn visit(&mut self, node: Node<'a>, ctx: Ctx<'a>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "namespace_definition" => {
                let body = node.child_by_field_name("body");
                // An anonymous namespace only gives its contents internal
                // linkage; they stay file-level, with no named scope.
                let Some(name_node) = node.child_by_field_name("name") else {
                    if let Some(body) = body {
                        self.visit_children(body, ctx, depth + 1);
                    }
                    return;
                };
                let name = text(name_node, self.source);
                self.namespaces
                    .insert(name.rsplit("::").next().unwrap_or(name));
                self.push_symbol(
                    name.to_string(),
                    SymbolKind::Module,
                    location(node),
                    ctx.parent(),
                );
                if let Some(body) = body {
                    self.visit_children(body, ctx.with_scope(name, false), depth + 1);
                }
            }
            "class_specifier" | "struct_specifier" | "union_specifier" => {
                self.visit_class(node, ctx, depth);
            }
            "enum_specifier" => {
                // Only a definition (with a body) declares the enum; `enum
                // Color c;` merely uses it.
                let (Some(name_node), Some(body)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("body"),
                ) else {
                    return;
                };
                let (name, parent) = split_qualified(name_node, self.source, ctx.parent());
                self.push_symbol(name.clone(), SymbolKind::Enum, location(node), parent);
                let mut cursor = body.walk();
                let enumerators: Vec<Node> = body.named_children(&mut cursor).collect();
                for enumerator in enumerators {
                    if enumerator.kind() != "enumerator" {
                        continue;
                    }
                    if let Some(n) = enumerator.child_by_field_name("name") {
                        self.push_symbol(
                            text(n, self.source).to_string(),
                            SymbolKind::Constant,
                            location(enumerator),
                            Some(name.clone()),
                        );
                    }
                    if let Some(value) = enumerator.child_by_field_name("value") {
                        self.visit(value, ctx, depth + 1);
                    }
                }
            }
            // `using Name = Type;`
            "alias_declaration" => {
                if !ctx.local {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        self.push_symbol(
                            text(name_node, self.source).to_string(),
                            SymbolKind::TypeAlias,
                            location(node),
                            ctx.parent(),
                        );
                    }
                }
            }
            // `typedef Type Name, *NamePtr;`
            "type_definition" => {
                if let Some(ty) = node.child_by_field_name("type") {
                    self.visit(ty, ctx, depth + 1);
                }
                if ctx.local {
                    return;
                }
                let mut cursor = node.walk();
                let declarators: Vec<Node> = node
                    .children_by_field_name("declarator", &mut cursor)
                    .collect();
                for declarator in declarators {
                    if let Some(name_node) = type_declarator_identifier(declarator) {
                        self.push_symbol(
                            text(name_node, self.source).to_string(),
                            SymbolKind::TypeAlias,
                            location(node),
                            ctx.parent(),
                        );
                    }
                }
            }
            // A function/method *definition* (has a body). The declarator
            // may be wrapped (`int* Foo::bar()` → pointer_declarator around
            // the function_declarator) and its own name may be a bare
            // identifier (free function / in-class definition) or a
            // `qualified_identifier` (`ClassName::method`, the out-of-line
            // definition half of the header/source split).
            "function_definition" => {
                let Some(func_declarator) = node
                    .child_by_field_name("declarator")
                    .and_then(find_function_declarator)
                else {
                    self.visit_children(node, ctx, depth + 1);
                    return;
                };
                let (name, parent, kind) =
                    self.function_identity(func_declarator.child_by_field_name("declarator"), ctx);
                let id = self.push_symbol(name, kind, location(node), parent);
                let inner = ctx.in_body_of(id);
                if let Some(params) = func_declarator.child_by_field_name("parameters") {
                    self.visit_children(params, inner, depth + 1);
                }
                // Constructor initializer lists (`: base_(make())`) and the
                // body both run inside the function.
                let mut cursor = node.walk();
                let rest: Vec<Node<'a>> = node
                    .children(&mut cursor)
                    .filter(|c| {
                        matches!(
                            c.kind(),
                            "field_initializer_list" | "compound_statement" | "try_statement"
                        )
                    })
                    .collect();
                for child in rest {
                    self.visit_children(child, inner, depth + 1);
                }
            }
            // Prototype-only forms: a class member declared but not defined
            // (`field_declaration`, inside a `class`/`struct` body) or a
            // free function/variable declared at namespace scope
            // (`declaration`). Both can hold more than one comma-separated
            // declarator (`int a, b;`). Inside a function body they are
            // locals — `std::lock_guard lock(m);` even parses like a
            // function prototype — so only their initializers are walked.
            "field_declaration" | "declaration" => {
                let mut cursor = node.walk();
                let declarators: Vec<Node<'a>> = node
                    .children_by_field_name("declarator", &mut cursor)
                    .collect();
                if !ctx.local {
                    for declarator in &declarators {
                        if let Some(func_declarator) = find_function_declarator(*declarator) {
                            let (name, parent, kind) = self.function_identity(
                                func_declarator.child_by_field_name("declarator"),
                                ctx,
                            );
                            self.push_symbol(name, kind, location(node), parent);
                        } else if let Some(name_node) = plain_declarator_identifier(*declarator) {
                            self.push_symbol(
                                text(name_node, self.source).to_string(),
                                ctx.variable_kind(),
                                location(*declarator),
                                ctx.parent(),
                            );
                        }
                    }
                }
                // Initializers (`= make()`, `x(make())`, `{a, b}`), default
                // member initializers and an inline type definition
                // (`struct { ... } x;`) can all contain calls or symbols.
                self.visit_children(node, ctx, depth + 1);
            }
            "preproc_include" => {
                if let Some(path_node) = node.child_by_field_name("path") {
                    let raw = text(path_node, self.source);
                    // Quoted (`"Invoice.h"`) is a local, in-repo header;
                    // angle-bracketed (`<vector>`) is a system/external one
                    // — recorded the same way (Imports is the only fitting
                    // `RelationKind`), but its name will simply never match
                    // a file in this repo's index, so it naturally never
                    // resolves to a local symbol (see `mct-index`'s
                    // by-name relation resolution) instead of needing a
                    // separate "external dependency" concept here.
                    let name = if let Some(stripped) =
                        raw.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
                    {
                        stripped.to_string()
                    } else {
                        raw.trim_start_matches('<')
                            .trim_end_matches('>')
                            .to_string()
                    };
                    self.push_relation(ctx.owner, RelationKind::Imports, name, location(node));
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(name_node) = callee_identifier(function) {
                        // location(name_node), not location(node): a chained
                        // call (`a.f(x).f(y)`) has its outer and inner
                        // call_expression both start at `a`, which would make
                        // two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(
                            ctx.owner,
                            RelationKind::Calls,
                            text(name_node, self.source).to_string(),
                            location(name_node),
                        );
                    }
                    self.visit(function, ctx, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, ctx, depth + 1);
                }
            }
            _ => self.visit_children(node, ctx, depth + 1),
        }
    }

    fn visit_class(&mut self, node: Node<'a>, ctx: Ctx<'a>, depth: u32) {
        let body = node.child_by_field_name("body");
        let name_node = node.child_by_field_name("name");
        // A forward declaration (`class Writer;`), an elaborated type use
        // (`struct Foo* p`) or a `friend class X;` declares nothing new.
        let Some(body) = body else {
            return;
        };
        let kind = match node.kind() {
            "class_specifier" => SymbolKind::Class,
            _ => SymbolKind::Struct,
        };
        // An out-of-line nested class definition (`class Outer::Inner {`)
        // belongs to `Outer`, like the in-class declaration would.
        let (name, parent) = match name_node {
            Some(n) => split_qualified(n, self.source, ctx.parent()),
            None => (String::new(), ctx.parent()),
        };
        let id = self.push_symbol(name, kind, location(node), parent);

        // `base_class_clause` is a direct child, not a named field —
        // every C++ base (`public`/`private`/`protected`) is genuine
        // inheritance (unlike C#, there's no syntactic "implements"
        // to split out: interfaces are just abstract base classes),
        // so every entry becomes Extends regardless of access.
        if let Some(bases) = find_child(node, "base_class_clause") {
            let mut cursor = bases.walk();
            let bases: Vec<Node> = bases.children(&mut cursor).collect();
            for base in bases {
                if matches!(
                    base.kind(),
                    "type_identifier" | "qualified_identifier" | "template_type"
                ) {
                    let name_node = if base.kind() == "qualified_identifier" {
                        qualified_tail(base)
                    } else {
                        base
                    };
                    self.push_relation(
                        id,
                        RelationKind::Extends,
                        text(name_node, self.source).to_string(),
                        location(base),
                    );
                }
            }
        }
        // Members hang off the class's own unqualified name.
        let scope_name = name_node.map_or("", |n| last_segment_text(n, self.source));
        self.visit_children(body, ctx.with_scope(scope_name, true), depth + 1);
    }

    /// Name, parent and kind of a function from its declarator's name
    /// node. An unqualified name takes the enclosing scope; a qualified
    /// one (`Class::method`, `ns::Class::method`, `Tmpl<T>::method`,
    /// `ns::free_fn`) takes its innermost scope segment, template
    /// arguments dropped, so it matches the in-class/in-namespace
    /// declaration. It is a method unless that segment is a namespace
    /// opened earlier in this file.
    fn function_identity(
        &self,
        name_node: Option<Node>,
        ctx: Ctx,
    ) -> (String, Option<String>, SymbolKind) {
        let Some(node) = name_node else {
            return (String::new(), ctx.parent(), ctx.function_kind());
        };
        if node.kind() != "qualified_identifier" {
            return (
                text(node, self.source).to_string(),
                ctx.parent(),
                ctx.function_kind(),
            );
        }
        let (name, parent) = split_qualified(node, self.source, ctx.parent());
        let kind = match parent.as_deref() {
            Some(p) if self.namespaces.contains(p) => SymbolKind::Function,
            _ => SymbolKind::Method,
        };
        (name, parent, kind)
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            ..Default::default()
        }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|n| n.kind() == kind)
}

/// Descends through declarator wrappers (`int* foo()` → `pointer_declarator`
/// around the real `function_declarator`) to find the actual function
/// signature node, or `None` if this declarator is a plain variable/field
/// (no function signature to unwrap).
fn find_function_declarator(node: Node) -> Option<Node> {
    match node.kind() {
        "function_declarator" => Some(node),
        "pointer_declarator"
        | "array_declarator"
        | "parenthesized_declarator"
        | "init_declarator" => node
            .child_by_field_name("declarator")
            .and_then(find_function_declarator),
        "reference_declarator" => node.named_child(0).and_then(find_function_declarator),
        _ => None,
    }
}

/// Same unwrapping as `find_function_declarator`, but for the non-function
/// (field/variable) case — returns the identifier actually being declared.
fn plain_declarator_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" | "field_identifier" => Some(node),
        "pointer_declarator" | "array_declarator" | "init_declarator" => node
            .child_by_field_name("declarator")
            .and_then(plain_declarator_identifier),
        "reference_declarator" => node.named_child(0).and_then(plain_declarator_identifier),
        _ => None,
    }
}

/// The name a `typedef` introduces (`typedef int Id;`, `typedef T* Ptr;`).
fn type_declarator_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "type_identifier" | "primitive_type" => Some(node),
        "pointer_declarator" | "array_declarator" | "function_declarator" => node
            .child_by_field_name("declarator")
            .and_then(type_declarator_identifier),
        "reference_declarator" => node.named_child(0).and_then(type_declarator_identifier),
        _ => None,
    }
}

/// Splits a possibly-qualified name into `(name, parent)`: `label` →
/// (`label`, `fallback`); `Class::method`, `ns::Class::method` and
/// `Tmpl<T>::method` → (`method`, `Class`/`Tmpl`) — the innermost scope
/// segment, template arguments dropped, which is what the in-class
/// declaration of the same member records as its parent. That is how the
/// header and source halves of a split definition line up by name+parent.
fn split_qualified(node: Node, source: &str, fallback: Option<String>) -> (String, Option<String>) {
    if node.kind() != "qualified_identifier" {
        return (last_segment_text(node, source).to_string(), fallback);
    }
    let mut scope = node.child_by_field_name("scope");
    let mut current = node;
    while let Some(inner) = current
        .child_by_field_name("name")
        .filter(|n| n.kind() == "qualified_identifier")
    {
        scope = inner.child_by_field_name("scope").or(scope);
        current = inner;
    }
    let name = current
        .child_by_field_name("name")
        .map(|n| last_segment_text(n, source).to_string())
        .unwrap_or_default();
    let parent = scope
        .map(|s| last_segment_text(s, source).to_string())
        .filter(|s| !s.is_empty())
        .or(fallback);
    (name, parent)
}

/// The unqualified, template-argument-free text of a name node:
/// `Tmpl<K, V>` → `Tmpl`, `a::b::C` → `C`, anything else verbatim.
fn last_segment_text<'s>(node: Node, source: &'s str) -> &'s str {
    match node.kind() {
        "template_type" | "template_function" => node
            .child_by_field_name("name")
            .map_or("", |n| last_segment_text(n, source)),
        "qualified_identifier" => last_segment_text(qualified_tail(node), source),
        _ => text(node, source),
    }
}

/// Rightmost name in a possibly-nested `qualified_identifier`
/// (`A::B::C` → `C`), used for base-class names and qualified call targets.
fn qualified_tail(node: Node) -> Node {
    if node.kind() == "qualified_identifier" {
        if let Some(name) = node.child_by_field_name("name") {
            return qualified_tail(name);
        }
    }
    node
}

/// The function/method name being invoked: a bare `identifier`/
/// `operator_name`, the `field` of `receiver.Name(...)`/`receiver->Name(...)`,
/// or the tail of a qualified call (`Base::method()`, `std::max(...)`).
fn callee_identifier(function: Node) -> Option<Node> {
    match function.kind() {
        "identifier" | "operator_name" | "destructor_name" | "field_identifier" => Some(function),
        "field_expression" => function.child_by_field_name("field"),
        "qualified_identifier" => Some(qualified_tail(function)),
        "template_function" => function.child_by_field_name("name").map(qualified_tail),
        _ => None,
    }
}
