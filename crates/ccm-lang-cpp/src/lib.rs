//! `LanguageParser` implementation for C++, via `tree-sitter-cpp`.
//!
//! The one case this parser is built around: a class/function is routinely
//! **declared** in a `.h`/`.hpp` header and **defined** in a separate `.cpp`
//! file (`ReturnType ClassName::method(...) { ... }`). Each file is parsed in
//! isolation (see [`ccm_core::LanguageParser::parse`]), so there is no way to
//! merge them into one database row here — that only happens if both sides
//! emit a [`SymbolRecord`] with the *exact same* `name` and `parent`.
//! `declarator_name_and_parent` below exists specifically to pull `name` and
//! `parent` apart from a `qualified_identifier` (`ClassName::method`) the
//! same way the in-class declaration would produce them, so
//! `find_symbol`/`find_references` correlate the two by name+parent — see
//! `tests/index_integration.rs` for the header/definition-split assertions.

use ccm_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
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
        self.symbols.push(SymbolRecord { id, name, kind, location, parent });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    /// `owner` is the innermost enclosing function/method (calls attach to
    /// it); `type_name` is the innermost enclosing class/struct/namespace
    /// name, used as `parent` for members declared directly inside it.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "namespace_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(name.clone(), SymbolKind::Module, location(node), type_name.map(str::to_string));
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            "class_specifier" | "struct_specifier" => {
                let kind = if node.kind() == "class_specifier" { SymbolKind::Class } else { SymbolKind::Struct };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(name.clone(), kind, location(node), type_name.map(str::to_string));

                // `base_class_clause` is a direct child, not a named field —
                // every C++ base (`public`/`private`/`protected`) is genuine
                // inheritance (unlike C#, there's no syntactic "implements"
                // to split out: interfaces are just abstract base classes),
                // so every entry becomes Extends regardless of access.
                if let Some(bases) = find_child(node, "base_class_clause") {
                    let mut cursor = bases.walk();
                    for base in bases.children(&mut cursor) {
                        if matches!(base.kind(), "type_identifier" | "qualified_identifier" | "template_type") {
                            let name_node = if base.kind() == "qualified_identifier" { qualified_tail(base) } else { base };
                            self.push_relation(id, RelationKind::Extends, text(name_node, self.source).to_string(), location(base));
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
            }
            // A function/method *definition* (has a body). The declarator
            // may be wrapped (`int* Foo::bar()` → pointer_declarator around
            // the function_declarator) and its own name may be a bare
            // identifier (free function / in-class definition) or a
            // `qualified_identifier` (`ClassName::method`, the out-of-line
            // definition half of the header/source split).
            "function_definition" => {
                let Some(declarator) = node.child_by_field_name("declarator") else {
                    self.visit_children(node, owner, type_name, depth + 1);
                    return;
                };
                let Some(func_declarator) = find_function_declarator(declarator) else {
                    self.visit_children(node, owner, type_name, depth + 1);
                    return;
                };
                let name_node = func_declarator.child_by_field_name("declarator");
                let (name, parent) = declarator_name_and_parent(name_node, self.source, type_name);
                let kind = if parent.is_some() { SymbolKind::Method } else { SymbolKind::Function };
                let id = self.push_symbol(name, kind, location(node), parent);
                if let Some(params) = func_declarator.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, type_name, depth + 1);
                }
            }
            // Prototype-only forms: a class member declared but not defined
            // (`field_declaration`, inside a `class`/`struct` body) or a
            // free function/variable declared at namespace scope
            // (`declaration`). Both can hold more than one comma-separated
            // declarator (`int a, b;`).
            "field_declaration" | "declaration" => {
                let mut cursor = node.walk();
                let declarators: Vec<Node> = node
                    .children_by_field_name("declarator", &mut cursor)
                    .collect();
                for declarator in declarators {
                    if let Some(func_declarator) = find_function_declarator(declarator) {
                        let name_node = func_declarator.child_by_field_name("declarator");
                        let (name, parent) = declarator_name_and_parent(name_node, self.source, type_name);
                        let kind = if parent.is_some() { SymbolKind::Method } else { SymbolKind::Function };
                        self.push_symbol(name, kind, location(node), parent);
                    } else if let Some(name_node) = plain_declarator_identifier(declarator) {
                        let kind = if type_name.is_some() { SymbolKind::Field } else { SymbolKind::Variable };
                        self.push_symbol(text(name_node, self.source).to_string(), kind, location(declarator), type_name.map(str::to_string));
                    }
                }
            }
            "preproc_include" => {
                if let Some(path_node) = node.child_by_field_name("path") {
                    let raw = text(path_node, self.source);
                    // Quoted (`"Invoice.h"`) is a local, in-repo header;
                    // angle-bracketed (`<vector>`) is a system/external one
                    // — recorded the same way (Imports is the only fitting
                    // `RelationKind`), but its name will simply never match
                    // a file in this repo's index, so it naturally never
                    // resolves to a local symbol (see `ccm-index`'s
                    // by-name relation resolution) instead of needing a
                    // separate "external dependency" concept here.
                    let name = if let Some(stripped) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                        stripped.to_string()
                    } else {
                        raw.trim_start_matches('<').trim_end_matches('>').to_string()
                    };
                    self.push_relation(owner, RelationKind::Imports, name, location(node));
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
                        self.push_relation(owner, RelationKind::Calls, text(name_node, self.source).to_string(), location(name_node));
                    }
                    self.visit(function, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
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
        "pointer_declarator" | "array_declarator" | "parenthesized_declarator" | "init_declarator" => {
            node.child_by_field_name("declarator").and_then(find_function_declarator)
        }
        "reference_declarator" => node.named_child(0).and_then(find_function_declarator),
        _ => None,
    }
}

/// Same unwrapping as `find_function_declarator`, but for the non-function
/// (field/variable) case — returns the identifier actually being declared.
fn plain_declarator_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" | "field_identifier" => Some(node),
        "pointer_declarator" | "array_declarator" | "init_declarator" => {
            node.child_by_field_name("declarator").and_then(plain_declarator_identifier)
        }
        "reference_declarator" => node.named_child(0).and_then(plain_declarator_identifier),
        _ => None,
    }
}

/// A `function_declarator`'s own `declarator` field is the name being
/// declared: a bare `identifier`/`field_identifier`/`operator_name`/
/// `destructor_name` for an in-class or free-function form, or a
/// `qualified_identifier` (`ClassName::method`) for the out-of-line
/// definition half of a header/source split — in which case `scope` becomes
/// `parent`, matching exactly what the in-class declaration would have
/// produced for the same member.
fn declarator_name_and_parent(name_node: Option<Node>, source: &str, fallback_parent: Option<&str>) -> (String, Option<String>) {
    let Some(node) = name_node else {
        return (String::new(), fallback_parent.map(str::to_string));
    };
    if node.kind() == "qualified_identifier" {
        let scope = node.child_by_field_name("scope").map(|n| text(n, source).to_string());
        let inner = node.child_by_field_name("name");
        let (inner_name, _) = declarator_name_and_parent(inner, source, None);
        (inner_name, scope.or_else(|| fallback_parent.map(str::to_string)))
    } else {
        (text(node, source).to_string(), fallback_parent.map(str::to_string))
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
