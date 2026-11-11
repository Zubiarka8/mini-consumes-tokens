//! D5: the receiver type of `x.m()` when the code states it, so the call
//! is qualified by that type instead of resolved by `m` alone.
//!
//! Proven, inside one function:
//! - `self.m()` / `cls.m()`: the first parameter of a method that is not a
//!   `@staticmethod`, never rebound, when the class body itself defines `m`
//!   (an inherited `m` lives in a base, which the syntax doesn't name);
//! - a parameter annotated with a class (`cat: Catalogue`), or a local bound
//!   exactly once, to `Catalogue(…)` or by `x: Catalogue = …`;
//! - `T.m()` on a class the file defines or imports (`from a import T`).
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

/// The enclosing class of a method: its name and the methods its own body
/// defines.
pub(crate) struct ClassScope {
    pub(crate) name: String,
    pub(crate) methods: HashSet<String>,
}

impl ClassScope {
    pub(crate) fn of(class: Node, source: &str) -> Self {
        let name = class
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string())
            .unwrap_or_default();
        let mut methods = HashSet::new();
        if let Some(body) = class.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                let definition = if child.kind() == "decorated_definition" {
                    child.child_by_field_name("definition")
                } else {
                    Some(child)
                };
                if let Some(def) = definition.filter(|d| d.kind() == "function_definition") {
                    if let Some(n) = def.child_by_field_name("name") {
                        methods.insert(text(n, source).to_string());
                    }
                }
            }
        }
        Self { name, methods }
    }
}

/// What one function's names are bound to. `None`: bound without a stated
/// type, or more than once.
#[derive(Default)]
pub(crate) struct Frame {
    class: Option<(String, HashSet<String>)>,
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
        frame.class = class.map(|c| (c.name.clone(), c.methods.clone()));
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
        if self.self_name.as_deref() == Some(object) && !self.bindings.contains_key(object) {
            let (class, methods) = self.class.as_ref()?;
            return methods
                .contains(method)
                .then(|| Receiver::Class(class.clone()));
        }
        match self.bindings.get(object) {
            Some(bound) => bound.clone(),
            None => module_receiver(object, scope, classes),
        }
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
