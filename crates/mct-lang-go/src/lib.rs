//! `LanguageParser` implementation for Go, via `tree-sitter-go`.
//!
//! **Deferred by design**: `find_implementations` is not implemented for Go.
//! Go interface satisfaction is structural (a type "implements" an interface
//! purely by having a matching method set — there is no `implements`
//! keyword anywhere in the source), so answering it correctly needs a real
//! type checker, not an AST walk. This parser only extracts what the syntax
//! actually declares: an `interface_type`'s own symbol and its declared
//! `method_elem`s (see the `type_spec` arm below) — never a guess at which
//! structs satisfy it. Revisit alongside the general LSP/type-resolution
//! decision (see `checklist.md`).

use std::collections::HashSet;

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_child, first_error, location};
use tree_sitter::{Node, Parser};

mod details;
mod receivers;
mod stdlib;

pub struct GoParser;

impl LanguageParser for GoParser {
    fn language_id(&self) -> &'static str {
        "go"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_go::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("tree-sitter-go grammar is statically valid");

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

        // A Go package spans multiple files, each restating its own
        // `package name` clause — same pattern C#'s `namespace_declaration`
        // uses (one Module-kind symbol per occurrence, not deduplicated
        // across files), so `find_symbol("billing")` returning one hit per
        // file that declares it is expected, not a bug.
        let package_name = find_child(root, "package_clause")
            .and_then(|pc| pc.named_child(0))
            .map(|n| text(n, &file.contents).to_string());

        let module_name = module_name_for(&file.relative_path);
        let mut walker = Walker::new(&file.contents);
        walker.globals = receivers::globals(root, &file.contents);
        walker.structs = receivers::structs(root, &file.contents);
        walker.std_imports = stdlib::StdImports::of(root, &file.contents);
        walker.packages = repo_packages(root, &file.contents);
        let file_loc = file_location(root);
        let file_module_id = walker.push_symbol(module_name, SymbolKind::Module, file_loc, None);
        if let Some(ref pkg) = package_name {
            walker.push_symbol(pkg.clone(), SymbolKind::Module, file_loc, None);
        }
        walker.visit_children(root, file_module_id, package_name.as_deref(), 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".go")
        .to_string()
}

/// The whole file's location. The root node of a file ending in a newline
/// ends at column 0 of the (empty) line after the last one, so its end line
/// is the previous row.
fn file_location(root: Node) -> Location {
    let mut loc = location(root);
    let end = root.end_position();
    if end.column == 0 && end.row > 0 {
        loc.end_line = Some(end.row as u32);
    }
    loc
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

#[derive(Default)]
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
    /// Declared types of the enclosing function's locals (see `receivers`).
    locals: receivers::Types,
    /// Declared types of the file's package-level `var`s.
    globals: receivers::Types,
    /// The structs and methods this file declares.
    structs: receivers::Structs,
    /// Declaration details of the symbols recorded so far (ADR-034).
    details: Vec<mct_core::SymbolDetail>,
    /// The standard packages this file imports and never rebinds.
    std_imports: stdlib::StdImports,
    /// The names this file's unaliased repository imports bind (see
    /// `repo_packages`).
    packages: HashSet<String>,
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

    /// `owner` is the innermost enclosing function/method (calls attach to
    /// it); `package_name` is this file's package, used as `parent` for
    /// top-level functions/types/interfaces (Go has no further nesting of
    /// declarations below package scope).
    fn visit_children(
        &mut self,
        node: Node,
        owner: SymbolId,
        package_name: Option<&str>,
        depth: u32,
    ) {
        mct_tree_sitter::for_each_child(node, |child| {
            self.visit(child, owner, package_name, depth + 1);
        });
    }

    fn visit(&mut self, node: Node, owner: SymbolId, package_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let detail = details::function_detail(node, &name, self.next_id, self.source);
                let id = self.push_symbol(
                    name,
                    SymbolKind::Function,
                    location(node),
                    package_name.map(str::to_string),
                );
                self.details.push(detail);
                self.visit_function(node, id, package_name, depth);
            }
            "method_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let receiver_type = receiver_type_name(node, self.source);
                let parent = receiver_type.or_else(|| package_name.map(str::to_string));
                let detail = details::function_detail(node, &name, self.next_id, self.source);
                let id = self.push_symbol(name, SymbolKind::Method, location(node), parent);
                self.details.push(detail);
                self.visit_function(node, id, package_name, depth);
            }
            "type_spec" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let detail = details::type_detail(node, &name, self.next_id, self.source);
                self.details.push(detail);
                match node.child_by_field_name("type") {
                    Some(type_node) if type_node.kind() == "struct_type" => {
                        let id = self.push_symbol(
                            name.clone(),
                            SymbolKind::Struct,
                            location(node),
                            package_name.map(str::to_string),
                        );
                        self.push_fields(type_node, &name, id, package_name);
                    }
                    Some(type_node) if type_node.kind() == "interface_type" => {
                        let id = self.push_symbol(
                            name.clone(),
                            SymbolKind::Interface,
                            location(node),
                            package_name.map(str::to_string),
                        );
                        // The interface's own declared method set — NOT a
                        // resolution of which types implement it (see the
                        // module doc's "deferred by design" note above).
                        let mut cursor = type_node.walk();
                        for member in type_node.children(&mut cursor) {
                            if member.kind() == "type_elem" && member.named_child_count() == 1 {
                                if let Some(embedded) = member.named_child(0) {
                                    self.push_embedding(
                                        id,
                                        embedded,
                                        Some(SymbolKind::Interface),
                                        package_name,
                                    );
                                }
                            }
                            if member.kind() == "method_elem" {
                                let method_name = member
                                    .child_by_field_name("name")
                                    .map(|n| text(n, self.source).to_string())
                                    .unwrap_or_default();
                                let detail = details::function_detail(
                                    member,
                                    &method_name,
                                    self.next_id,
                                    self.source,
                                );
                                self.details.push(detail);
                                self.push_symbol(
                                    method_name,
                                    SymbolKind::Method,
                                    location(member),
                                    Some(name.clone()),
                                );
                            }
                        }
                    }
                    _ => {
                        // `type UserID int` and similar — recorded for
                        // completeness, out of the spec's required set but
                        // free given the trait already models `TypeAlias`.
                        self.push_symbol(
                            name,
                            SymbolKind::TypeAlias,
                            location(node),
                            package_name.map(str::to_string),
                        );
                    }
                }
            }
            "type_alias" => {
                // `type Amount = Money`
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let detail = details::type_detail(node, &name, self.next_id, self.source);
                self.details.push(detail);
                self.push_symbol(
                    name,
                    SymbolKind::TypeAlias,
                    location(node),
                    package_name.map(str::to_string),
                );
            }
            "const_declaration" | "var_declaration"
                if node.parent().is_some_and(|p| p.kind() == "source_file") =>
            {
                for (spec, function) in self.push_package_values(node, package_name) {
                    match function {
                        // The literal's body is the function's own.
                        Some((literal, id)) => {
                            self.visit_function(literal, id, package_name, depth + 1)
                        }
                        None => self.visit_children(spec, owner, package_name, depth + 1),
                    }
                }
            }
            "import_declaration" => {
                let mut specs = Vec::new();
                collect_import_specs(node, &mut specs);
                for spec in specs {
                    if let Some(path_node) = spec.child_by_field_name("path") {
                        let raw = text(path_node, self.source);
                        let clean = raw.trim_matches(|c| c == '"' || c == '`').to_string();
                        self.push_package_import(owner, &clean, location(spec));
                        // The path names a package, never a same-named
                        // field or variable (`import "log"`).
                        self.push_target(RelationTarget {
                            kind: Some(SymbolKind::Module),
                            ..Default::default()
                        });
                        self.push_relation(owner, RelationKind::Imports, clean, location(spec));
                    }
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(name_node) = callee_identifier(function) {
                        if !self.push_std_package(function) {
                            let name = text(name_node, self.source);
                            self.push_call_evidence(function, name, package_name);
                        }
                        // location(name_node), not location(node): a chained
                        // call (`a.F(x).F(y)`) has its outer and inner
                        // call_expression both start at `a`, which would make
                        // two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(
                            owner,
                            RelationKind::Calls,
                            text(name_node, self.source).to_string(),
                            location(name_node),
                        );
                    }
                    self.visit(function, owner, package_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, package_name, depth + 1);
                }
            }
            "composite_literal" => {
                if let Some(ty) = node.child_by_field_name("type") {
                    self.push_type_use(owner, RelationKind::References, ty, None, package_name);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, package_name, depth + 1);
                }
            }
            "type_conversion_expression" => {
                // `Fail[T](err)` with a single type argument is ambiguous
                // with a conversion to the generic type `Fail[T]`; the
                // grammar picks the conversion. Without type information a
                // generic instantiation is read as the call it usually is.
                if let Some(ty) = node.child_by_field_name("type") {
                    if ty.kind() == "generic_type" {
                        if let Some(name_node) = callee_identifier(ty) {
                            self.push_relation(
                                owner,
                                RelationKind::Calls,
                                text(name_node, self.source).to_string(),
                                location(name_node),
                            );
                        }
                    }
                }
                if let Some(operand) = node.child_by_field_name("operand") {
                    self.visit(operand, owner, package_name, depth + 1);
                }
            }
            _ => self.visit_children(node, owner, package_name, depth + 1),
        }
    }

    /// A function's or method's parameters and body, with its receiver's,
    /// parameters' and locals' declared types known while the body is walked.
    /// Each named field of `struct_type` is a `Field` of `parent`; an
    /// embedded field (`*Base`) names a type, not a field of its own: the
    /// struct `id` extends it.
    fn push_fields(
        &mut self,
        struct_type: Node,
        parent: &str,
        id: SymbolId,
        package_name: Option<&str>,
    ) {
        let Some(list) = struct_type
            .named_children(&mut struct_type.walk())
            .find(|c| c.kind() == "field_declaration_list")
        else {
            return;
        };
        for field in list.named_children(&mut list.walk()) {
            if field.kind() != "field_declaration" {
                continue;
            }
            if field.child_by_field_name("name").is_some() {
                self.push_names(field, "", SymbolKind::Field, Some(parent));
            } else if let Some(embedded) = field.child_by_field_name("type") {
                self.push_embedding(id, embedded, None, package_name);
            }
        }
    }

    /// `from` extends the type `embedded` names, which must be a `kind`
    /// when given (see `push_type_use`).
    fn push_embedding(
        &mut self,
        from: SymbolId,
        embedded: Node,
        kind: Option<SymbolKind>,
        package_name: Option<&str>,
    ) {
        self.push_type_use(from, RelationKind::Extends, embedded, kind, package_name);
    }

    /// A `relation` from `from` to the named type `ty` (`Base`, `pkg.Base`,
    /// `List[T]`), which must be a `kind` when given. A bare name is a
    /// declaration of this package; `pkg.T` is external through a standard
    /// package, a declaration of `pkg` through an unaliased repository
    /// import, and unproven otherwise. Any other type (a union, `~T`, a
    /// slice or map type) names none.
    fn push_type_use(
        &mut self,
        from: SymbolId,
        relation: RelationKind,
        ty: Node,
        kind: Option<SymbolKind>,
        package_name: Option<&str>,
    ) {
        let named = if ty.kind() == "generic_type" {
            match ty.child_by_field_name("type") {
                Some(base) => base,
                None => return,
            }
        } else {
            ty
        };
        let (name, target) = match named.kind() {
            "type_identifier" => (
                named,
                package_name.map(|package| RelationTarget {
                    qualifier: Some(package.to_string()),
                    kind,
                    ..Default::default()
                }),
            ),
            "qualified_type" => {
                let (Some(package), Some(name)) = (
                    named.child_by_field_name("package"),
                    named.child_by_field_name("name"),
                ) else {
                    return;
                };
                let package = text(package, self.source);
                let target = if self.std_imports.is_std(package) {
                    Some(RelationTarget {
                        external: true,
                        ..Default::default()
                    })
                } else if self.packages.contains(package) {
                    Some(RelationTarget {
                        qualifier: Some(package.to_string()),
                        kind,
                        ..Default::default()
                    })
                } else {
                    None
                };
                (name, target)
            }
            _ => return,
        };
        if let Some(target) = target {
            self.push_target(target);
        }
        let to_name = text(name, self.source).to_string();
        self.push_relation(from, relation, to_name, location(ty));
    }

    /// Each name of a package-level `const` / `var` declaration, grouped
    /// (`const ( … )`, `var ( … )`) or not, is a `Constant` / `Variable` of
    /// the package; `var f = func(…) {…}` (one name, no type) is a
    /// `Function`, as a JS arrow bound to a name is. Returns each spec, with
    /// the function literal and symbol of such a binding.
    fn push_package_values<'t>(
        &mut self,
        declaration: Node<'t>,
        package_name: Option<&str>,
    ) -> Vec<(Node<'t>, Option<(Node<'t>, SymbolId)>)> {
        let (keyword, kind) = if declaration.kind() == "const_declaration" {
            ("const", SymbolKind::Constant)
        } else {
            ("var", SymbolKind::Variable)
        };
        let mut specs = Vec::new();
        for child in declaration.named_children(&mut declaration.walk()) {
            match child.kind() {
                "const_spec" | "var_spec" => specs.push(child),
                "var_spec_list" => specs.extend(
                    child
                        .named_children(&mut child.walk())
                        .filter(|c| c.kind() == "var_spec"),
                ),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for spec in specs {
            let function = match func_value(spec) {
                Some(literal) if kind == SymbolKind::Variable => self
                    .push_func_value(spec, literal, package_name)
                    .map(|id| (literal, id)),
                _ => None,
            };
            if function.is_none() {
                self.push_names(spec, keyword, kind, package_name);
            }
            out.push((spec, function));
        }
        out
    }

    /// The `Function` `var f = func(…) {…}` declares, unless its name is `_`.
    fn push_func_value(
        &mut self,
        spec: Node,
        literal: Node,
        parent: Option<&str>,
    ) -> Option<SymbolId> {
        let name_node = spec.child_by_field_name("name")?;
        let name = text(name_node, self.source);
        if name == "_" {
            return None;
        }
        let detail = details::func_value_detail(spec, literal, name, self.next_id, self.source);
        self.details.push(detail);
        Some(self.push_symbol(
            name.to_string(),
            SymbolKind::Function,
            location(name_node),
            parent.map(str::to_string),
        ))
    }

    /// One `kind` symbol per declared name of `spec`, skipping `_`.
    fn push_names(&mut self, spec: Node, keyword: &str, kind: SymbolKind, parent: Option<&str>) {
        let names: Vec<Node> = spec
            .children_by_field_name("name", &mut spec.walk())
            .collect();
        for name_node in names {
            let name = text(name_node, self.source);
            if name == "_" {
                continue;
            }
            let detail = details::value_detail(spec, keyword, name, self.next_id, self.source);
            self.details.push(detail);
            self.push_symbol(
                name.to_string(),
                kind,
                location(name_node),
                parent.map(str::to_string),
            );
        }
    }

    fn visit_function(&mut self, node: Node, id: SymbolId, package_name: Option<&str>, depth: u32) {
        let mut locals = receivers::Types::new();
        receivers::locals(node, self.source, &mut locals, depth + 1);
        let outer_locals = std::mem::replace(&mut self.locals, locals);
        if let Some(params) = node.child_by_field_name("parameters") {
            self.visit_children(params, id, package_name, depth + 1);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_children(body, id, package_name, depth + 1);
        }
        self.locals = outer_locals;
    }

    fn declared(&self, name: &str) -> Option<&receivers::Ty> {
        self.locals.get(name).or_else(|| self.globals.get(name))
    }

    /// The receiver evidence of `x.M()`, from what the syntax proves about
    /// `x`'s type: a named type is the qualifier; a declared type without a
    /// name (`interface{ … }`, a slice) makes a member call. Without proof
    /// (`x := f()`, a `range` variable, a call result, a field of a struct
    /// declared elsewhere, an imported package) nothing is recorded, as
    /// before. A struct this file declares with an embedded field and
    /// without `M` may have `M` promoted from that field, so its type is no
    /// qualifier there: a member call.
    fn push_receiver(&mut self, operand: Node, method: &str) {
        use receivers::Ty;
        let ty = match operand.kind() {
            "identifier" => self.declared(text(operand, self.source)).cloned(),
            // `s.repo.M()`: the field's type, when this file declares the
            // struct `s` is.
            "selector_expression" => {
                let base = operand
                    .child_by_field_name("operand")
                    .filter(|b| b.kind() == "identifier");
                let field = operand.child_by_field_name("field");
                match (
                    base.and_then(|b| self.declared(text(b, self.source))),
                    field,
                ) {
                    (Some(Ty::Named(t)), Some(f)) => self
                        .structs
                        .fields
                        .get(t)
                        .and_then(|fields| fields.get(text(f, self.source)))
                        .cloned(),
                    _ => None,
                }
            }
            // `x.(T).M()`.
            "type_assertion_expression" => operand
                .child_by_field_name("type")
                .map(|t| receivers::named_type(t, self.source).map_or(Ty::Unnamed, Ty::Named)),
            _ => receivers::constructed(operand, self.source).map(Ty::Named),
        };
        let (qualifier, member) = match ty {
            Some(Ty::Named(t)) => {
                let promoted = self.structs.embedding.contains(&t)
                    && !self
                        .structs
                        .methods
                        .get(&t)
                        .is_some_and(|m| m.contains(method));
                if promoted {
                    (None, true)
                } else {
                    (Some(t), false)
                }
            }
            Some(Ty::Unnamed) => (None, true),
            Some(Ty::Unknown) | None => return self.push_value_call(operand),
        };
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            qualifier,
            member,
            ..Default::default()
        });
    }

    /// The evidence of a call about to be pushed. Go has no implicit
    /// receiver, so a bare `f()` names a package-level declaration of
    /// `package_name`, unless `f` is a local (a closure). A selector call
    /// is a receiver's method (`push_receiver`).
    fn push_call_evidence(&mut self, function: Node, name: &str, package_name: Option<&str>) {
        match function.kind() {
            "identifier" if !self.locals.contains_key(name) => {
                if let Some(package) = package_name {
                    self.push_target(RelationTarget {
                        qualifier: Some(package.to_string()),
                        ..Default::default()
                    });
                }
            }
            "selector_expression" => {
                if let Some(operand) = function.child_by_field_name("operand") {
                    self.push_receiver(operand, name);
                }
            }
            _ => {}
        }
    }

    /// `v.M()` where nothing proves `v`'s type. A declared name, an index,
    /// a call result or a field is a value, so `M` is a method, never a
    /// package-level name. An undeclared name is an imported package when
    /// an unaliased repository import binds it: `M` is declared in that
    /// package. Any other name proves nothing.
    fn push_value_call(&mut self, operand: Node) {
        let target = if operand.kind() != "identifier" {
            RelationTarget {
                kind: Some(SymbolKind::Method),
                ..Default::default()
            }
        } else {
            let name = text(operand, self.source);
            if self.declared(name).is_some() {
                RelationTarget {
                    kind: Some(SymbolKind::Method),
                    ..Default::default()
                }
            } else if self.packages.contains(name) {
                RelationTarget {
                    qualifier: Some(name.to_string()),
                    ..Default::default()
                }
            } else {
                return;
            }
        };
        self.push_target(target);
    }

    /// `target` for the relation about to be pushed.
    fn push_target(&mut self, target: RelationTarget) {
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            ..target
        });
    }

    /// `errors.Is(…)` about to be pushed, through a standard package this
    /// file imports and never rebinds: external. `true` when recorded.
    fn push_std_package(&mut self, function: Node) -> bool {
        let package = (function.kind() == "selector_expression")
            .then(|| function.child_by_field_name("operand"))
            .flatten()
            .filter(|o| o.kind() == "identifier");
        if !package.is_some_and(|p| self.std_imports.is_std(text(p, self.source))) {
            return false;
        }
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            external: true,
            ..Default::default()
        });
        true
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

    /// The directory name an import path ends in, skipping a `/vN` major-version
    /// suffix. `None` for cgo (`C`), `unsafe`, empty paths and paths whose first
    /// element has no dot (see `push_package_import`).
    fn package_dir_name(import_path: &str) -> Option<String> {
        if !import_path
            .split('/')
            .next()
            .is_some_and(|root| root.contains('.'))
        {
            return None;
        }
        let mut segments = import_path.rsplit('/').filter(|s| !s.is_empty());
        let mut last = segments.next()?;
        let is_version = |s: &str| {
            s.strip_prefix('v')
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        };
        if is_version(last) {
            last = segments.next()?;
        }
        if matches!(last, "C" | "unsafe" | "." | "..") {
            return None;
        }
        Some(last.to_string())
    }

    /// `import "a/b/pkg"` depends on the package declared in a directory
    /// named `pkg` (the import path's last element): an `Imports` relation to
    /// the `Module` symbol every file of that package declares by its
    /// `package` clause. A package spanning several files therefore has
    /// several candidates and stays ambiguous; the module path prefix is
    /// unknown without `go.mod`, so only the last element is stated.
    ///
    /// Only domain-rooted paths (`example.com/...`) are recorded: a dotless
    /// path is the standard library or a dotless local module, and a stdlib
    /// name (`errors`, `encoding/json`) is also a common file stem, which
    /// would resolve to the wrong local file.
    fn push_package_import(&mut self, owner: SymbolId, import_path: &str, loc: Location) {
        let Some(name) = Self::package_dir_name(import_path) else {
            return;
        };
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name.clone(), loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module: Some(name),
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
    }
}

/// The names a file's unaliased repository imports bind: `store` for
/// `import "example.com/app/store"`. Code can name such a package by its
/// directory only if its `package` clause uses that name, so the name is
/// the package's. A name an aliased import also binds is left out.
fn repo_packages(root: Node, source: &str) -> HashSet<String> {
    let mut specs = Vec::new();
    for declaration in root.named_children(&mut root.walk()) {
        if declaration.kind() == "import_declaration" {
            collect_import_specs(declaration, &mut specs);
        }
    }
    let mut aliases = HashSet::new();
    let mut packages = HashSet::new();
    for spec in specs {
        if let Some(alias) = spec.child_by_field_name("name") {
            aliases.insert(text(alias, source).to_string());
        } else if let Some(path) = spec.child_by_field_name("path") {
            let path = text(path, source).trim_matches(|c| c == '"' || c == '`');
            packages.extend(Walker::package_dir_name(path));
        }
    }
    packages.retain(|name| !aliases.contains(name));
    packages
}

fn collect_import_specs<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => out.push(child),
            "import_spec_list" => collect_import_specs(child, out),
            _ => {}
        }
    }
}

/// The receiver's type name (`func (r *Invoice) AddItem(...)` → `"Invoice"`),
/// unwrapping the pointer for a pointer receiver — `Invoice` and `*Invoice`
/// methods both attach to the same `parent`, matching how a real Go method
/// set doesn't distinguish them for indexing purposes.
fn receiver_type_name(method_declaration: Node, source: &str) -> Option<String> {
    let receiver = method_declaration.child_by_field_name("receiver")?;
    let mut cursor = receiver.walk();
    let param = receiver
        .children(&mut cursor)
        .find(|c| c.kind() == "parameter_declaration")?;
    let ty = param.child_by_field_name("type")?;
    let mut unwrapped = if ty.kind() == "pointer_type" {
        ty.named_child(0).unwrap_or(ty)
    } else {
        ty
    };
    // `func (s *Stack[T]) Push`: the owner is `Stack`, not `Stack[T]`.
    if unwrapped.kind() == "generic_type" {
        unwrapped = unwrapped.child_by_field_name("type").unwrap_or(unwrapped);
    }
    Some(text(unwrapped, source).to_string())
}

/// The function/method name being invoked: a bare `identifier`, or the
/// `field` of `receiver.Name(...)`.
fn callee_identifier(function: Node) -> Option<Node> {
    match function.kind() {
        "identifier" => Some(function),
        "selector_expression" => function.child_by_field_name("field"),
        // `Map[A, B](xs)`: explicit type arguments wrap the callee.
        "type_instantiation_expression" | "generic_type" => {
            callee_identifier(function.child_by_field_name("type")?)
        }
        "qualified_type" => function.child_by_field_name("name"),
        "type_identifier" => Some(function),
        _ => None,
    }
}

/// The function literal of a `var_spec` that binds one name, with no type,
/// to one `func(…) {…}`.
fn func_value(spec: Node) -> Option<Node> {
    if spec.kind() != "var_spec"
        || spec.child_by_field_name("type").is_some()
        || spec
            .children_by_field_name("name", &mut spec.walk())
            .count()
            != 1
    {
        return None;
    }
    let values = spec.child_by_field_name("value")?;
    if values.named_child_count() != 1 {
        return None;
    }
    values.named_child(0).filter(|v| v.kind() == "func_literal")
}
