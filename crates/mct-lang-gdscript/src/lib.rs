//! Indexes Godot scripts using the GDScript Tree-sitter grammar.
//!
//! Every `.gd` file is a `Module` that owns one script `Class`: the name
//! from `class_name`, or the file name without `.gd`, which only that file
//! can resolve. Members, inner classes, enums, signals, constants, locals and
//! parameters hang under it. Script paths (`extends "…"`, `preload`, `load`)
//! become `Imports`, and `mct-index` roots `res://` at the nearest
//! `project.godot`.
//!
//! Known limits:
//!
//! - tree-sitter-gdscript 6.1.0 accepts only ASCII identifiers. Godot 4
//!   accepts Unicode ones (`var café`), but such a file is a syntax error
//!   here and is not indexed.
//! - A call through a PascalCase name the file does not bind
//!   (`ItemCatalog.find()`) is qualified by that name: a `class_name`
//!   global, an autoload or an engine class. `mct-index` makes an autoload
//!   extend the class named after its script file, so an autoload whose
//!   script declares another `class_name` does not resolve. A call through
//!   an untyped value is a member call the index can only report as
//!   ambiguous.
//! - Computed resource paths, scene (`.tscn`) bindings and signal
//!   connections made in the editor are not relations.

mod relations;
mod tokens;

pub use tokens::GdscriptTokenSource;

use std::{collections::HashMap, ops::Range};

use mct_core::{
    LanguageParser, LiteralCollector, Modifier, Parameter, ParameterKind, ParseError, ParsedFile,
    SourceFile, SymbolDetail, SymbolId, SymbolKind, SymbolRecord, TypeText, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_named_child, first_error, location, module_location};
use tree_sitter::{Node, Parser};

/// The `LanguageParser` for Godot `.gd` scripts, language id `gdscript`.
pub struct GdscriptParser;

impl LanguageParser for GdscriptParser {
    fn language_id(&self) -> &'static str {
        "gdscript"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["gd"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let error = |line, message: &str| ParseError::Syntax {
            path: file.relative_path.clone(),
            line,
            message: message.into(),
        };
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .map_err(|_| error(1, "could not load GDScript grammar"))?;
        let tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| error(1, "tree-sitter produced no parse tree"))?;
        let root = tree.root_node();
        if root.has_error() {
            let node = first_error(root).unwrap_or(root);
            return Err(error(location(node).line, "GDScript syntax error"));
        }

        let mut walker = Walker {
            file,
            result: ParsedFile::default(),
            scopes: Vec::new(),
            receiver_types: Default::default(),
            preloaded_scripts: Default::default(),
            http_uses: Vec::new(),
            bindings: Default::default(),
            local_ranges: Default::default(),
            pending: Vec::new(),
            literals: LiteralCollector::default(),
        };
        let module_name = basename(&file.relative_path);
        let module = walker.symbol(module_name, SymbolKind::Module, root, None);
        let class_header = find_named_child(root, "class_name_statement");
        let class_name = class_header.and_then(|n| n.child_by_field_name("name"));
        let class = walker.symbol(
            class_name.map_or_else(|| module_name.trim_end_matches(".gd"), |n| walker.text(n)),
            SymbolKind::Class,
            class_header.unwrap_or(root),
            Some(module),
        );
        walker.result.symbols[class as usize].location.end_line = module_location(root).end_line;
        if class_name.is_none() {
            walker.result.file_scoped.push(class);
        }
        let base = class_header
            .and_then(|n| n.child_by_field_name("extends"))
            .or_else(|| find_named_child(root, "extends_statement"));
        walker.children(root, class, base, 0);
        walker.finish_relations();
        walker.result.literals = walker.literals.finish();
        Ok(walker.result)
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn first_named(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let child = node.named_children(&mut cursor).find(|n| !n.is_extra());
    child
}

struct Walker<'a> {
    file: &'a SourceFile,
    result: ParsedFile,
    scopes: Vec<Option<SymbolId>>,
    bindings: HashMap<(SymbolId, &'a str), Vec<SymbolId>>,
    local_ranges: HashMap<SymbolId, Range<usize>>,
    receiver_types: HashMap<SymbolId, &'a str>,
    preloaded_scripts: HashMap<SymbolId, String>,
    http_uses: Vec<relations::HttpUse<'a>>,
    pending: Vec<relations::Pending<'a>>,
    literals: LiteralCollector,
}

impl<'a> Walker<'a> {
    fn text(&self, node: Node<'_>) -> &'a str {
        node.utf8_text(self.file.contents.as_bytes())
            .unwrap_or_default()
    }

    fn symbol(
        &mut self,
        name: &'a str,
        kind: SymbolKind,
        node: Node<'_>,
        owner: Option<SymbolId>,
    ) -> SymbolId {
        let id = self.result.symbols.len() as SymbolId;
        self.result.symbols.push(SymbolRecord {
            id,
            name: name.into(),
            kind,
            location: module_location(node),
            parent: owner.map(|id| self.result.symbols[id as usize].name.clone()),
            level: None,
        });
        self.scopes.push(owner);
        if let Some(parent) = owner {
            if !matches!(
                self.result.symbols[parent as usize].kind,
                SymbolKind::Class | SymbolKind::Module | SymbolKind::Enum
            ) {
                self.result.file_scoped.push(id);
                if let Some(range) = local_scope(node) {
                    self.local_ranges.insert(id, range);
                }
            }
        }
        if let Some(owner) = owner {
            self.bindings.entry((owner, name)).or_default().push(id);
            let parent = &self.result.symbols[owner as usize];
            if parent.kind == SymbolKind::Enum && parent.name == "<anonymous enum>" {
                if let Some(class) = self.scopes[owner as usize] {
                    self.bindings.entry((class, name)).or_default().push(id);
                }
            }
        }
        id
    }

    fn children<'tree>(
        &mut self,
        node: Node<'tree>,
        owner: SymbolId,
        base: Option<Node<'tree>>,
        depth: u32,
    ) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, owner, base, depth + 1);
        }
    }

    fn visit<'tree>(
        &mut self,
        node: Node<'tree>,
        mut owner: SymbolId,
        mut base: Option<Node<'tree>>,
        depth: u32,
    ) {
        if depth > MAX_TRAVERSAL_DEPTH {
            return;
        }
        let kind = self.result.symbols[owner as usize].kind;
        let in_class = kind == SymbolKind::Class;
        let name = node.child_by_field_name("name").map(|n| self.text(n));
        let lambda = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("right"))
            .filter(|n| n.kind() == "lambda");
        let declaration = match node.kind() {
            "class_definition" => {
                base = node.child_by_field_name("extends").or_else(|| {
                    node.child_by_field_name("body")
                        .and_then(|body| find_named_child(body, "extends_statement"))
                });
                name.map(|n| (n, SymbolKind::Class))
            }
            "function_definition" => name.map(|n| (n, callable_kind(in_class))),
            "constructor_definition" => Some(("_init", SymbolKind::Method)),
            "lambda" => name.map(|n| (n, SymbolKind::Function)),
            "assignment" if lambda.is_some() => node
                .child_by_field_name("left")
                .filter(|n| n.kind() == "identifier")
                .map(|n| (self.text(n), SymbolKind::Function)),
            "signal_statement" => name.map(|n| (n, SymbolKind::Field)),
            "enum_definition" => Some((name.unwrap_or("<anonymous enum>"), SymbolKind::Enum)),
            "enumerator" => node
                .child_by_field_name("left")
                .map(|n| (self.text(n), SymbolKind::Constant)),
            "const_statement" if lambda.is_none() => name.map(|n| (n, SymbolKind::Constant)),
            "const_statement"
            | "variable_statement"
            | "export_variable_statement"
            | "onready_variable_statement" => {
                let kind = if lambda.is_some() {
                    callable_kind(in_class)
                } else if in_class {
                    SymbolKind::Field
                } else {
                    SymbolKind::Variable
                };
                name.map(|n| (n, kind))
            }
            "for_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let id = self.symbol(self.text(left), SymbolKind::Variable, left, Some(owner));
                    if let Some(body) = node.child_by_field_name("body") {
                        self.local_ranges.insert(id, body.byte_range());
                    }
                }
                None
            }
            "set_body" => {
                if let Some(param) = find_named_child(node, "parameters").and_then(first_named) {
                    let name = if param.kind() == "identifier" {
                        Some(param)
                    } else {
                        first_named(param)
                    };
                    if let Some(name) = name {
                        let id =
                            self.symbol(self.text(name), SymbolKind::Variable, name, Some(owner));
                        if let Some(body) = node.child_by_field_name("body") {
                            self.local_ranges.insert(id, body.byte_range());
                        }
                    }
                }
                None
            }
            "pattern_binding" => first_named(node).map(|n| (self.text(n), SymbolKind::Variable)),
            _ => None,
        };

        if let Some((name, kind)) = declaration {
            let id = self.symbol(name, kind, node, Some(owner));
            self.record_binding_type(id, node);
            if let Some(lambda) = lambda {
                self.callable_details(id, lambda);
                self.children(lambda, id, base, depth + 1);
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor).filter(|n| *n != lambda) {
                    if node.kind() != "assignment"
                        || node.child_by_field_name("left") != Some(child)
                    {
                        self.visit(child, id, base, depth + 1);
                    }
                }
                return;
            }
            if matches!(kind, SymbolKind::Method | SymbolKind::Function)
                || node.kind() == "signal_statement"
            {
                self.callable_details(id, node);
            }
            // Initializers are evaluated in the enclosing scope. Accessor bodies
            // and signal parameters belong to the declaration they describe.
            if kind != SymbolKind::Variable {
                owner = id;
            }
        }
        self.observe(node, owner, base);
        self.children(node, owner, base, depth);
    }

    fn callable_details(&mut self, id: SymbolId, node: Node<'_>) {
        let parameters = node.child_by_field_name("parameters");
        let signature_end = node
            .child_by_field_name("body")
            .map_or(node.end_byte(), |body| body.start_byte());
        let signature = self
            .file
            .contents
            .get(node.start_byte()..signature_end)
            .map(|s| s.trim_end().trim_end_matches(':').to_owned());
        let mut detail = SymbolDetail {
            symbol: id,
            signature,
            parameters_known: parameters.is_some(),
            return_type: node
                .child_by_field_name("return_type")
                .map(|n| TypeText::syntax(self.text(n))),
            ..Default::default()
        };
        if node.kind() == "function_definition" && node.child_by_field_name("body").is_none() {
            detail.modifiers.push(Modifier::Abstract);
        }
        if find_named_child(node, "static_keyword").is_some() {
            detail.modifiers.push(Modifier::Static);
        }
        if let Some(parameters) = parameters {
            let mut cursor = parameters.walk();
            for mut param in parameters
                .named_children(&mut cursor)
                .filter(|n| !n.is_extra())
            {
                let variadic = param.kind() == "variadic_parameter";
                if variadic {
                    let Some(inner) = first_named(param) else {
                        continue;
                    };
                    param = inner;
                }
                let name = if param.kind() == "identifier" {
                    Some(param)
                } else {
                    first_named(param)
                };
                if let Some(n) = name {
                    let param_id = self.symbol(self.text(n), SymbolKind::Variable, n, Some(id));
                    self.record_binding_type(param_id, param);
                }
                detail.parameters.push(Parameter {
                    name: name.map(|n| self.text(n).to_string()),
                    type_text: param
                        .child_by_field_name("type")
                        .filter(|n| n.kind() == "type")
                        .map(|n| TypeText::syntax(self.text(n))),
                    default: param
                        .child_by_field_name("value")
                        .map(|n| self.text(n).into()),
                    kind: if variadic {
                        ParameterKind::Variadic
                    } else {
                        ParameterKind::Positional
                    },
                });
            }
        }
        self.result.symbol_details.push(detail.normalized());
    }
}

fn callable_kind(in_class: bool) -> SymbolKind {
    if in_class {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    }
}

/// A local is visible only from its declaration to the end of its block.
/// Parameters are owned by the callable and are visible throughout it.
fn local_scope(node: Node<'_>) -> Option<Range<usize>> {
    let mut enclosing = node.parent();
    while let Some(parent) = enclosing {
        match parent.kind() {
            "parameters" => return None,
            "pattern_section" | "body" => return Some(node.end_byte()..parent.end_byte()),
            _ => enclosing = parent.parent(),
        }
    }
    None
}
