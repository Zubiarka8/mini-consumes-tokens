//! AST traversal and symbol ownership shared by the JS, TS and TSX grammars.

use mct_core::{
    Location, ParsedFile, RelationKind, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
    MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::location;
use tree_sitter::Node;

mod exports;
mod imports;

use imports::require_argument;

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

pub(super) struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
        }
    }

    pub(super) fn push_symbol(
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
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    /// Params + body of any function-shaped node (`function_declaration`,
    /// `method_definition`, `arrow_function`, `function_expression`) — an
    /// arrow function's body can be a single expression (`() => foo()`)
    /// rather than a `statement_block`, so the body is dispatched through
    /// `visit` (not `visit_children`) to still catch a bare top-level call.
    /// The enclosing class stops here: a function nested in a method body
    /// belongs to the method, not to the class.
    fn visit_function_like_body(&mut self, function_node: Node, owner: SymbolId, depth: u32) {
        if let Some(params) = function_node.child_by_field_name("parameters") {
            self.visit_children(params, owner, None, depth + 1);
        }
        if let Some(param) = function_node.child_by_field_name("parameter") {
            self.visit_children(param, owner, None, depth + 1);
        }
        if let Some(body) = function_node.child_by_field_name("body") {
            self.visit(body, owner, None, depth + 1);
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
                self.push_symbol(
                    name,
                    SymbolKind::TypeAlias,
                    location(node),
                    type_name.map(str::to_string),
                );
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
                        "identifier" => {
                            let name = text(function, self.source);
                            if name == "require" {
                                if let Some(module) = require_argument(node, self.source) {
                                    self.push_relation(
                                        owner,
                                        RelationKind::Imports,
                                        module,
                                        location(node),
                                    );
                                }
                            } else {
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
                    self.visit(function, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "new_expression" => {
                if let Some(constructor) = node.child_by_field_name("constructor") {
                    if let Some(name_node) = rightmost_name(constructor) {
                        self.push_relation(
                            owner,
                            RelationKind::Calls,
                            text(name_node, self.source).to_string(),
                            location(name_node),
                        );
                    }
                    self.visit(constructor, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "import_statement" => {
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
                // nested JSX; the closing tag produces no second reference.
                self.visit_children(node, owner, type_name, depth + 1);
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

    pub(super) fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            ..Default::default()
        }
    }
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
