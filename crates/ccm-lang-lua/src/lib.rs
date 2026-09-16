//! `LanguageParser` implementation for Lua, via `tree-sitter-lua`.
//!
//! Lua was deliberately chosen for this crate because it is *not* one of the
//! project's seven initial languages — it exists purely to prove the plugin
//! architecture: everything needed to support it lives here, in a single new
//! crate, with zero changes to `ccm-core`, `ccm-index`, or `ccm-mcp-server`.
//!
//! Node/field names below (`function_declaration`, `dot_index_expression`,
//! `method_index_expression`, `variable_list`/`expression_list` pairing,
//! etc.) come from inspecting `tree-sitter-lua` 0.5.0's actual parse trees
//! (`tree_sitter::Node::to_sexp`), not from grammar docs — Lua's grammar is
//! less standardized across tree-sitter-lua forks than Rust's or Python's.

use ccm_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct LuaParser;

impl LanguageParser for LuaParser {
    fn language_id(&self) -> &'static str {
        "lua"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["lua"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_lua::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .expect("tree-sitter-lua grammar is statically valid");

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
        walker.visit_children(root, module_id);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".lua")
        .to_string()
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

    fn visit_children(&mut self, node: Node, owner: SymbolId) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId) {
        match node.kind() {
            // `function foo() end`, `local function foo() end`,
            // `function M.new() end`, `function M:greet() end` — same node
            // kind for all four; `local` and dotted/method names only
            // differ in which *field of the parent* points here, which a
            // kind-based match never sees, so this one arm covers all of
            // them uniformly.
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let (name, parent, is_method) = target_name(name_node, self.source);
                    let kind = if is_method || parent.is_some() {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    };
                    let id = self.push_symbol(name, kind, location(node), parent);
                    self.visit_function_body(node, id);
                }
            }
            "assignment_statement" => self.visit_assignment(node, owner),
            "function_call" => {
                if let Some(callee) = node.child_by_field_name("name") {
                    let (name, name_node) = call_target_name(callee, self.source);
                    if name == "require" {
                        if let Some(module) = require_argument(node, self.source) {
                            self.push_relation(owner, RelationKind::Imports, module, location(node));
                        }
                    } else if !name.is_empty() {
                        // location(name_node), not location(node): a chained
                        // call (`a:f(x):f(y)`) has its outer and inner
                        // function_call both start at `a`, which would make
                        // two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    }
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner);
                }
            }
            // An anonymous function not caught by the assignment-statement
            // special case below (e.g. passed inline as a callback
            // argument): no symbol to record, but calls inside it still
            // attach to whatever function currently owns this scope.
            "function_definition" => self.visit_function_body(node, owner),
            _ => self.visit_children(node, owner),
        }
    }

    fn visit_function_body(&mut self, function_node: Node, owner: SymbolId) {
        if let Some(params) = function_node.child_by_field_name("parameters") {
            self.visit_children(params, owner);
        }
        if let Some(body) = function_node.child_by_field_name("body") {
            self.visit_children(body, owner);
        }
    }

    /// `local M = {}` (or a plain global `M = {}`), and `local x = function()
    /// ... end` — Lua has no `class`/`module` keyword, so both patterns are
    /// only visible as an assignment whose right-hand side is a
    /// `table_constructor` or `function_definition`. Pairs `variable_list`
    /// and `expression_list` positionally (Lua's own multiple-assignment
    /// semantics); anything else on the right just gets recursed into for
    /// nested calls/requires, without producing a symbol.
    fn visit_assignment(&mut self, node: Node, owner: SymbolId) {
        // `variable_list`/`expression_list` are plain positional children of
        // `assignment_statement` in this grammar, not named fields on it
        // (only the identifiers *inside* each one carry field names) — found
        // by kind rather than `child_by_field_name`, which returns nothing here.
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let Some(variables) = children.iter().find(|n| n.kind() == "variable_list").copied() else {
            return;
        };
        let Some(values) = children.iter().find(|n| n.kind() == "expression_list").copied() else {
            return;
        };
        let mut var_cursor = variables.walk();
        let vars: Vec<Node> = variables.named_children(&mut var_cursor).collect();
        let mut val_cursor = values.walk();
        let vals: Vec<Node> = values.named_children(&mut val_cursor).collect();

        for (i, value) in vals.iter().enumerate() {
            let matching_var = vars.get(i);
            match value.kind() {
                "function_definition" => {
                    if let Some(var) = matching_var {
                        let (name, parent, _) = target_name(*var, self.source);
                        let id = self.push_symbol(name, SymbolKind::Function, location(*var), parent);
                        self.visit_function_body(*value, id);
                        continue;
                    }
                    self.visit_function_body(*value, owner);
                }
                "table_constructor" => {
                    if let Some(var) = matching_var {
                        let (name, parent, _) = target_name(*var, self.source);
                        self.push_symbol(name, SymbolKind::Module, location(*var), parent);
                        continue;
                    }
                }
                _ => self.visit(*value, owner),
            }
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
}

/// Extracts `(name, enclosing_table_name, is_method_call_syntax)` from an
/// assignment/declaration target: a plain `identifier`, a dotted
/// `table.field` (`dot_index_expression`), or a colon `table:method`
/// (`method_index_expression`). The enclosing table's own name is flattened
/// through nested dots (`Foo.Bar.baz` → parent `"Foo.Bar"`, name `"baz"`).
fn target_name(node: Node, source: &str) -> (String, Option<String>, bool) {
    match node.kind() {
        "identifier" => (text(node, source).to_string(), None, false),
        "dot_index_expression" => {
            let field_name = node
                .child_by_field_name("field")
                .map(|n| text(n, source).to_string())
                .unwrap_or_default();
            let table_name = node
                .child_by_field_name("table")
                .map(|n| flatten_table(n, source));
            (field_name, table_name, false)
        }
        "method_index_expression" => {
            let method_name = node
                .child_by_field_name("method")
                .map(|n| text(n, source).to_string())
                .unwrap_or_default();
            let table_name = node
                .child_by_field_name("table")
                .map(|n| flatten_table(n, source));
            (method_name, table_name, true)
        }
        _ => (text(node, source).to_string(), None, false),
    }
}

/// The callee's own name node for a `function_call` — never the whole
/// callee expression, whose start position is shared with any nested call
/// it's chained from. See [`target_name`] for the general table/method-name
/// shape; this only needs the specific leaf node the name text came from.
fn call_target_name<'a>(node: Node<'a>, source: &str) -> (String, Node<'a>) {
    match node.kind() {
        "dot_index_expression" => match node.child_by_field_name("field") {
            Some(field) => (text(field, source).to_string(), field),
            None => (text(node, source).to_string(), node),
        },
        "method_index_expression" => match node.child_by_field_name("method") {
            Some(method) => (text(method, source).to_string(), method),
            None => (text(node, source).to_string(), node),
        },
        _ => (text(node, source).to_string(), node),
    }
}

fn flatten_table(node: Node, source: &str) -> String {
    match node.kind() {
        "dot_index_expression" => {
            let table = node
                .child_by_field_name("table")
                .map(|n| flatten_table(n, source))
                .unwrap_or_default();
            let field = node
                .child_by_field_name("field")
                .map(|n| text(n, source))
                .unwrap_or_default();
            if table.is_empty() {
                field.to_string()
            } else {
                format!("{table}.{field}")
            }
        }
        _ => text(node, source).to_string(),
    }
}

/// `require('module_name')`'s argument: the literal module name, or `None`
/// for a non-literal argument (`require(computedName())`) — nothing static
/// to record in that case.
fn require_argument(call_node: Node, source: &str) -> Option<String> {
    let arguments = call_node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    if first.kind() != "string" {
        return None;
    }
    first
        .child_by_field_name("content")
        .map(|n| text(n, source).to_string())
}
