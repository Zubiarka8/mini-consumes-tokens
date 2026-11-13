//! D5: the receiver type of `x.m()` when the code states it, so the call
//! is qualified by that type instead of resolved by `m` alone.
//!
//! Proven, inside one function:
//! - `self.m()` / `cls.m()`: the first parameter of a method that is not a
//!   `@staticmethod`, never rebound, when the class body itself defines `m`
//!   (an inherited `m` lives in a base, which the syntax doesn't name);
//! - a parameter annotated with a class (`cat: Catalogue`), or a local bound
//!   exactly once, to `Catalogue(…)` or by `x: Catalogue = …`;
//! - `T.m()` on a class the file defines or imports (`from a import T`);
//! - `self.attr.m()` when the enclosing class annotates `attr` (`attr: T` in
//!   its body, or `self.attr: T = …` in a method), defines no method or class
//!   named `attr`, and nothing in the file states another type for an
//!   attribute of that name (an annotation, `x.attr = U(…)`, a literal) or
//!   sets attributes dynamically (`setattr`, `__dict__`, `vars`,
//!   `__setattr__`, `__getattr__`).
//!
//! A receiver typed by a builtin container or `str` (`opts: dict`,
//! `rows = []`, `name = "a"`) is the builtin's, so the call is external.
//! A name bound twice, or bound without a stated type (an untyped parameter,
//! a `for` target, `x = f()`), proves nothing and is left as it was.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::scope::ModuleScope;
use crate::text;

/// Builtin types whose methods a receiver of that type calls.
const BUILTIN_TYPES: &[&str] = &[
    "bytearray",
    "bytes",
    "dict",
    "frozenset",
    "list",
    "set",
    "str",
    "tuple",
];

/// Literals whose value is a builtin container or string.
const BUILTIN_LITERALS: &[&str] = &[
    "concatenated_string",
    "dictionary",
    "dictionary_comprehension",
    "list",
    "list_comprehension",
    "set",
    "set_comprehension",
    "string",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Receiver {
    Class(String),
    Builtin,
}

/// Identifiers that set or read attributes by a computed name, so no
/// attribute's type is proven in a file that uses one.
const DYNAMIC_ATTRIBUTES: &[&str] = &[
    "__dict__",
    "__getattr__",
    "__getattribute__",
    "__setattr__",
    "setattr",
    "vars",
];

/// What the whole file states about attribute types, by attribute name.
#[derive(Default)]
pub(crate) struct AttributeFacts {
    /// The file sets attributes by a computed name.
    dynamic: bool,
    /// Every type stated for an attribute of that name: an annotation
    /// (`None` when it names no provable type) or a typed value.
    stated: HashMap<String, Vec<Option<Receiver>>>,
}

impl AttributeFacts {
    pub(crate) fn collect(
        root: Node,
        source: &str,
        scope: &ModuleScope,
        classes: &HashSet<String>,
    ) -> Self {
        let ctx = Ctx {
            source,
            scope,
            classes,
        };
        let mut facts = Self::default();
        facts.walk(root, &ctx, 0);
        facts
    }

    fn walk(&mut self, node: Node, ctx: &Ctx, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.dynamic = true;
            return;
        }
        match node.kind() {
            "identifier" if DYNAMIC_ATTRIBUTES.contains(&text(node, ctx.source)) => {
                self.dynamic = true;
            }
            "assignment" => self.assignment(node, ctx),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child, ctx, depth + 1);
        }
    }

    /// `x.attr = …` anywhere, or `attr = …` directly in a class body.
    fn assignment(&mut self, node: Node, ctx: &Ctx) {
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        let name = match left.kind() {
            "attribute" => left.child_by_field_name("attribute"),
            "identifier" if in_class_body(node) => Some(left),
            _ => None,
        };
        let Some(name) = name else {
            return;
        };
        let annotation = node
            .child_by_field_name("type")
            .map(|ty| ctx.annotation(ty));
        let value = node.child_by_field_name("right").and_then(|v| ctx.value(v));
        if annotation.is_some() || value.is_some() {
            let stated = self
                .stated
                .entry(text(name, ctx.source).to_string())
                .or_default();
            stated.extend(annotation);
            stated.extend(value.map(Some));
        }
    }

    /// The type of `attr` when every type the file states for it is `ty`.
    fn agrees(&self, attr: &str, ty: &Receiver) -> bool {
        !self.dynamic
            && self
                .stated
                .get(attr)
                .is_some_and(|all| all.iter().all(|s| s.as_ref() == Some(ty)))
    }
}

/// Whether the statement holding `assignment` sits directly in a class body.
fn in_class_body(assignment: Node) -> bool {
    assignment
        .parent()
        .filter(|p| p.kind() == "expression_statement")
        .and_then(|p| p.parent())
        .filter(|b| b.kind() == "block")
        .and_then(|b| b.parent())
        .is_some_and(|c| c.kind() == "class_definition")
}

/// The enclosing class of a method: its name, the methods its own body
/// defines, and the attributes whose type it states and the file agrees on.
pub(crate) struct ClassScope {
    pub(crate) name: String,
    pub(crate) methods: HashSet<String>,
    attributes: HashMap<String, Receiver>,
}

impl ClassScope {
    pub(crate) fn of(
        class: Node,
        source: &str,
        scope: &ModuleScope,
        classes: &HashSet<String>,
        facts: &AttributeFacts,
    ) -> Self {
        let ctx = Ctx {
            source,
            scope,
            classes,
        };
        let name = class
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string())
            .unwrap_or_default();
        let mut methods = HashSet::new();
        let mut nested = HashSet::new();
        let mut annotated: HashMap<String, Option<Receiver>> = HashMap::new();
        let mut annotate = |attr: &str, ty: Option<Receiver>| {
            annotated
                .entry(attr.to_string())
                .and_modify(|seen| {
                    if *seen != ty {
                        *seen = None;
                    }
                })
                .or_insert(ty);
        };
        if let Some(body) = class.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                let definition = if child.kind() == "decorated_definition" {
                    child.child_by_field_name("definition")
                } else {
                    Some(child)
                };
                let Some(def) = definition else {
                    continue;
                };
                match def.kind() {
                    "function_definition" => {
                        if let Some(n) = def.child_by_field_name("name") {
                            methods.insert(text(n, source).to_string());
                        }
                        if !is_staticmethod(def, source) {
                            self_annotations(def, &ctx, &mut annotate);
                        }
                    }
                    "class_definition" => {
                        if let Some(n) = def.child_by_field_name("name") {
                            nested.insert(text(n, source).to_string());
                        }
                    }
                    "expression_statement" => {
                        if let Some((attr, ty)) = def
                            .named_child(0)
                            .and_then(|a| annotation_of(a, "identifier", &ctx))
                        {
                            annotate(text(attr, source), ty);
                        }
                    }
                    _ => {}
                }
            }
        }
        let attributes = annotated
            .into_iter()
            .filter_map(|(attr, ty)| Some((attr, ty?)))
            .filter(|(attr, ty)| {
                !methods.contains(attr) && !nested.contains(attr) && facts.agrees(attr, ty)
            })
            .collect();
        Self {
            name,
            methods,
            attributes,
        }
    }
}

/// `target: T …`, an annotated assignment whose left side is a `kind` node:
/// the annotated name and the type it states.
fn annotation_of<'t>(
    assignment: Node<'t>,
    kind: &str,
    ctx: &Ctx,
) -> Option<(Node<'t>, Option<Receiver>)> {
    if assignment.kind() != "assignment" {
        return None;
    }
    let left = assignment
        .child_by_field_name("left")
        .filter(|l| l.kind() == kind)?;
    let ty = assignment.child_by_field_name("type")?;
    Some((left, ctx.annotation(ty)))
}

/// Every `self.attr: T …` in `method`, `self` its first parameter, not
/// entering a nested scope.
fn self_annotations(method: Node, ctx: &Ctx, annotate: &mut impl FnMut(&str, Option<Receiver>)) {
    let first = method
        .child_by_field_name("parameters")
        .and_then(|p| p.named_child(0))
        .filter(|p| p.kind() == "identifier");
    let (Some(first), Some(body)) = (first, method.child_by_field_name("body")) else {
        return;
    };
    let self_name = text(first, ctx.source);
    let mut frame = Frame::default();
    frame.collect(body, ctx, 0);
    if frame.opaque || frame.bindings.contains_key(self_name) {
        return;
    }
    let mut stack = vec![(body, 0u32)];
    while let Some((node, depth)) = stack.pop() {
        if depth >= MAX_TRAVERSAL_DEPTH {
            continue;
        }
        if matches!(
            node.kind(),
            "function_definition" | "class_definition" | "lambda"
        ) {
            continue;
        }
        if let Some((left, ty)) = annotation_of(node, "attribute", ctx) {
            let object = left.child_by_field_name("object");
            let attr = left.child_by_field_name("attribute");
            if let (Some(object), Some(attr)) = (object, attr) {
                if object.kind() == "identifier" && text(object, ctx.source) == self_name {
                    annotate(text(attr, ctx.source), ty);
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push((child, depth + 1));
        }
    }
}

/// What one function's names are bound to. `None`: bound without a stated
/// type, or more than once.
#[derive(Default)]
pub(crate) struct Frame {
    class: Option<(String, HashSet<String>, HashMap<String, Receiver>)>,
    self_name: Option<String>,
    bindings: HashMap<String, Option<Receiver>>,
    opaque: bool,
}

impl Frame {
    /// A scope that proves nothing about its names (a lambda's body, whose
    /// parameters may shadow the enclosing function's).
    pub(crate) fn opaque() -> Self {
        Self {
            opaque: true,
            ..Self::default()
        }
    }

    /// The frame of `function`; `class` is set when it is a method.
    pub(crate) fn of(
        function: Node,
        source: &str,
        class: Option<&ClassScope>,
        scope: &ModuleScope,
        classes: &HashSet<String>,
    ) -> Self {
        let mut frame = Self::default();
        let ctx = Ctx {
            source,
            scope,
            classes,
        };
        let is_static = is_staticmethod(function, source);
        if let Some(params) = function.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            for (i, param) in params.named_children(&mut cursor).enumerate() {
                if i == 0 && class.is_some() && !is_static && param.kind() == "identifier" {
                    frame.self_name = Some(text(param, source).to_string());
                    continue;
                }
                frame.parameter(param, &ctx);
            }
        }
        if let Some(body) = function.child_by_field_name("body") {
            frame.collect(body, &ctx, 0);
        }
        frame.class = class.map(|c| (c.name.clone(), c.methods.clone(), c.attributes.clone()));
        frame
    }

    /// The proven receiver of `object.method()`, an identifier `object`.
    pub(crate) fn receiver(
        &self,
        object: &str,
        method: &str,
        scope: &ModuleScope,
        classes: &HashSet<String>,
    ) -> Option<Receiver> {
        if self.opaque {
            return None;
        }
        if self.is_self(object) {
            let (class, methods, _) = self.class.as_ref()?;
            return methods
                .contains(method)
                .then(|| Receiver::Class(class.clone()));
        }
        match self.bindings.get(object) {
            Some(bound) => bound.clone(),
            None => module_receiver(object, scope, classes),
        }
    }

    /// The proven receiver of `object.attr.method()`: the type the enclosing
    /// class states for `attr`, when `object` is its never-rebound `self`.
    pub(crate) fn attribute_receiver(&self, object: &str, attr: &str) -> Option<Receiver> {
        if self.opaque || !self.is_self(object) {
            return None;
        }
        let (_, _, attributes) = self.class.as_ref()?;
        attributes.get(attr).cloned()
    }

    /// Whether this function binds `name` itself (a parameter or a local),
    /// `self` aside.
    pub(crate) fn binds(&self, name: &str) -> bool {
        !self.opaque && self.bindings.contains_key(name)
    }

    fn is_self(&self, object: &str) -> bool {
        self.self_name.as_deref() == Some(object) && !self.bindings.contains_key(object)
    }

    fn bind(&mut self, name: &str, receiver: Option<Receiver>) {
        if name.is_empty() {
            return;
        }
        match self.bindings.get_mut(name) {
            Some(existing) => *existing = None,
            None => {
                self.bindings.insert(name.to_string(), receiver);
            }
        }
    }

    fn parameter(&mut self, param: Node, ctx: &Ctx) {
        match param.kind() {
            "identifier" => self.bind(text(param, ctx.source), None),
            "typed_parameter" => {
                let mut cursor = param.walk();
                let name = param
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "identifier");
                let ty = param.child_by_field_name("type");
                match name {
                    Some(name) => {
                        let receiver = ty.and_then(|t| ctx.annotation(t));
                        self.bind(text(name, ctx.source), receiver);
                    }
                    // `*args: T`, `**kw: T`: the name is a tuple or dict.
                    None => self.names(param, ctx, 0),
                }
            }
            "typed_default_parameter" => {
                if let Some(name) = param.child_by_field_name("name") {
                    let receiver = param
                        .child_by_field_name("type")
                        .and_then(|t| ctx.annotation(t));
                    self.bind(text(name, ctx.source), receiver);
                }
            }
            _ => self.names(param, ctx, 0),
        }
    }

    /// Every binding in a function body, not entering a nested scope.
    fn collect(&mut self, node: Node, ctx: &Ctx, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(text(name, ctx.source), None);
                }
                return;
            }
            "lambda" => return,
            "assignment" => {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let receiver = match node.child_by_field_name("type") {
                            Some(ty) => ctx.annotation(ty),
                            None => node.child_by_field_name("right").and_then(|v| ctx.value(v)),
                        };
                        self.bind(text(left, ctx.source), receiver);
                    } else {
                        self.targets(left, ctx, depth + 1);
                    }
                }
            }
            "augmented_assignment" | "for_statement" | "for_in_clause" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.targets(left, ctx, depth + 1);
                }
            }
            "named_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(text(name, ctx.source), None);
                }
            }
            "as_pattern" | "except_clause" => {
                if let Some(alias) = node.child_by_field_name("alias") {
                    self.targets(alias, ctx, depth + 1);
                }
            }
            "case_pattern" | "global_statement" | "nonlocal_statement" => {
                self.names(node, ctx, depth + 1);
            }
            "import_statement" | "import_from_statement" => self.names(node, ctx, depth + 1),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.collect(child, ctx, depth + 1);
        }
    }

    /// The names an assignment target binds, each without a stated type.
    fn targets(&mut self, node: Node, ctx: &Ctx, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        match node.kind() {
            "identifier" => self.bind(text(node, ctx.source), None),
            "attribute" | "subscript" => {}
            _ => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.targets(child, ctx, depth + 1);
                }
            }
        }
    }

    /// Every identifier under `node`, each without a stated type
    /// (over-collecting only proves less).
    fn names(&mut self, node: Node, ctx: &Ctx, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        if node.kind() == "identifier" {
            self.bind(text(node, ctx.source), None);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.names(child, ctx, depth + 1);
        }
    }
}

/// `T.m()` outside any function binding `T`: `T` is a class only a
/// `class` statement of this file, or an import, binds.
pub(crate) fn module_receiver(
    object: &str,
    scope: &ModuleScope,
    classes: &HashSet<String>,
) -> Option<Receiver> {
    classes
        .contains(object)
        .then(|| Receiver::Class(object.to_string()))
        .filter(|_| scope.is_local_function(object) || scope.imported_from(object).is_some())
}

/// The class names the file can name a receiver by: every `class`
/// statement's, and every name a `from … import` binds that reads as a class
/// (`Catalogue`, not `CONSTANT` or `helper`).
pub(crate) fn class_names(root: Node, source: &str, scope: &ModuleScope) -> HashSet<String> {
    let mut out = HashSet::new();
    classes_in(root, source, &mut out, 0);
    out.retain(|name| scope.is_local_function(name));
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "import_from_statement" {
            continue;
        }
        let mut inner = child.walk();
        for part in child.named_children(&mut inner) {
            let name = match part.kind() {
                "dotted_name" if Some(part) != child.child_by_field_name("module_name") => {
                    part.named_child(0)
                }
                "aliased_import" => part.child_by_field_name("alias"),
                _ => None,
            };
            if let Some(name) = name.map(|n| text(n, source)) {
                if reads_as_class(name) && scope.imported_from(name).is_some() {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

fn classes_in(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if node.kind() == "class_definition" {
        if let Some(name) = node.child_by_field_name("name") {
            out.insert(text(name, source).to_string());
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        classes_in(child, source, out, depth + 1);
    }
}

fn reads_as_class(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase()) && name.chars().any(|c| c.is_lowercase())
}

fn is_staticmethod(function: Node, source: &str) -> bool {
    let Some(parent) = function
        .parent()
        .filter(|p| p.kind() == "decorated_definition")
    else {
        return false;
    };
    let mut cursor = parent.walk();
    let found = parent
        .children(&mut cursor)
        .filter(|d| d.kind() == "decorator")
        .any(|d| {
            d.named_child(0)
                .is_some_and(|e| text(e, source) == "staticmethod")
        });
    found
}

struct Ctx<'a> {
    source: &'a str,
    scope: &'a ModuleScope,
    classes: &'a HashSet<String>,
}

impl Ctx<'_> {
    /// The receiver an annotation states: a class or a builtin type name.
    fn annotation(&self, ty: Node) -> Option<Receiver> {
        let expr = if ty.kind() == "type" {
            ty.named_child(0)?
        } else {
            ty
        };
        if expr.kind() != "identifier" {
            return None;
        }
        self.type_name(text(expr, self.source))
    }

    /// The receiver a bound value states: `T(…)` or a builtin literal.
    fn value(&self, value: Node) -> Option<Receiver> {
        if BUILTIN_LITERALS.contains(&value.kind()) {
            return Some(Receiver::Builtin);
        }
        if value.kind() != "call" {
            return None;
        }
        let callee = value.child_by_field_name("function")?;
        if callee.kind() != "identifier" {
            return None;
        }
        self.type_name(text(callee, self.source))
    }

    fn type_name(&self, name: &str) -> Option<Receiver> {
        if BUILTIN_TYPES.contains(&name) && self.scope.is_builtin(name) {
            return Some(Receiver::Builtin);
        }
        module_receiver(name, self.scope, self.classes)
    }
}
