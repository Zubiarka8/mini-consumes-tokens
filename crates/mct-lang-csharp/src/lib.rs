//! `LanguageParser` implementation for C#, via `tree-sitter-c-sharp`.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use tree_sitter::{Node, Parser};

pub struct CSharpParser;

impl LanguageParser for CSharpParser {
    fn language_id(&self) -> &'static str {
        "csharp"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["cs"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_c_sharp::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("tree-sitter-c-sharp grammar is statically valid");

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
        let mut walker = Walker::new(&file.contents, file_end_line(root));
        let mut module_location = location(root);
        module_location.end_line = Some(walker.file_end);
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, module_location, None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".cs")
        .to_string()
}

/// The file's last line. The root node of a file ending in a newline ends
/// at column 0 of the (empty) line after the last one.
fn file_end_line(root: Node) -> u32 {
    let end = root.end_position();
    if end.column == 0 && end.row > 0 {
        end.row as u32
    } else {
        end.row as u32 + 1
    }
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
    file_end: u32,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, file_end: u32) -> Self {
        Self {
            source,
            file_end,
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

    fn field_text(&self, node: Node, field: &str) -> String {
        node.child_by_field_name(field)
            .map(|n| text(n, self.source).to_string())
            .unwrap_or_default()
    }

    /// `owner` is the innermost enclosing symbol calls attach to (a
    /// member, a type for anything directly in its body, or the file
    /// module); `type_name` is the innermost enclosing
    /// class/interface/struct/record/enum/namespace name, used as `parent`
    /// for members declared directly inside it. Every `method_declaration`
    /// — including each overload — gets its own `SymbolRecord` row, so
    /// overloads are never collapsed.
    ///
    /// A file-scoped `namespace X;` has no body: the declarations after it
    /// are its siblings, so it becomes their `type_name` from here on.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut scope: Option<String> = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "file_scoped_namespace_declaration" {
                let name = self.field_text(child, "name");
                let mut loc = location(child);
                loc.end_line = Some(self.file_end);
                self.push_symbol(
                    name.clone(),
                    SymbolKind::Module,
                    loc,
                    type_name.map(str::to_string),
                );
                scope = Some(name);
                continue;
            }
            self.visit(child, owner, scope.as_deref().or(type_name), depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        let parent = type_name.map(str::to_string);
        match node.kind() {
            "namespace_declaration" => {
                let name = self.field_text(node, "name");
                self.push_symbol(name.clone(), SymbolKind::Module, location(node), parent);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "class_declaration"
            | "interface_declaration"
            | "struct_declaration"
            | "record_declaration"
            | "enum_declaration" => {
                self.visit_type(node, type_name, depth);
            }
            "delegate_declaration" => {
                let name = self.field_text(node, "name");
                let id = self.push_symbol(name, SymbolKind::TypeAlias, location(node), parent);
                self.visit_attributes(node, id, type_name, depth);
            }
            "method_declaration"
            | "constructor_declaration"
            | "destructor_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration" => {
                let name = match node.kind() {
                    "destructor_declaration" => format!("~{}", self.field_text(node, "name")),
                    "operator_declaration" => {
                        format!("operator {}", self.field_text(node, "operator"))
                    }
                    // `implicit operator Result<T>` → `operator Result`.
                    "conversion_operator_declaration" => {
                        let target = node
                            .child_by_field_name("type")
                            .and_then(|t| type_base_name(t, self.source))
                            .unwrap_or_else(|| self.field_text(node, "type"));
                        format!("operator {target}")
                    }
                    _ => self.field_text(node, "name"),
                };
                let id = self.push_symbol(name, SymbolKind::Method, location(node), parent);
                self.visit_callable(node, id, type_name, depth);
            }
            // A local function (in a method body or among top-level
            // statements) is a plain function with no owning type; its
            // calls attach to it, not to the enclosing method.
            "local_function_statement" => {
                let name = self.field_text(node, "name");
                let id = self.push_symbol(name, SymbolKind::Function, location(node), None);
                self.visit_callable(node, id, None, depth);
            }
            // A property (`Name { get; set; }`) is ONE symbol, not two —
            // the accessors are walked into using the property's own id as
            // owner, so a custom getter/setter's calls attach to the
            // property, and get/set are never indexed as separate
            // unrelated methods. Same for an indexer (named `this`) and an
            // event with `add`/`remove` accessors. An expression body
            // (`=> …`) or initializer (`= …`) is the `value` field.
            "property_declaration" | "indexer_declaration" | "event_declaration" => {
                let name = if node.kind() == "indexer_declaration" {
                    "this".to_string()
                } else {
                    self.field_text(node, "name")
                };
                let id = self.push_symbol(name, SymbolKind::Field, location(node), parent);
                self.visit_attributes(node, id, type_name, depth);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(accessors) = node.child_by_field_name("accessors") {
                    let mut cursor = accessors.walk();
                    for accessor in accessors.children(&mut cursor) {
                        if let Some(body) = accessor.child_by_field_name("body") {
                            self.visit_children(body, id, type_name, depth + 1);
                        }
                    }
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, id, type_name, depth + 1);
                }
            }
            // One field symbol per declarator (`int a, b;`); an
            // initializer's calls attach to its field.
            "field_declaration" | "event_field_declaration" => {
                if let Some(variable_declaration) = find_child(node, "variable_declaration") {
                    let mut cursor = variable_declaration.walk();
                    let declarators: Vec<Node> = variable_declaration
                        .children(&mut cursor)
                        .filter(|n| n.kind() == "variable_declarator")
                        .collect();
                    for declarator in declarators {
                        let Some(name_node) = declarator.child_by_field_name("name") else {
                            continue;
                        };
                        let id = self.push_symbol(
                            text(name_node, self.source).to_string(),
                            SymbolKind::Field,
                            location(declarator),
                            parent.clone(),
                        );
                        self.visit_attributes(node, id, type_name, depth);
                        let mut cursor = declarator.walk();
                        for child in declarator.named_children(&mut cursor) {
                            if child.id() != name_node.id() {
                                self.visit(child, id, type_name, depth + 1);
                            }
                        }
                    }
                }
            }
            "using_directive" => {
                if let Some(path) = node.named_child(0) {
                    if let Some(last) = last_identifier(path) {
                        self.push_relation(
                            owner,
                            RelationKind::Imports,
                            text(last, self.source).to_string(),
                            location(node),
                        );
                    }
                }
            }
            // `[Name(args)]` refers to the class `NameAttribute` (C#
            // resolves the suffixed name first), so that is the target;
            // calls in the arguments attach to the attributed symbol.
            "attribute" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| type_base_name(n, self.source))
                {
                    let target = if name.ends_with("Attribute") {
                        name
                    } else {
                        format!("{name}Attribute")
                    };
                    self.push_relation(owner, RelationKind::References, target, location(node));
                }
                if let Some(args) = find_child(node, "attribute_argument_list") {
                    self.visit_children(args, owner, type_name, depth + 1);
                }
            }
            "invocation_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if let Some(name_node) = callee_identifier(function) {
                        let name = text(name_node, self.source);
                        // `nameof(x)` is an operator, not a call.
                        if !(name == "nameof" && function.kind() == "identifier") {
                            // location(name_node), not location(node): a
                            // chained call (`a.F(x).F(y)`) has its outer and
                            // inner invocation_expression both start at `a`,
                            // which would make two same-named chained calls
                            // collide into one indistinguishable row.
                            self.push_relation(
                                owner,
                                RelationKind::Calls,
                                name.to_string(),
                                location(name_node),
                            );
                        }
                    }
                    self.visit(function, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            // `new T(…)` calls T's constructor: recorded as a call to the
            // type's bare name, as `T(…)` is in languages without `new`.
            "object_creation_expression" => {
                if let Some(type_node) = node.child_by_field_name("type") {
                    if let Some(name) = type_base_name(type_node, self.source) {
                        self.push_relation(owner, RelationKind::Calls, name, location(type_node));
                    }
                }
                self.visit_children(node, owner, type_name, depth + 1);
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    /// A class, interface, struct, record (`record struct` is a Struct)
    /// or enum: the symbol, its attributes, base list, primary-constructor
    /// parameters and body. Members hang off `name`; anything else in the
    /// body (and a primary constructor's defaults) attaches to the type.
    fn visit_type(&mut self, node: Node, type_name: Option<&str>, depth: u32) {
        let kind = match node.kind() {
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "struct_declaration" => SymbolKind::Struct,
            "enum_declaration" => SymbolKind::Enum,
            _ if find_child(node, "struct").is_some() => SymbolKind::Struct,
            _ => SymbolKind::Class,
        };
        let name = self.field_text(node, "name");
        let id = self.push_symbol(
            name.clone(),
            kind,
            location(node),
            type_name.map(str::to_string),
        );
        self.visit_attributes(node, id, type_name, depth);
        if kind != SymbolKind::Enum {
            if let Some(base_list) = find_child(node, "base_list") {
                self.visit_base_list(base_list, id, kind, type_name, depth);
            }
        }
        if let Some(params) = find_child(node, "parameter_list") {
            self.visit_children(params, id, Some(&name), depth + 1);
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if kind == SymbolKind::Enum {
            let mut cursor = body.walk();
            for member in body.named_children(&mut cursor) {
                if member.kind() == "enum_member_declaration" {
                    let member_name = self.field_text(member, "name");
                    self.push_symbol(
                        member_name,
                        SymbolKind::Constant,
                        location(member),
                        Some(name.clone()),
                    );
                }
            }
        } else {
            self.visit_children(body, id, Some(&name), depth + 1);
        }
    }

    /// `base_list` is unified in this grammar — C# doesn't syntactically
    /// distinguish "extends" from "implements", both are one
    /// comma-separated list after `:`. Heuristic (AST-only, no type
    /// resolution): an interface's bases are all Extends, a struct's all
    /// Implements; for a class or record the base class, when present, must
    /// come first, so entry 0 is Extends unless it is named like an
    /// interface (`IFoo`, the .NET convention), and the rest are
    /// Implements. Targets are bare names (`Entity<Sku>` → `Entity`,
    /// `System.Exception` → `Exception`) so they match the definition.
    fn visit_base_list(
        &mut self,
        base_list: Node,
        id: SymbolId,
        kind: SymbolKind,
        type_name: Option<&str>,
        depth: u32,
    ) {
        let mut cursor = base_list.walk();
        let entries: Vec<Node> = base_list.named_children(&mut cursor).collect();
        let mut first = true;
        for entry in entries {
            let type_node = match entry.kind() {
                // `: Base(x, y)` after a primary constructor.
                "primary_constructor_base_type" | "argument_list" => {
                    if let Some(args) = find_child(entry, "argument_list")
                        .or((entry.kind() == "argument_list").then_some(entry))
                    {
                        self.visit_children(args, id, type_name, depth + 1);
                    }
                    entry.child_by_field_name("type")
                }
                _ => Some(entry),
            };
            let Some(type_node) = type_node else {
                continue;
            };
            let Some(base) = type_base_name(type_node, self.source) else {
                continue;
            };
            let relation_kind = match kind {
                SymbolKind::Interface => RelationKind::Extends,
                SymbolKind::Struct => RelationKind::Implements,
                _ if first && !looks_like_interface(&base) => RelationKind::Extends,
                _ => RelationKind::Implements,
            };
            first = false;
            self.push_relation(id, relation_kind, base, location(type_node));
        }
    }

    /// Attributes, parameters and body of a method-like member.
    fn visit_callable(&mut self, node: Node, id: SymbolId, type_name: Option<&str>, depth: u32) {
        self.visit_attributes(node, id, type_name, depth);
        if let Some(params) = node.child_by_field_name("parameters") {
            self.visit_children(params, id, type_name, depth + 1);
        }
        // A constructor's `: base(…)` / `: this(…)` initializer.
        if let Some(init) = find_child(node, "constructor_initializer") {
            self.visit_children(init, id, type_name, depth + 1);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_children(body, id, type_name, depth + 1);
        }
    }

    fn visit_attributes(&mut self, node: Node, id: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        let lists: Vec<Node> = node
            .children(&mut cursor)
            .filter(|n| n.kind() == "attribute_list")
            .collect();
        for list in lists {
            self.visit_children(list, id, type_name, depth + 1);
        }
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

/// The method/member name being invoked: a bare `identifier`, a generic
/// method's name (`Render<T>(…)` → `Render`), or the `name` of a
/// `member_access_expression` (`receiver.Name(...)`) or of the
/// `member_binding_expression` in a null-conditional call (`x?.Name(…)`).
fn callee_identifier(function: Node) -> Option<Node> {
    match function.kind() {
        "identifier" => Some(function),
        "generic_name" => find_child(function, "identifier"),
        "member_access_expression" | "member_binding_expression" => function
            .child_by_field_name("name")
            .and_then(callee_identifier),
        "conditional_access_expression" => {
            find_child(function, "member_binding_expression").and_then(callee_identifier)
        }
        _ => None,
    }
}

/// A type reference's bare name: `Foo`, `Foo<T>` → `Foo`, `A.B.Foo` →
/// `Foo`, `global::Foo` → `Foo`. `None` for predefined, tuple, array and
/// other types that have no single declared name.
fn type_base_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(text(node, source).to_string()),
        "generic_name" => find_child(node, "identifier").map(|n| text(n, source).to_string()),
        "qualified_name" | "alias_qualified_name" => node
            .child_by_field_name("name")
            .and_then(|n| type_base_name(n, source)),
        _ => None,
    }
}

/// `IFoo`: an `I`, an upper-case letter, then a lower-case one — the .NET
/// naming convention for interfaces (`IO` or `ID` alone don't match).
fn looks_like_interface(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('I')
        && chars.next().is_some_and(char::is_uppercase)
        && chars.next().is_some_and(char::is_lowercase)
}

/// Last identifier in a `using` path: the bare name, or a `qualified_name`'s
/// `name` field (`using System.Math;` → `Math`).
fn last_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" => Some(node),
        "qualified_name" => node.child_by_field_name("name").and_then(last_identifier),
        _ => None,
    }
}
