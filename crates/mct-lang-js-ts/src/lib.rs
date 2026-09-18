//! `LanguageParser` implementation for JavaScript and TypeScript, via
//! `tree-sitter-javascript` and `tree-sitter-typescript`.
//!
//! One crate, three grammars selected per file extension: `tree-sitter-javascript`
//! for `.js`/`.jsx`/`.mjs`/`.cjs` (its grammar already understands JSX — no
//! separate JS-JSX variant exists), `tree-sitter-typescript`'s
//! `LANGUAGE_TYPESCRIPT` for `.ts`/`.mts`/`.cts`, and its `LANGUAGE_TSX` for
//! `.tsx` (a plain `.ts` parser can't disambiguate `<T>` type-assertion
//! syntax from JSX, hence the separate grammar). All three share the same
//! node-kind vocabulary for plain JS/TS constructs (`function_declaration`,
//! `call_expression`, `class_declaration`, ...), which is what lets a single
//! `Walker` below handle all three without per-grammar branching — only
//! TS-only node kinds (`interface_declaration`, `type_alias_declaration`,
//! `implements_clause`, ...) are extra arms that simply never fire on plain
//! JS input.
//!
//! Reported as one combined `language_id` ("javascript_typescript") rather
//! than two separate parsers: real projects freely mix `.js` and `.ts` files
//! in one module graph, and splitting coverage reporting by grammar would
//! suggest a distinction the rest of the index (symbol/relation lookup by
//! name, language-agnostic) doesn't actually make.
//!
//! JSX/TSX elements are deliberately *not* structurally indexed here (no
//! `SymbolKind`/`RelationKind` for a JSX element, attribute, or component
//! usage) — only the logic inside them (component functions, hooks, event
//! handler calls) is extracted, via the same generic recursion that handles
//! every other unrecognized node kind. Structural HTML/CSS/JSX indexing is
//! deferred to a future session that first extends `mct-core`'s symbol model.
//!
//! CommonJS (`require`/`module.exports`) and ES modules (`import`/`export`)
//! are handled by two independent sets of match arms on the node kind found —
//! both can appear in the same file (a common real-world interop pattern),
//! and neither requires knowing up front which module system a file uses.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct JsTsParser;

impl LanguageParser for JsTsParser {
    fn language_id(&self) -> &'static str {
        "javascript_typescript"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["js", "jsx", "mjs", "cjs", "ts", "mts", "cts", "tsx"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let extension = file.relative_path.rsplit('.').next().unwrap_or_default();
        let language: tree_sitter::Language = match extension {
            "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
            _ => tree_sitter_javascript::LANGUAGE.into(),
        };

        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `language` is chosen above among statically linked
        // JS/TS/TSX grammars compiled into this binary; `set_language` only
        // fails on an ABI mismatch between a grammar and this `tree-sitter`
        // version, which Cargo.lock pins at build time — it never depends on
        // the content of an indexed repo.
        parser
            .set_language(&language)
            .expect("tree-sitter-javascript/typescript grammars are statically valid");

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
    let file_name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    for ext in [".tsx", ".mts", ".cts", ".ts", ".jsx", ".mjs", ".cjs", ".js"] {
        if let Some(stripped) = file_name.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    file_name.to_string()
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

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
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
        });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
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
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
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
    fn visit_function_like_body(&mut self, function_node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if let Some(params) = function_node.child_by_field_name("parameters") {
            self.visit_children(params, owner, type_name, depth + 1);
        }
        if let Some(param) = function_node.child_by_field_name("parameter") {
            self.visit_children(param, owner, type_name, depth + 1);
        }
        if let Some(body) = function_node.child_by_field_name("body") {
            self.visit(body, owner, type_name, depth + 1);
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
                self.visit_function_like_body(node, id, None, depth);
            }
            "class_declaration" | "abstract_class_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), SymbolKind::Class, location(node), type_name.map(str::to_string));
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
                let id = self.push_symbol(name.clone(), SymbolKind::Interface, location(node), type_name.map(str::to_string));
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "extends_type_clause" {
                        let mut c2 = child.walk();
                        for t in child.named_children(&mut c2) {
                            if let Some(name_node) = rightmost_name(t) {
                                self.push_relation(id, RelationKind::Extends, text(name_node, self.source).to_string(), location(t));
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
                self.push_symbol(name, SymbolKind::TypeAlias, location(node), type_name.map(str::to_string));
            }
            "method_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name, SymbolKind::Method, location(node), type_name.map(str::to_string));
                self.visit_function_like_body(node, id, type_name, depth);
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
                    Some(value) if matches!(value.kind(), "arrow_function" | "function_expression") => {
                        let id = self.push_symbol(name, SymbolKind::Method, location(node), type_name.map(str::to_string));
                        self.visit_function_like_body(value, id, type_name, depth);
                    }
                    _ => {
                        self.push_symbol(name, SymbolKind::Field, location(node), type_name.map(str::to_string));
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
                let Some(value) = node.child_by_field_name("value") else { return };
                let name_node = node.child_by_field_name("name").filter(|n| n.kind() == "identifier");
                if let (true, Some(name_node)) = (matches!(value.kind(), "arrow_function" | "function_expression"), name_node) {
                    let id = self.push_symbol(
                        text(name_node, self.source).to_string(),
                        SymbolKind::Function,
                        location(node),
                        type_name.map(str::to_string),
                    );
                    self.visit_function_like_body(value, id, type_name, depth);
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
                self.visit_function_like_body(node, owner, type_name, depth);
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    match function.kind() {
                        "identifier" => {
                            let name = text(function, self.source);
                            if name == "require" {
                                if let Some(module) = require_argument(node, self.source) {
                                    self.push_relation(owner, RelationKind::Imports, module, location(node));
                                }
                            } else {
                                self.push_relation(owner, RelationKind::Calls, name.to_string(), location(function));
                            }
                        }
                        "member_expression" => {
                            if let Some(property) = function.child_by_field_name("property") {
                                // location(property), not location(node): a
                                // chained call (`a.f(x).f(y)`) has its outer
                                // and inner call_expression both start at `a`,
                                // which would make two same-named chained
                                // calls collide into one indistinguishable row.
                                self.push_relation(owner, RelationKind::Calls, text(property, self.source).to_string(), location(property));
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
                        self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
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
            "export_statement" => self.visit_export_statement(node, owner, type_name, depth),
            "assignment_expression" => self.visit_assignment_expression(node, owner, type_name, depth),
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
                            self.push_relation(class_id, RelationKind::Extends, text(name_node, self.source).to_string(), location(value));
                        }
                    }
                }
                "implements_clause" => {
                    let mut c2 = child.walk();
                    for value in child.named_children(&mut c2) {
                        if let Some(name_node) = rightmost_name(value) {
                            self.push_relation(class_id, RelationKind::Implements, text(name_node, self.source).to_string(), location(value));
                        }
                    }
                }
                _ => {
                    if let Some(name_node) = rightmost_name(child) {
                        self.push_relation(class_id, RelationKind::Extends, text(name_node, self.source).to_string(), location(child));
                    }
                }
            }
        }
    }

    /// One `import` statement's clause: a default binding (bare
    /// `identifier`), a namespace binding (`* as ns`), and/or a named-import
    /// list. Each records the *local* bound name as `to_name` (the alias
    /// when present, matching the Python plugin's `import x as y` ->
    /// `"y"` precedent), since that is the name later calls in this file
    /// will actually reference — not the name the source module exports it
    /// under.
    fn visit_import_clause(&mut self, clause: Node, owner: SymbolId) {
        let mut cursor = clause.walk();
        for child in clause.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    self.push_relation(owner, RelationKind::Imports, text(child, self.source).to_string(), location(child));
                }
                "namespace_import" => {
                    if let Some(name_node) = child.named_child(0) {
                        self.push_relation(owner, RelationKind::Imports, text(name_node, self.source).to_string(), location(child));
                    }
                }
                "named_imports" => {
                    let mut c2 = child.walk();
                    for spec in child.named_children(&mut c2) {
                        if spec.kind() != "import_specifier" {
                            continue;
                        }
                        let target = spec.child_by_field_name("alias").or_else(|| spec.child_by_field_name("name"));
                        if let Some(target) = target {
                            if target.kind() == "default" {
                                continue;
                            }
                            self.push_relation(owner, RelationKind::Imports, text(target, self.source).to_string(), location(spec));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `export function foo() {}` / `export class Foo {}` still create their
    /// underlying symbol via the normal declaration visit; `export default
    /// foo` and `export { foo, bar as baz }` reference an *existing* symbol
    /// by name rather than declaring a new one, recorded as `References`
    /// (the same relation the Python plugin uses for decorator references).
    fn visit_export_statement(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if let Some(declaration) = node.child_by_field_name("declaration") {
            self.visit(declaration, owner, type_name, depth + 1);
        }
        if let Some(value) = node.child_by_field_name("value") {
            if let Some(name_node) = rightmost_name(value) {
                self.push_relation(owner, RelationKind::References, text(name_node, self.source).to_string(), location(value));
            } else {
                self.visit(value, owner, type_name, depth + 1);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "export_clause" {
                let mut c2 = child.walk();
                for spec in child.named_children(&mut c2) {
                    if spec.kind() != "export_specifier" {
                        continue;
                    }
                    if let Some(name_node) = spec.child_by_field_name("name") {
                        if name_node.kind() != "default" {
                            self.push_relation(owner, RelationKind::References, text(name_node, self.source).to_string(), location(spec));
                        }
                    }
                }
            }
        }
    }

    /// CommonJS's only way to export: an assignment to `module.exports`,
    /// `module.exports.name`, or `exports.name`. Not a real relation kind of
    /// its own (`RelationKind` has no `Exports`) — a bare re-export of an
    /// existing identifier is a `References`, same as an ES module named
    /// export; a function/arrow assigned directly as the exported value is
    /// recorded as a new `Function` symbol, same treatment as the ESM
    /// arrow-assigned-to-variable case. Anything that isn't one of these
    /// shapes (e.g. a plain reassignment) is just recursed into normally.
    fn visit_assignment_expression(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");
        if let (Some(left), Some(right)) = (left, right) {
            if let Some(member_name) = commonjs_export_target(left, self.source) {
                self.record_commonjs_export(member_name, right, owner, depth);
                return;
            }
        }
        if let Some(right) = right {
            self.visit(right, owner, type_name, depth + 1);
        }
    }

    fn record_commonjs_export(&mut self, member_name: Option<String>, value: Node, owner: SymbolId, depth: u32) {
        match value.kind() {
            "arrow_function" | "function_expression" => match member_name {
                Some(name) => {
                    let id = self.push_symbol(name, SymbolKind::Function, location(value), None);
                    self.visit_function_like_body(value, id, None, depth);
                }
                None => self.visit_function_like_body(value, owner, None, depth),
            },
            "identifier" => {
                self.push_relation(owner, RelationKind::References, text(value, self.source).to_string(), location(value));
            }
            "object" => {
                let mut cursor = value.walk();
                for child in value.named_children(&mut cursor) {
                    match child.kind() {
                        "shorthand_property_identifier" => {
                            self.push_relation(owner, RelationKind::References, text(child, self.source).to_string(), location(child));
                        }
                        "pair" => {
                            let (Some(key), Some(val)) = (child.child_by_field_name("key"), child.child_by_field_name("value")) else {
                                continue;
                            };
                            match val.kind() {
                                "identifier" => {
                                    self.push_relation(owner, RelationKind::References, text(val, self.source).to_string(), location(child));
                                }
                                "arrow_function" | "function_expression" => {
                                    let id = self.push_symbol(text(key, self.source).to_string(), SymbolKind::Function, location(child), None);
                                    self.visit_function_like_body(val, id, None, depth);
                                }
                                _ => {}
                            }
                        }
                        "method_definition" => {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let id = self.push_symbol(text(name_node, self.source).to_string(), SymbolKind::Function, location(child), None);
                                self.visit_function_like_body(child, id, None, depth);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => self.visit(value, owner, None, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
}

/// Innermost identifier-like name of a (possibly dotted/generic/called)
/// type or value expression: `Foo` -> `Foo`, `ns.Foo` -> `Foo`,
/// `Foo<T>` -> `Foo`, `Mixin(Base)` -> `Mixin`. Used for `extends`/
/// `implements` targets and `export default <expr>`.
fn rightmost_name(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" | "type_identifier" | "property_identifier" | "shorthand_property_identifier" => Some(node),
        "member_expression" => node.child_by_field_name("property").and_then(rightmost_name),
        "generic_type" => node.child_by_field_name("name").and_then(rightmost_name),
        "nested_type_identifier" => node.child_by_field_name("name").and_then(rightmost_name),
        "call_expression" => node.child_by_field_name("function").and_then(rightmost_name),
        _ => None,
    }
}

/// Recognizes an assignment target as one of the three CommonJS export
/// shapes: `module.exports = ...` / `exports.NAME = ...` /
/// `module.exports.NAME = ...`. Returns `Some(None)` for the bare
/// whole-module form, `Some(Some(name))` for a named member, `None` if the
/// left-hand side isn't a CommonJS export at all.
fn commonjs_export_target(left: Node, source: &str) -> Option<Option<String>> {
    if left.kind() != "member_expression" {
        return None;
    }
    let object = left.child_by_field_name("object")?;
    let property = left.child_by_field_name("property")?;
    let property_name = text(property, source).to_string();
    match object.kind() {
        "identifier" => {
            let object_name = text(object, source);
            if object_name == "module" && property_name == "exports" {
                Some(None)
            } else if object_name == "exports" {
                Some(Some(property_name))
            } else {
                None
            }
        }
        "member_expression" => {
            let inner_object = object.child_by_field_name("object")?;
            let inner_property = object.child_by_field_name("property")?;
            if inner_object.kind() == "identifier"
                && text(inner_object, source) == "module"
                && text(inner_property, source) == "exports"
            {
                Some(Some(property_name))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `require('module_name')`'s argument: the literal module path, or `None`
/// for a non-literal argument (`require(computedPath())`) — nothing static
/// to record in that case.
fn require_argument(call_node: Node, source: &str) -> Option<String> {
    let arguments = call_node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    if first.kind() != "string" {
        return None;
    }
    let mut string_cursor = first.walk();
    let fragment = first
        .named_children(&mut string_cursor)
        .find(|c| c.kind() == "string_fragment");
    fragment.map(|c| text(c, source).to_string())
}
