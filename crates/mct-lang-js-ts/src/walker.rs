//! AST traversal and symbol ownership shared by the JS, TS and TSX grammars.

use mct_core::{
    Location, ParsedFile, RelationKind, RelationTarget, SymbolId, SymbolKind, SymbolRecord,
    SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::location;
use std::collections::HashSet;
use tree_sitter::Node;

mod exports;
mod globals;
mod imports;

use globals::FileGlobals;
use imports::{require_argument, string_value};

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

pub(super) struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    /// Evidence for the file-level `Imports` relations (see `imports.rs`).
    relation_targets: Vec<RelationTarget>,
    /// This file's repository-relative path, the base of relative imports.
    path: &'a str,
    next_id: SymbolId,
    /// Identifiers read as values (`onclick: toggle`, `.map(card)`,
    /// `return wrapped`), as `(owner, name, location)`. [`Walker::finish`]
    /// keeps those naming a function of this file as `References`.
    value_uses: Vec<(SymbolId, String, Location)>,
    /// Locals bound by the enclosing functions (parameters, `let`/`const`
    /// values that are not functions, patterns): a value use of one of these
    /// names reads the local, never the same-named function.
    bound: HashSet<String>,
    /// TS type parameters declared by the enclosing declarations: a type
    /// named like one of them is that parameter, not a type to record.
    generics: HashSet<String>,
    /// The ECMAScript globals this file never binds.
    globals: FileGlobals,
}

impl<'a> Walker<'a> {
    pub(super) fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
            path,
            next_id: 0,
            value_uses: Vec::new(),
            bound: HashSet::new(),
            generics: HashSet::new(),
            globals: FileGlobals::default(),
        }
    }

    /// Reads what the whole file binds, before the walk starts, so a call
    /// to a global it never binds can be proven external.
    pub(super) fn scan_globals(&mut self, root: Node) {
        self.globals = FileGlobals::collect(root, self.source);
    }

    /// Marks the relation about to be pushed external when `name` (written
    /// bare, or as the object of `name.f(…)`) is an unbound global.
    fn mark_global(&mut self, name: Node) {
        if name.kind() == "identifier" && self.globals.is_global(text(name, self.source)) {
            self.relation_targets.push(RelationTarget {
                relation: self.relations.len(),
                external: true,
                ..Default::default()
            });
        }
    }

    pub(super) fn push_symbol(
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

    /// `owner` is the innermost enclosing function/method/module (calls and
    /// imports attach to it); `type_name` is the innermost enclosing
    /// class/interface name, used as the `parent` of members declared
    /// directly inside its body.
    pub(super) fn visit_children(
        &mut self,
        node: Node,
        owner: SymbolId,
        type_name: Option<&str>,
        depth: u32,
    ) {
        mct_tree_sitter::for_each_child(node, |child| {
            self.visit(child, owner, type_name, depth + 1);
        });
    }

    /// Params + body of any function-shaped node (`function_declaration`,
    /// `method_definition`, `arrow_function`, `function_expression`) — an
    /// arrow function's body can be a single expression (`() => foo()`)
    /// rather than a `statement_block`, so the body is dispatched through
    /// `visit` (not `visit_children`) to still catch a bare top-level call.
    /// The enclosing class stops here: a function nested in a method body
    /// belongs to the method, not to the class.
    ///
    /// The function's locals join the enclosing functions' for its subtree:
    /// a JS closure sees the locals around it.
    fn visit_function_like_body(&mut self, function_node: Node, owner: SymbolId, depth: u32) {
        let mut locals = self.bound.clone();
        for field in ["parameters", "parameter", "body"] {
            if let Some(part) = function_node.child_by_field_name(field) {
                collect_bindings(part, self.source, &mut locals, depth + 1);
            }
        }
        let outer = std::mem::replace(&mut self.bound, locals);
        if let Some(params) = function_node.child_by_field_name("parameters") {
            self.visit_children(params, owner, None, depth + 1);
        }
        if let Some(param) = function_node.child_by_field_name("parameter") {
            self.visit_children(param, owner, None, depth + 1);
        }
        self.visit_part(function_node, "return_type", owner, depth);
        if let Some(body) = function_node.child_by_field_name("body") {
            self.visit(body, owner, None, depth + 1);
        }
        self.bound = outer;
    }

    /// `depth` bounds native stack usage against adversarially deep/nested
    /// input (see `MAX_TRAVERSAL_DEPTH`) — every recursive call below passes
    /// `depth + 1`, and this early-return prunes the subtree instead of
    /// recursing further once the ceiling is hit.
    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        // A declaration's type parameters (`<T extends Base>`) are in scope
        // for its whole subtree, and only there.
        let outer_generics = self.enter_generics(node);
        self.visit_node(node, owner, type_name, depth);
        if let Some(outer) = outer_generics {
            self.generics = outer;
        }
    }

    /// Adds the type parameters `node` declares to `generics` and returns the
    /// previous set to restore, or `None` when it declares none.
    fn enter_generics(&mut self, node: Node) -> Option<HashSet<String>> {
        let params = node.child_by_field_name("type_parameters")?;
        let outer = self.generics.clone();
        mct_tree_sitter::for_each_child(params, |param| {
            if let Some(name) = param.child_by_field_name("name") {
                self.generics.insert(text(name, self.source).to_string());
            }
        });
        Some(outer)
    }

    /// Visits the `field` child of `node` (a type annotation, type
    /// arguments, an alias's value), when present.
    fn visit_part(&mut self, node: Node, field: &str, owner: SymbolId, depth: u32) {
        if let Some(part) = node.child_by_field_name(field) {
            self.visit(part, owner, None, depth + 1);
        }
    }

    fn visit_node(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        match node.kind() {
            "function_declaration" | "generator_function_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Function, location(node), None);
                self.visit_function_like_body(node, id, depth);
            }
            "class_declaration" | "abstract_class_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Class,
                    location(node),
                    type_name.map(str::to_string),
                );
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "class_heritage" {
                        self.visit_class_heritage(child, id);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "interface_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Interface,
                    location(node),
                    type_name.map(str::to_string),
                );
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "extends_type_clause" {
                        let mut c2 = child.walk();
                        for t in child.named_children(&mut c2) {
                            if let Some(name_node) = rightmost_name(t) {
                                self.push_relation(
                                    id,
                                    RelationKind::Extends,
                                    text(name_node, self.source).to_string(),
                                    location(t),
                                );
                            }
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "type_alias_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name,
                    SymbolKind::TypeAlias,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.visit_part(node, "value", id, depth);
            }
            "enum_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(
                    name,
                    SymbolKind::Enum,
                    location(node),
                    type_name.map(str::to_string),
                );
            }
            // `namespace Legacy { … }`: a symbol of its own, but its members
            // stay top-level (owner and parent unchanged).
            "internal_module" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(name, SymbolKind::Module, location(node), None);
                self.visit_children(node, owner, type_name, depth + 1);
            }
            "method_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                // An object literal is not a type: its methods are functions,
                // as in `record_commonjs_export`.
                let kind = match node.parent().map(|p| p.kind()) {
                    Some("object") => SymbolKind::Function,
                    _ => SymbolKind::Method,
                };
                let id =
                    self.push_symbol(name, kind, location(node), type_name.map(str::to_string));
                self.visit_function_like_body(node, id, depth);
            }
            // JS's `field_definition` vs TS's `public_field_definition` —
            // otherwise identical shape (`property`/`name` field, optional
            // `value`). An arrow function or function expression as the
            // value is the common React-style bound-method-as-field
            // pattern (`handleClick = () => {...}`) and is recorded as a
            // Method, not a Field with an opaque value.
            "field_definition" | "public_field_definition" => {
                let name_node = node
                    .child_by_field_name("property")
                    .or_else(|| node.child_by_field_name("name"));
                let Some(name_node) = name_node else { return };
                let name = text(name_node, self.source).to_string();
                self.visit_part(node, "type", owner, depth);
                match node.child_by_field_name("value") {
                    Some(value)
                        if matches!(value.kind(), "arrow_function" | "function_expression") =>
                    {
                        let id = self.push_symbol(
                            name,
                            SymbolKind::Method,
                            location(node),
                            type_name.map(str::to_string),
                        );
                        self.visit_function_like_body(value, id, depth);
                    }
                    _ => {
                        self.push_symbol(
                            name,
                            SymbolKind::Field,
                            location(node),
                            type_name.map(str::to_string),
                        );
                    }
                }
            }
            // `const foo = () => {...}` / `const foo = function() {...}` —
            // the arrow-function-assigned-to-variable pattern, analogous to
            // the Lua plugin's `local x = function() end` handling. Any
            // other value (including a destructuring pattern on the left,
            // e.g. `const { add } = require(...)`) just gets recursed into
            // normally, so a `require(...)` call on the right is still found.
            "variable_declarator" => {
                self.visit_part(node, "type", owner, depth);
                let Some(value) = node.child_by_field_name("value") else {
                    return;
                };
                let name_node = node
                    .child_by_field_name("name")
                    .filter(|n| n.kind() == "identifier");
                if let (true, Some(name_node)) = (
                    matches!(value.kind(), "arrow_function" | "function_expression"),
                    name_node,
                ) {
                    let id = self.push_symbol(
                        text(name_node, self.source).to_string(),
                        SymbolKind::Function,
                        location(node),
                        type_name.map(str::to_string),
                    );
                    self.visit_function_like_body(value, id, depth);
                    return;
                }
                self.visit(value, owner, type_name, depth + 1);
            }
            // A function/arrow not caught by the more specific cases above
            // (variable_declarator, field_definition, export value,
            // CommonJS export assignment) — e.g. an inline callback
            // (`setTimeout(function() {...})`) or a JSX event handler
            // (`onClick={() => foo()}`). No symbol to record, but calls
            // inside it still attach to whatever function currently owns
            // this scope.
            "arrow_function" | "function_expression" | "generator_function" => {
                self.visit_function_like_body(node, owner, depth);
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    match function.kind() {
                        // Dynamic `import("./x")`: a literal path is a
                        // static dependency like a `require`.
                        "import" => {
                            if let Some(spec) = require_argument(node, self.source) {
                                self.push_module_import(owner, &spec, location(node));
                            }
                        }
                        "identifier" => {
                            let name = text(function, self.source);
                            if name == "require" {
                                if let Some(module) = require_argument(node, self.source) {
                                    // A builtin is recorded once, as external,
                                    // under its own spec name.
                                    let named_by_spec =
                                        self.push_module_import(owner, &module, location(node));
                                    if !named_by_spec {
                                        self.push_relation(
                                            owner,
                                            RelationKind::Imports,
                                            module,
                                            location(node),
                                        );
                                    }
                                }
                            } else {
                                self.mark_global(function);
                                self.push_relation(
                                    owner,
                                    RelationKind::Calls,
                                    name.to_string(),
                                    location(function),
                                );
                            }
                        }
                        "member_expression" => {
                            if let Some(property) = function.child_by_field_name("property") {
                                // location(property), not location(node): a
                                // chained call (`a.f(x).f(y)`) has its outer
                                // and inner call_expression both start at `a`,
                                // which would make two same-named chained
                                // calls collide into one indistinguishable row.
                                if let Some(object) = function.child_by_field_name("object") {
                                    self.mark_global(object);
                                }
                                self.push_relation(
                                    owner,
                                    RelationKind::Calls,
                                    text(property, self.source).to_string(),
                                    location(property),
                                );
                            }
                        }
                        _ => {}
                    }
                    // A bare callee is already the `Calls` above, not a value.
                    if function.kind() != "identifier" {
                        self.visit(function, owner, type_name, depth + 1);
                    }
                }
                self.visit_part(node, "type_arguments", owner, depth);
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "new_expression" => {
                if let Some(constructor) = node.child_by_field_name("constructor") {
                    if let Some(name_node) = rightmost_name(constructor) {
                        self.mark_global(constructor);
                        self.push_relation(
                            owner,
                            RelationKind::Calls,
                            text(name_node, self.source).to_string(),
                            location(name_node),
                        );
                    }
                    if constructor.kind() != "identifier" {
                        self.visit(constructor, owner, type_name, depth + 1);
                    }
                }
                self.visit_part(node, "type_arguments", owner, depth);
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "import_statement" => {
                if let Some(spec) = node
                    .child_by_field_name("source")
                    .and_then(|source| string_value(source, self.source))
                {
                    self.push_module_import(owner, &spec, location(node));
                }
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "import_clause" {
                        self.visit_import_clause(child, owner);
                    }
                }
            }
            "jsx_opening_element" | "jsx_self_closing_element" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let component = text(name, self.source);
                    // Lowercase single names and namespaced names denote
                    // intrinsic/custom DOM elements. Member expressions are
                    // component values even with a lowercase namespace.
                    let is_component = name.kind() == "member_expression"
                        || (name.kind() == "identifier"
                            && component
                                .chars()
                                .next()
                                .is_some_and(|first| !first.is_ascii_lowercase()));
                    if is_component {
                        // Retain qualification: UI.Button must not resolve
                        // to an unrelated local Button by discarding UI.
                        self.push_relation(
                            owner,
                            RelationKind::References,
                            component.to_string(),
                            location(name),
                        );
                    }
                }
                // Attribute expressions still contain ordinary calls and
                // nested JSX; neither the tag name nor the closing tag
                // produces a second reference.
                let name = node.child_by_field_name("name").map(|n| n.id());
                mct_tree_sitter::for_each_child(node, |child| {
                    if Some(child.id()) != name {
                        self.visit(child, owner, type_name, depth + 1);
                    }
                });
            }
            // An identifier reaching here is read as a value: callees,
            // declared names and import/export names are handled by their
            // own arms. Bindings (parameters, patterns, `catch (e)`) are not.
            "identifier" | "shorthand_property_identifier" => {
                let name = text(node, self.source);
                if !is_binding(node) && !self.bound.contains(name) {
                    self.value_uses
                        .push((owner, name.to_string(), location(node)));
                }
            }
            "jsx_closing_element" => {}
            // A TS type named in an annotation, generic argument, alias or
            // `as` cast (`props: ReviewPanelProps`, `useState<Problem>`) is a
            // use of that type. Declared names are read by their own arms.
            "type_identifier" => {
                let name = text(node, self.source);
                if !self.generics.contains(name) {
                    self.push_relation(
                        owner,
                        RelationKind::References,
                        name.to_string(),
                        location(node),
                    );
                }
            }
            "export_statement" => self.visit_export_statement(node, owner, type_name, depth),
            "assignment_expression" => {
                self.visit_assignment_expression(node, owner, type_name, depth)
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    /// `class Foo extends Base implements A, B` — in JS grammar
    /// `class_heritage` wraps the superclass expression directly (JS has no
    /// `implements`); in TS grammar it wraps `extends_clause`/
    /// `implements_clause` children instead. Both shapes are handled here
    /// since either can appear depending on which grammar parsed the file.
    fn visit_class_heritage(&mut self, heritage: Node, class_id: SymbolId) {
        let mut cursor = heritage.walk();
        for child in heritage.named_children(&mut cursor) {
            match child.kind() {
                "extends_clause" => {
                    let mut c2 = child.walk();
                    for value in child.named_children(&mut c2) {
                        if value.kind() == "type_arguments" {
                            continue;
                        }
                        if let Some(name_node) = rightmost_name(value) {
                            self.push_relation(
                                class_id,
                                RelationKind::Extends,
                                text(name_node, self.source).to_string(),
                                location(value),
                            );
                        }
                    }
                }
                "implements_clause" => {
                    let mut c2 = child.walk();
                    for value in child.named_children(&mut c2) {
                        if let Some(name_node) = rightmost_name(value) {
                            self.push_relation(
                                class_id,
                                RelationKind::Implements,
                                text(name_node, self.source).to_string(),
                                location(value),
                            );
                        }
                    }
                }
                _ => {
                    if let Some(name_node) = rightmost_name(child) {
                        self.push_relation(
                            class_id,
                            RelationKind::Extends,
                            text(name_node, self.source).to_string(),
                            location(child),
                        );
                    }
                }
            }
        }
    }

    /// Keeps the value uses that name a function declared in this file,
    /// except a function's use of its own name (`setTimeout(tick)` inside
    /// `tick`), which does not make it reachable.
    pub(super) fn finish(mut self) -> ParsedFile {
        let functions: HashSet<String> = self
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| s.name.clone())
            .collect();
        for (owner, name, loc) in std::mem::take(&mut self.value_uses) {
            let own_name = self
                .symbols
                .get(owner as usize)
                .is_some_and(|s| s.name == name);
            if functions.contains(&name) && !own_name {
                self.push_relation(owner, RelationKind::References, name, loc);
            }
        }
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            ..Default::default()
        }
    }
}

/// True when `node` (an identifier) declares a local instead of reading a
/// value: a parameter, a destructuring target, a `catch` parameter, a
/// declarator's name or a `for … in/of` variable.
fn is_binding(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let is_field = |field: &str| {
        parent
            .child_by_field_name(field)
            .is_some_and(|n| n.id() == node.id())
    };
    match parent.kind() {
        "formal_parameters" | "array_pattern" | "object_pattern" | "rest_pattern" => true,
        "required_parameter" | "optional_parameter" => is_field("pattern"),
        "assignment_pattern" => is_field("left"),
        "pair_pattern" => is_field("value"),
        "arrow_function" => is_field("parameter"),
        "catch_clause" => is_field("parameter"),
        "variable_declarator" => is_field("name"),
        "for_in_statement" => is_field("left"),
        _ => false,
    }
}

/// Adds the locals `node` binds to `out`, without entering nested functions
/// or classes (their locals are their own). A declarator whose value is a
/// function (`const away = () => {}`) declares a `Function` symbol, not a
/// local that hides one.
fn collect_bindings(node: Node, source: &str, out: &mut HashSet<String>, depth: u32) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    match node.kind() {
        "identifier" if is_binding(node) && !declares_function(node) => {
            out.insert(text(node, source).to_string());
        }
        "shorthand_property_identifier_pattern" => {
            out.insert(text(node, source).to_string());
        }
        "arrow_function"
        | "function_expression"
        | "function_declaration"
        | "generator_function"
        | "generator_function_declaration"
        | "method_definition"
        | "class_declaration"
        | "class" => {}
        _ => mct_tree_sitter::for_each_child(node, |child| {
            collect_bindings(child, source, out, depth + 1);
        }),
    }
}

/// True for the name of `const name = () => {}` / `= function () {}`.
fn declares_function(name: Node) -> bool {
    name.parent().is_some_and(|parent| {
        parent.kind() == "variable_declarator"
            && parent
                .child_by_field_name("value")
                .is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression"))
    })
}

/// Innermost identifier-like name of a (possibly dotted/generic/called)
/// type or value expression: `Foo` -> `Foo`, `ns.Foo` -> `Foo`,
/// `Foo<T>` -> `Foo`, `Mixin(Base)` -> `Mixin`. Used for `extends`/
/// `implements` targets and `export default <expr>`.
fn rightmost_name(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier"
        | "type_identifier"
        | "property_identifier"
        | "shorthand_property_identifier" => Some(node),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(rightmost_name),
        "generic_type" => node.child_by_field_name("name").and_then(rightmost_name),
        "nested_type_identifier" => node.child_by_field_name("name").and_then(rightmost_name),
        "call_expression" => node
            .child_by_field_name("function")
            .and_then(rightmost_name),
        _ => None,
    }
}
