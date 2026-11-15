//! `LanguageParser` implementation for Python, via `tree-sitter-python`.
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for Python support.

use std::collections::HashSet;

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location, Supertype};
use tree_sitter::{Node, Parser};

mod details;
#[cfg(test)]
mod libraries;
mod receivers;
mod scope;
mod tokens;

use scope::ModuleScope;

pub use tokens::PythonTokenSource;

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["py", "pyi"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_python::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("tree-sitter-python grammar is statically valid");

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
        walker.scope = ModuleScope::collect(root, &file.contents);
        walker.classes = receivers::class_names(root, &file.contents, &walker.scope);
        walker.attributes = receivers::AttributeFacts::collect(
            root,
            &file.contents,
            &walker.scope,
            &walker.classes,
        );
        let mut module_location = location(root);
        // The root node of a file ending in a newline ends at column 0 of
        // the (empty) line after the last one; the file's last line is the
        // one before that.
        let end = root.end_position();
        if end.column == 0 && end.row > 0 {
            module_location.end_line = Some(end.row as u32);
        }
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location, None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    mct_tree_sitter::module_name(relative_path, &[".pyi", ".py"])
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

#[derive(Default)]
struct Walker<'a> {
    source: &'a str,
    /// This file's repository-relative path, the base of relative imports.
    path: &'a str,
    symbols: Vec<SymbolRecord>,
    /// `(parent, name)` of every symbol recorded so far.
    declared: HashSet<(Option<String>, String)>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
    /// What this module binds, for bare-name evidence.
    scope: ModuleScope,
    /// Declaration details of the symbols recorded so far (ADR-034).
    details: Vec<mct_core::SymbolDetail>,
    /// The candidate supertypes of each enclosing class, innermost last.
    supertypes: Vec<Vec<Supertype>>,
    /// Class names a receiver can be typed by (D5).
    classes: HashSet<String>,
    /// What the file states about attribute types (D5).
    attributes: receivers::AttributeFacts,
    /// The enclosing classes' own methods, innermost last (D5).
    class_scopes: Vec<receivers::ClassScope>,
    /// The enclosing functions' bindings, innermost last (D5).
    frames: Vec<receivers::Frame>,
}

/// The methods `object` defines, the only ones a class with no base can
/// override (CPython 3.14, `dir(object)`).
const OBJECT_METHODS: [&str; 23] = [
    "__class_getitem__",
    "__delattr__",
    "__dir__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__getstate__",
    "__gt__",
    "__hash__",
    "__init__",
    "__init_subclass__",
    "__le__",
    "__lt__",
    "__ne__",
    "__new__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
];

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
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
        self.declared.insert((parent.clone(), name.clone()));
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

    /// `owner` is the innermost enclosing function/method/module (calls and
    /// imports attach to it); `class_name` is the enclosing `class` name, so
    /// a `def` found directly in its body is recorded as a Method with that
    /// parent rather than a bare Function.
    fn visit_children(
        &mut self,
        node: Node,
        owner: SymbolId,
        class_name: Option<&str>,
        depth: u32,
    ) {
        mct_tree_sitter::for_each_child(node, |child| {
            self.visit(child, owner, class_name, depth + 1);
        });
    }

    fn visit(&mut self, node: Node, owner: SymbolId, class_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "decorated_definition" => {
                let definition = node.child_by_field_name("definition");
                let target_id = definition.and_then(|definition| {
                    self.visit_definition(definition, owner, class_name, depth + 1)
                });
                if let (Some(id), Some(definition), Some(_)) = (target_id, definition, class_name) {
                    if definition.kind() == "function_definition" && self.has_override(node) {
                        self.push_overrides(id, definition);
                    }
                }
                let mut cursor = node.walk();
                for decorator in node.children(&mut cursor) {
                    if decorator.kind() == "decorator" {
                        if let Some(expr) = decorator.named_child(0) {
                            if let Some(name) = expr_name(expr, self.source) {
                                // `@cache_page(60)` names `cache_page`.
                                let named = if expr.kind() == "call" {
                                    expr.child_by_field_name("function").unwrap_or(expr)
                                } else {
                                    expr
                                };
                                self.push_use_evidence(named);
                                if !self.reaches_own_attribute(named) {
                                    self.rule_out_field();
                                }
                                self.push_relation(
                                    target_id.unwrap_or(owner),
                                    RelationKind::References,
                                    name,
                                    location(decorator),
                                );
                            }
                            // Decorator arguments run in the enclosing scope:
                            // `@renderer(fmt_name("csv"))` calls `fmt_name`
                            // from `owner`, not from the decorated symbol.
                            if expr.kind() == "call" {
                                if let Some(arguments) = expr.child_by_field_name("arguments") {
                                    self.visit_children(arguments, owner, class_name, depth + 1);
                                }
                            }
                        }
                    }
                }
            }
            "function_definition" | "class_definition" => {
                self.visit_definition(node, owner, class_name, depth);
            }
            // PEP 695 `type Name[T] = ...`.
            "type_alias_statement" => {
                if let Some(name) = node
                    .child_by_field_name("left")
                    .and_then(|left| first_identifier(left, depth))
                {
                    let name = text(name, self.source).to_string();
                    let id = self.push_symbol(
                        name,
                        SymbolKind::TypeAlias,
                        location(node),
                        class_name.map(str::to_string),
                    );
                    self.details
                        .push(details::type_detail(node, id, self.source));
                }
            }
            "import_statement" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.visit_import_name(child, owner);
                }
            }
            "import_from_statement" => {
                let module_child_id = node.child_by_field_name("module_name").map(|n| n.id());
                if let Some(module_node) = node.child_by_field_name("module_name") {
                    self.visit_from_module(module_node, owner);
                }
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if Some(child.id()) == module_child_id {
                        continue;
                    }
                    let module = node
                        .child_by_field_name("module_name")
                        .and_then(|m| from_module(m, self.source));
                    for (name, loc) in import_names(child, self.source) {
                        // `from m import x` names an `x` of module `m`,
                        // whatever else binds `x` later.
                        if let Some(module) = module {
                            self.push_target(RelationTarget {
                                module: Some(module.to_string()),
                                ..Default::default()
                            });
                        }
                        self.push_relation(owner, RelationKind::Imports, name, loc);
                    }
                }
            }
            "call" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some((name, name_node)) = expr_name_node(function, self.source) {
                        if !self.push_receiver_evidence(function) {
                            self.push_use_evidence(function);
                            if !self.reaches_own_attribute(function) {
                                self.rule_out_field();
                            }
                        }
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    }
                    self.visit(function, owner, class_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.push_value_references(arguments, owner, depth + 1);
                    self.visit_children(arguments, owner, class_name, depth + 1);
                }
            }
            "assignment" => {
                // `f = lambda …` declares `f`, and the lambda's body is its.
                let owner = self.push_lambda_binding(node).unwrap_or_else(|| {
                    self.push_bindings(node, depth);
                    owner
                });
                self.push_self_fields(node, depth);
                if let Some(right) = node.child_by_field_name("right") {
                    self.push_value_references(right, owner, depth + 1);
                }
                self.visit_children(node, owner, class_name, depth + 1);
            }
            "return_statement" => {
                if let Some(value) = node.named_child(0) {
                    self.push_value_references(value, owner, depth + 1);
                }
                self.visit_children(node, owner, class_name, depth + 1);
            }
            // A lambda's parameters may shadow the enclosing function's names.
            "lambda" => {
                self.frames.push(receivers::Frame::opaque());
                self.visit_children(node, owner, class_name, depth + 1);
                self.frames.pop();
            }
            _ => self.visit_children(node, owner, class_name, depth + 1),
        }
    }

    /// The names a module-level or class-body `assignment` binds: a
    /// `variable` of the module (a `constant` when annotated `Final`), or a
    /// `field` of the class. Only the first binding of a name the file does
    /// not already declare there counts; `_`, attribute and subscript
    /// targets, and anything inside a function declare nothing.
    fn push_bindings(&mut self, assignment: Node, depth: u32) {
        let Some(scope) = binding_scope(assignment, self.source) else {
            return;
        };
        let annotation = assignment.child_by_field_name("type");
        let annotation_text = annotation.map(|t| text(t, self.source));
        let kind = match scope {
            Some(_) => SymbolKind::Field,
            None if annotation_text.is_some_and(is_final) => SymbolKind::Constant,
            None => SymbolKind::Variable,
        };
        let mut names = Vec::new();
        if let Some(left) = assignment.child_by_field_name("left") {
            target_names(left, &mut names, depth);
        }
        for name_node in names {
            if text(name_node, self.source) != "_" {
                self.push_binding(name_node, kind, scope, annotation_text);
            }
        }
    }

    /// A module-level or class-body `name = lambda …` (one plain name, no
    /// annotation): a `function` of the module or a `method` of the class,
    /// as a JS arrow bound to a `const` is. `None` for any other assignment,
    /// or when the file already declares the name there.
    fn push_lambda_binding(&mut self, assignment: Node) -> Option<SymbolId> {
        let left = assignment.child_by_field_name("left")?;
        let lambda = assignment
            .child_by_field_name("right")
            .filter(|r| r.kind() == "lambda")?;
        if left.kind() != "identifier" || assignment.child_by_field_name("type").is_some() {
            return None;
        }
        let scope = binding_scope(assignment, self.source)?;
        let name = text(left, self.source);
        let parent = scope.map(str::to_string);
        if name == "_" || self.declared.contains(&(parent.clone(), name.to_string())) {
            return None;
        }
        let kind = match scope {
            Some(_) => SymbolKind::Method,
            None => SymbolKind::Function,
        };
        let id = self.push_symbol(name.to_string(), kind, location(left), parent);
        self.details.push(details::lambda_detail(
            assignment,
            lambda,
            scope.is_some(),
            id,
            self.source,
        ));
        Some(id)
    }

    /// The fields `self.x = …` declares in a method: each `x` of an
    /// attribute target on the method's own receiver, unless the class
    /// already declares `x` or defines a method `x`.
    fn push_self_fields(&mut self, assignment: Node, depth: u32) {
        let mut targets = Vec::new();
        if let Some(left) = assignment.child_by_field_name("left") {
            attribute_targets(left, &mut targets, depth);
        }
        let annotation = assignment
            .child_by_field_name("type")
            .map(|t| text(t, self.source));
        for target in targets {
            let (Some(object), Some(attr)) = (
                target.child_by_field_name("object"),
                target.child_by_field_name("attribute"),
            ) else {
                continue;
            };
            let Some((class, methods)) = self
                .frames
                .last()
                .and_then(|frame| frame.self_class(text(object, self.source)))
            else {
                continue;
            };
            if methods.contains(text(attr, self.source)) {
                continue;
            }
            let class = class.to_string();
            self.push_binding(attr, SymbolKind::Field, Some(&class), annotation);
        }
    }

    /// One binding `name_node` of `parent` (`None`: the module), unless the
    /// file already declares that name there. The signature is the name as
    /// written, with its annotation.
    fn push_binding(
        &mut self,
        name_node: Node,
        kind: SymbolKind,
        parent: Option<&str>,
        annotation: Option<&str>,
    ) {
        let name = text(name_node, self.source);
        let parent = parent.map(str::to_string);
        if self.declared.contains(&(parent.clone(), name.to_string())) {
            return;
        }
        let signature = match annotation {
            Some(ty) => format!("{name}: {ty}"),
            None => name.to_string(),
        };
        let id = self.push_symbol(name.to_string(), kind, location(name_node), parent);
        self.details.push(mct_core::SymbolDetail {
            symbol: id,
            signature: Some(signature),
            ..Default::default()
        });
    }

    /// Creates the symbol for a `function_definition`/`class_definition`
    /// node and walks its body, returning the new symbol's id so a wrapping
    /// `decorated_definition` can attach the decorator's `References`
    /// relation to the decorated symbol itself rather than its enclosing
    /// owner. Any other node kind is dispatched to `visit` and yields `None`
    /// (a decorator on something else has no symbol to attach to).
    fn visit_definition(
        &mut self,
        node: Node,
        owner: SymbolId,
        class_name: Option<&str>,
        depth: u32,
    ) -> Option<SymbolId> {
        match node.kind() {
            "function_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let kind = if class_name.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let id =
                    self.push_symbol(name, kind, location(node), class_name.map(str::to_string));
                self.details.push(details::function_detail(
                    node,
                    class_name.is_some(),
                    id,
                    self.source,
                ));
                let class = class_name.and_then(|_| self.class_scopes.last());
                let frame =
                    receivers::Frame::of(node, self.source, class, &self.scope, &self.classes);
                self.frames.push(frame);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, None, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, None, depth + 1);
                }
                self.frames.pop();
                Some(id)
            }
            "class_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                // A class nested directly in another class's body belongs to
                // it, like a method does.
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Class,
                    location(node),
                    class_name.map(str::to_string),
                );
                self.details
                    .push(details::type_detail(node, id, self.source));
                if let Some(superclasses) = node.child_by_field_name("superclasses") {
                    let mut cursor = superclasses.walk();
                    for base in superclasses.named_children(&mut cursor) {
                        if let Some(base_name) = expr_name(base, self.source) {
                            self.push_use_evidence(base);
                            self.push_relation(
                                id,
                                RelationKind::Extends,
                                base_name,
                                location(base),
                            );
                        }
                    }
                }
                let supertypes = class_supertypes(node, self.source);
                self.supertypes.push(supertypes);
                self.class_scopes.push(receivers::ClassScope::of(
                    node,
                    self.source,
                    &self.scope,
                    &self.classes,
                    &self.attributes,
                ));
                // A class body is its own scope: the enclosing function's
                // names are not its bindings.
                self.frames.push(receivers::Frame::default());
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, Some(&name), depth + 1);
                }
                self.frames.pop();
                self.class_scopes.pop();
                self.supertypes.pop();
                Some(id)
            }
            _ => {
                self.visit(node, owner, class_name, depth);
                None
            }
        }
    }

    /// Whether one of the `decorated_definition`'s decorators is PEP 698's
    /// `override`: a bare name only `typing`/`typing_extensions` imports
    /// bind, or `override` on a module alias of one of them.
    fn has_override(&self, decorated: Node) -> bool {
        let mut cursor = decorated.walk();
        let found = decorated
            .children(&mut cursor)
            .filter(|d| d.kind() == "decorator")
            .filter_map(|d| d.named_child(0))
            .any(|expr| match expr.kind() {
                "identifier" => self.scope.is_typing_override(text(expr, self.source)),
                "attribute" => {
                    let attribute = expr.child_by_field_name("attribute");
                    let object = expr.child_by_field_name("object");
                    attribute.is_some_and(|a| text(a, self.source) == "override")
                        && object.is_some_and(|o| {
                            o.kind() == "identifier"
                                && self
                                    .scope
                                    .module_alias(text(o, self.source))
                                    .is_some_and(|m| scope::TYPING_MODULES.contains(&m))
                        })
                }
                _ => false,
            });
        found
    }

    /// One `overrides` relation per candidate supertype of the enclosing
    /// class for the method `definition`.
    fn push_overrides(&mut self, id: SymbolId, definition: Node) {
        let Some(name_node) = definition.child_by_field_name("name") else {
            return;
        };
        let supertypes = self.supertypes.last().cloned().unwrap_or_default();
        mct_tree_sitter::push_overrides(
            &mut self.relations,
            &mut self.relation_targets,
            id,
            text(name_node, self.source),
            location(name_node),
            &supertypes,
            &OBJECT_METHODS,
        );
    }

    /// D5: `x.m()` on an identifier whose type the code states. A class
    /// qualifies the call; a builtin type makes it external. `false` when
    /// nothing is proven, so the call keeps its other evidence.
    fn push_receiver_evidence(&mut self, function: Node) -> bool {
        if function.kind() != "attribute" {
            return false;
        }
        let (Some(object), Some(method)) = (
            function.child_by_field_name("object"),
            function.child_by_field_name("attribute"),
        ) else {
            return false;
        };
        let receiver = match object.kind() {
            "identifier" => {
                let object = text(object, self.source);
                let method = text(method, self.source);
                match self.frames.last() {
                    Some(frame) => frame.receiver(object, method, &self.scope, &self.classes),
                    None => receivers::value_receiver(object, &self.scope, &self.classes),
                }
            }
            "attribute" => {
                let (Some(inner), Some(attr)) = (
                    object.child_by_field_name("object"),
                    object.child_by_field_name("attribute"),
                ) else {
                    return false;
                };
                if inner.kind() != "identifier" {
                    return false;
                }
                self.frames.last().and_then(|frame| {
                    frame.attribute_receiver(text(inner, self.source), text(attr, self.source))
                })
            }
            _ => return false,
        };
        let target = match receiver {
            Some(receivers::Receiver::Class(class)) => RelationTarget {
                qualifier: Some(class),
                ..Default::default()
            },
            Some(receivers::Receiver::Builtin) => RelationTarget {
                external: true,
                ..Default::default()
            },
            None => return false,
        };
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            ..target
        });
        true
    }

    /// Evidence for the relation about to be pushed, when its target is
    /// written as a bare name (`len(x)`, `class A(Exception)`, `@property`):
    /// a builtin nothing in this module binds is external, and a name only
    /// a `def`/`class` of this module binds stays in this file.
    ///
    /// `true` when the name carried evidence.
    /// Whether `name` at `expr` may be a class attribute: some class body
    /// assigns it and `expr` runs in a class body.
    fn is_class_attribute_use(&self, expr: Node, name: &str) -> bool {
        self.scope.is_class_attribute(name)
            && matches!(binding_scope(expr, self.source), Some(Some(_)))
    }

    /// `push_bare_evidence` for a name that is called, decorates or is a
    /// base class. Python reaches another module's name only through an
    /// import, so a bare name no import binds is this file's: one of its
    /// symbols, or a local or builtin, which stays unresolved. A value use
    /// gets no such evidence, so a local is never recorded as a reference.
    fn push_use_evidence(&mut self, expr: Node) {
        if self.push_bare_evidence(expr) || expr.kind() != "identifier" {
            return;
        }
        if self.scope.is_unimported(text(expr, self.source)) {
            self.push_target(RelationTarget {
                path: Some(self.path.to_string()),
                ..Default::default()
            });
        }
    }

    fn push_bare_evidence(&mut self, expr: Node) -> bool {
        if expr.kind() == "attribute" {
            if !self.push_module_attribute_evidence(expr) {
                self.push_value_attribute_evidence(expr);
            }
            return false;
        }
        if expr.kind() != "identifier" {
            return false;
        }
        let name = text(expr, self.source);
        let target = if self.scope.is_builtin(name) {
            RelationTarget {
                external: true,
                ..Default::default()
            }
        } else if self.scope.is_local_function(name) {
            RelationTarget {
                path: Some(self.path.to_string()),
                ..Default::default()
            }
        } else if let Some(module) = self
            .scope
            .imported_from(name)
            .filter(|_| !self.is_class_attribute_use(expr, name))
        {
            RelationTarget {
                module: Some(module.to_string()),
                ..Default::default()
            }
        } else {
            return false;
        };
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            ..target
        });
        true
    }

    /// A function or class used by name as a value — an argument
    /// (`safely(score, x)`, `key=rank`), an element of a tuple, list, set or
    /// dict value (`isinstance(e, (A, B))`, `{"k": rank}`), the right side of
    /// an assignment (`handler = rank`) or a returned value — is a use of
    /// it: a `References` relation, recorded only when the name carries
    /// evidence (this file's `def`, an import, a builtin), so a parameter or
    /// local is never resolved by its name alone.
    fn push_value_references(&mut self, value: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match value.kind() {
            "identifier" => {
                if self.push_bare_evidence(value) {
                    let name = text(value, self.source).to_string();
                    self.push_relation(owner, RelationKind::References, name, location(value));
                }
            }
            "keyword_argument" | "pair" => {
                if let Some(inner) = value.child_by_field_name("value") {
                    self.push_value_references(inner, owner, depth + 1);
                }
            }
            "argument_list"
            | "expression_list"
            | "tuple"
            | "list"
            | "set"
            | "dictionary"
            | "parenthesized_expression" => {
                let mut cursor = value.walk();
                for part in value.named_children(&mut cursor) {
                    self.push_value_references(part, owner, depth + 1);
                }
            }
            _ => {}
        }
    }

    /// `m.f()` where `m` names a module only through an import (`import
    /// a.b as m`, or `import a` then `a.b.f()`): `f` is defined in the last
    /// module of that path.
    fn push_module_attribute_evidence(&mut self, attribute: Node) -> bool {
        let Some(object) = attribute.child_by_field_name("object") else {
            return false;
        };
        let mut segments = Vec::new();
        let mut head = object;
        while head.kind() == "attribute" {
            let Some(name) = head.child_by_field_name("attribute") else {
                return false;
            };
            segments.push(text(name, self.source));
            match head.child_by_field_name("object") {
                Some(inner) => head = inner,
                None => return false,
            }
        }
        if head.kind() != "identifier" {
            return false;
        }
        let Some(path) = self.scope.module_alias(text(head, self.source)) else {
            return false;
        };
        let module = segments
            .first()
            .copied()
            .or_else(|| path.rsplit('.').next())
            .unwrap_or_default();
        if module.is_empty() {
            return false;
        }
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            module: Some(module.to_string()),
            ..Default::default()
        });
        true
    }

    /// `v.f()` where `v` is a value: a call result, a subscript, a literal,
    /// or a parameter or local of the enclosing function that no import
    /// binds and that is no `def`/`class` of this file
    /// (`member.tier.name.title()`). `f` is then a method of a type the
    /// syntax doesn't prove: a member call, never a module-level name or a
    /// class attribute. A name an import binds may be a module
    /// (`os.path.join`), a module-level name may hold one
    /// (`MODULE = importlib.util.module_from_spec(…)`), a class of this file
    /// may hold a nested class (`Outer.Inner()`), and `self` may reach an
    /// inherited method, so none of them proves anything.
    fn push_value_attribute_evidence(&mut self, attribute: Node) {
        let mut head = attribute;
        while head.kind() == "attribute" {
            match head.child_by_field_name("object") {
                Some(inner) => head = inner,
                None => return,
            }
        }
        if head.kind() == "identifier" {
            let name = text(head, self.source);
            // A function's own binding is local whatever `import *` or
            // `globals()` do to module names.
            let local = self.frames.last().is_some_and(|f| f.binds(name));
            if !local || self.scope.is_import_bound(name) || self.scope.defines(name) {
                return;
            }
        }
        self.push_target(RelationTarget {
            member: true,
            ..Default::default()
        });
    }

    /// Whether `function` may call an attribute of this method's own
    /// receiver (`self.handler()`), which can be a field of the class
    /// holding a callable. A bare name (`f()`) is no attribute at all.
    fn reaches_own_attribute(&self, function: Node) -> bool {
        if function.kind() != "attribute" {
            return true;
        }
        function
            .child_by_field_name("object")
            .filter(|object| object.kind() == "identifier")
            .is_some_and(|object| {
                self.frames
                    .last()
                    .is_some_and(|frame| frame.self_class(text(object, self.source)).is_some())
            })
    }

    /// The relation about to be pushed can't target a field: `v.m()` or
    /// `@v.m` on a receiver of unproven type, where a field `m` of some
    /// class matches by name only.
    fn rule_out_field(&mut self) {
        let relation = self.relations.len();
        match self.relation_targets.last_mut() {
            Some(target) if target.relation == relation => target.not_field = true,
            _ => self.push_target(RelationTarget {
                not_field: true,
                ..Default::default()
            }),
        }
    }

    /// `target` for the relation about to be pushed.
    fn push_target(&mut self, target: RelationTarget) {
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            ..target
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

    /// An `Imports` relation to the `Module` symbol of an imported module
    /// file, with the evidence the import statement proves about it.
    fn push_module_import(
        &mut self,
        owner: SymbolId,
        name: String,
        loc: Location,
        module: Option<String>,
    ) {
        if name.is_empty() {
            return;
        }
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name, loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module,
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
    }

    /// `import a.b.c [as x]`: the module `c`, which must live under `b`.
    /// The alias, when present, is still recorded as the local name.
    fn visit_import_name(&mut self, child: Node, owner: SymbolId) {
        let dotted = match child.kind() {
            "dotted_name" => Some(child),
            "aliased_import" => child
                .child_by_field_name("name")
                .filter(|n| n.kind() == "dotted_name"),
            _ => None,
        };
        let aliased = child.kind() == "aliased_import";
        if !aliased {
            // Unaliased: the module itself is the local name.
            if let Some(dotted) = dotted {
                let (name, module, loc) = dotted_parts(dotted, self.source);
                self.push_module_import(owner, name, loc, module);
            }
            return;
        }
        for (name, loc) in import_names(child, self.source) {
            self.push_relation(owner, RelationKind::Imports, name, loc);
        }
        if let Some(dotted) = dotted {
            let (name, module, loc) = dotted_parts(dotted, self.source);
            self.push_module_import(owner, name, loc, module);
        }
    }

    /// `from [.]a.b import …`: the module `b`, which must live under `a`
    /// (or, for `from .b`, under this file's package directory).
    fn visit_from_module(&mut self, module_node: Node, owner: SymbolId) {
        match module_node.kind() {
            "dotted_name" => {
                let (name, module, loc) = dotted_parts(module_node, self.source);
                self.push_module_import(owner, name, loc, module);
            }
            "relative_import" => {
                let mut cursor = module_node.walk();
                let mut dots = 0usize;
                let mut dotted = None;
                for part in module_node.named_children(&mut cursor) {
                    match part.kind() {
                        "import_prefix" => dots = text(part, self.source).matches('.').count(),
                        "dotted_name" => dotted = Some(part),
                        _ => {}
                    }
                }
                // `from . import x` names no module file of its own.
                let Some(dotted) = dotted else { return };
                let (name, module, loc) = dotted_parts(dotted, self.source);
                let module = module.or_else(|| relative_package(self.path, dots));
                self.push_module_import(owner, name, loc, module);
            }
            _ => {}
        }
    }
}

/// `a.b.c` -> (`c`, `Some("b")`, location of `c`).
fn dotted_parts(dotted: Node, source: &str) -> (String, Option<String>, Location) {
    let count = dotted.named_child_count();
    let Some(last) = count.checked_sub(1).and_then(|i| dotted.named_child(i)) else {
        return (String::new(), None, location(dotted));
    };
    let previous = count
        .checked_sub(2)
        .and_then(|i| dotted.named_child(i))
        .map(|n| text(n, source).to_string());
    (text(last, source).to_string(), previous, location(last))
}

/// The directory name `dots` leading dots reach from `path`'s package: one
/// dot is the file's own directory, each further dot climbs one level.
/// `None` at the repository root, where no directory name proves anything.
fn relative_package(path: &str, dots: usize) -> Option<String> {
    let mut dirs: Vec<&str> = path.split('/').collect();
    dirs.pop();
    let keep = dirs.len().checked_sub(dots.saturating_sub(1))?;
    dirs.get(keep.checked_sub(1)?)
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string())
}

/// The first `identifier` at or under `node` in document order — the alias
/// name of a `type` statement's left side (`Pair` in `Pair[T]`).
fn first_identifier(node: Node, depth: u32) -> Option<Node> {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return None;
    }
    if node.kind() == "identifier" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .find_map(|child| first_identifier(child, depth + 1))
}

/// The bases a method of `class` can override: every positional base
/// written as a name (`Base[int]` is `Base`), or the root `object`,
/// external, when the class has no positional base at all. `metaclass=…`
/// is not a base; `make_base()` or `*bases` is one the syntax can't name,
/// so it gets no relation but rules out the root.
fn class_supertypes(class: Node, source: &str) -> Vec<Supertype> {
    let mut supertypes = Vec::new();
    let mut has_base = false;
    if let Some(superclasses) = class.child_by_field_name("superclasses") {
        let mut cursor = superclasses.walk();
        for base in superclasses.named_children(&mut cursor) {
            if matches!(base.kind(), "keyword_argument" | "comment") {
                continue;
            }
            has_base = true;
            let base = if base.kind() == "subscript" {
                base.child_by_field_name("value").unwrap_or(base)
            } else {
                base
            };
            if matches!(base.kind(), "identifier" | "attribute") {
                if let Some(name) = expr_name(base, source) {
                    supertypes.push(Supertype {
                        name,
                        external: false,
                    });
                }
            }
        }
    }
    if !has_base {
        supertypes.push(Supertype {
            name: "object".to_string(),
            external: true,
        });
    }
    supertypes
}

/// Where an assignment binds: `Some(None)` at module level, `Some(class)`
/// in a class body (an `if` or `try` block inside either too), `None`
/// inside a function or lambda.
fn binding_scope<'a>(node: Node, source: &'a str) -> Option<Option<&'a str>> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_definition" | "lambda" => return None,
            "class_definition" => {
                return ancestor
                    .child_by_field_name("name")
                    .map(|n| Some(text(n, source)));
            }
            "module" => return Some(None),
            _ => current = ancestor.parent(),
        }
    }
    None
}

/// The module a `from … import` names by its last segment: `models` for
/// `django.db.models` and `.models`; `None` for `from . import x`.
fn from_module<'a>(module_name: Node, source: &'a str) -> Option<&'a str> {
    let dotted = match module_name.kind() {
        "dotted_name" => module_name,
        "relative_import" => module_name
            .named_children(&mut module_name.walk())
            .find(|c| c.kind() == "dotted_name")?,
        _ => return None,
    };
    let last = dotted.named_child(dotted.named_child_count().checked_sub(1)?)?;
    Some(text(last, source))
}

/// `Final`, `Final[int]`, `typing.Final`.
fn is_final(annotation: &str) -> bool {
    let base = annotation.split('[').next().unwrap_or_default().trim();
    base == "Final" || base.ends_with(".Final")
}

/// The identifiers an assignment target binds: `x`, and each name of
/// `a, b` / `(a, b)` / `[a, b]`. Attribute and subscript targets bind no
/// name of this scope.
fn target_names<'a>(target: Node<'a>, out: &mut Vec<Node<'a>>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match target.kind() {
        "identifier" => out.push(target),
        "pattern_list" | "tuple_pattern" | "list_pattern" => {
            let mut cursor = target.walk();
            for child in target.named_children(&mut cursor) {
                target_names(child, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// The attribute targets of an assignment: `a.x`, and each one of
/// `a.x, a.y` / `(a.x, a.y)` / `[a.x, a.y]`.
fn attribute_targets<'a>(target: Node<'a>, out: &mut Vec<Node<'a>>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match target.kind() {
        "attribute" => out.push(target),
        "pattern_list" | "tuple_pattern" | "list_pattern" => {
            let mut cursor = target.walk();
            for child in target.named_children(&mut cursor) {
                attribute_targets(child, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// Name of a callable/base-class expression: a bare identifier, or the
/// trailing attribute of a dotted access (`module.Class`, `obj.method`).
fn expr_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(text(node, source).to_string()),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|n| text(n, source).to_string()),
        "call" => node
            .child_by_field_name("function")
            .and_then(|n| expr_name(n, source)),
        _ => None,
    }
}

/// Same as [`expr_name`], but also returns the specific node the name came
/// from — never the enclosing `call`, whose start position is shared by
/// every call in a chain (`a.f(x).f(y)`'s outer and inner `call` both start
/// at `a`), which would otherwise make two same-named chained calls collide
/// into one indistinguishable relation row.
fn expr_name_node<'a>(node: Node<'a>, source: &str) -> Option<(String, Node<'a>)> {
    match node.kind() {
        "identifier" => Some((text(node, source).to_string(), node)),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|n| (text(n, source).to_string(), n)),
        "call" => node
            .child_by_field_name("function")
            .and_then(|n| expr_name_node(n, source)),
        _ => None,
    }
}

/// Names introduced by one element of an `import`/`from ... import` clause:
/// a `dotted_name` (last segment), a `wildcard_import` (skipped — no single
/// name), or an `aliased_import` (its alias, or the underlying name).
fn import_names(node: Node, source: &str) -> Vec<(String, Location)> {
    match node.kind() {
        "dotted_name" => {
            let last = node.named_child(node.named_child_count().saturating_sub(1));
            last.map(|n| vec![(text(n, source).to_string(), location(n))])
                .unwrap_or_default()
        }
        "aliased_import" => {
            if let Some(alias) = node.child_by_field_name("alias") {
                vec![(text(alias, source).to_string(), location(alias))]
            } else if let Some(name) = node.child_by_field_name("name") {
                import_names(name, source)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}
