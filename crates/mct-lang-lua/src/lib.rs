//! `LanguageParser` implementation for Lua, via `tree-sitter-lua`.
//!
//! Lua was deliberately chosen for this crate because it is *not* one of the
//! project's seven initial languages — it exists purely to prove the plugin
//! architecture: everything needed to support it lives here, in a single new
//! crate, with zero changes to `mct-core`, `mct-index`, or `mct-mcp-server`.
//!
//! Node/field names below (`function_declaration`, `dot_index_expression`,
//! `method_index_expression`, `variable_list`/`expression_list` pairing,
//! etc.) come from inspecting `tree-sitter-lua` 0.5.0's actual parse trees
//! (`tree_sitter::Node::to_sexp`), not from grammar docs — Lua's grammar is
//! less standardized across tree-sitter-lua forks than Rust's or Python's.

mod libraries;
mod tokens;

pub use tokens::LuaTokenSource;

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location};
use std::collections::{HashMap, HashSet};
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
        let mut walker = Walker::new(&file.contents);
        let module_id =
            walker.push_symbol(module_name, SymbolKind::Module, module_location(root), None);
        walker.visit_children(root, module_id, 0);
        Ok(walker.finish())
    }
}

/// The module a file defines: its stem, except that a package's `init.lua`
/// is named after its directory, because `require("a.b")` loads either
/// `a/b.lua` or `a/b/init.lua`.
fn module_name_for(relative_path: &str) -> String {
    let mut segments = relative_path.rsplit('/');
    let file = segments.next().unwrap_or(relative_path);
    if file == "init.lua" {
        if let Some(directory) = segments.next().filter(|d| !d.is_empty()) {
            return directory.to_string();
        }
    }
    file.trim_end_matches(".lua").to_string()
}

/// The file-level module's location: the root node of a file ending in a
/// newline ends at column 0 of the row *after* the last line, which would
/// put the module one line past the end of the file.
fn module_location(root: Node) -> Location {
    let mut loc = location(root);
    let end = root.end_position();
    if end.column == 0 && end.row > root.start_position().row {
        loc.end_line = Some(end.row as u32);
    }
    loc
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// A module a local name was bound to by a literal `require`, possibly through
/// one of its fields (`require("a.b").C`).
#[derive(Clone)]
struct Import {
    /// The dotted module path as written.
    module: String,
    field: Option<String>,
}

/// What the walk knows about one lexically scoped local name.
#[derive(Clone, Default)]
struct Binding {
    /// A symbol for the name exists already, so assigning it again is a
    /// reassignment, not a declaration.
    indexed: bool,
    import: Option<Import>,
    /// The in-file name an alias such as `local B = Child` stands for.
    alias_of: Option<String>,
}

/// The target of an `Extends` or `References` relation: the name to link and
/// the proof of where it was imported from, when there is one.
struct TargetRef {
    name: String,
    import: Option<Import>,
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
    /// Function bodies (named or anonymous) the walk is currently inside.
    function_depth: u32,
    /// Lexical scopes, innermost last; the first is the file's.
    scopes: Vec<HashMap<String, Binding>>,
    /// Globals already indexed: the first assignment declares one.
    globals: HashSet<String>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
            next_id: 0,
            function_depth: 0,
            scopes: vec![HashMap::new()],
            globals: HashSet::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str, binding: Binding) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), binding);
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
    }

    /// Whether assigning `var` declares a symbol (`true`) or only reassigns a
    /// name that has one already. Only a plain name can be a reassignment:
    /// a declared local, or a global assigned before.
    fn declares(&mut self, var: Node, is_local: bool) -> bool {
        if is_local || var.kind() != "identifier" {
            return true;
        }
        let name = text(var, self.source).to_string();
        if let Some(binding) = self.lookup_mut(&name) {
            return !std::mem::replace(&mut binding.indexed, true);
        }
        self.globals.insert(name)
    }

    /// The name of the function symbol `owner`, when `owner` is one.
    fn function_name(&self, owner: SymbolId) -> Option<String> {
        if owner == FILE_MODULE {
            return None;
        }
        self.symbols
            .get(owner as usize)
            .map(|s| s.name.clone())
            .filter(|n| !n.is_empty())
    }

    /// Outside every function body: a declaration here belongs to the file.
    fn at_file_scope(&self) -> bool {
        self.function_depth == 0
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

    fn visit_children(&mut self, node: Node, owner: SymbolId, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, depth + 1);
        }
    }

    /// `depth` bounds native stack usage against adversarially deep/nested
    /// input (see `MAX_TRAVERSAL_DEPTH`) — every recursive call below passes
    /// `depth + 1`, and this early-return prunes the subtree instead of
    /// recursing further once the ceiling is hit.
    fn visit(&mut self, node: Node, owner: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
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
                    let is_local = node.prev_sibling().is_some_and(|s| s.kind() == "local")
                        || text(node, self.source).starts_with("local ");
                    if name_node.kind() == "identifier" {
                        if is_local {
                            let binding = Binding {
                                indexed: true,
                                ..Binding::default()
                            };
                            self.declare(&name, binding);
                        } else {
                            self.globals.insert(name.clone());
                        }
                    }
                    let id = self.push_symbol(name, kind, location(node), parent);
                    self.visit_function_body(node, id, depth);
                }
            }
            "assignment_statement" => self.visit_assignment(node, owner, depth),
            "variable_declaration" => self.visit_declaration(node, owner, depth),
            "block" => {
                self.push_scope();
                self.visit_children(node, owner, depth);
                self.pop_scope();
            }
            "for_statement" => self.visit_for(node, owner, depth),
            "function_call" => {
                if let Some(callee) = node.child_by_field_name("name") {
                    let (name, name_node) = call_target_name(callee, self.source);
                    if let Some((kind, path)) = self.file_loader(callee, &name, node) {
                        self.push_relation(owner, kind, path, location(node));
                    } else if name == "setmetatable" && callee.kind() == "identifier" {
                        self.visit_setmetatable(node);
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    } else if name == "require" && callee.kind() == "identifier" {
                        if let Some(module) = require_argument(node, self.source) {
                            let merged = self.push_module_import(owner, &module, location(node));
                            if !merged {
                                self.push_relation(
                                    owner,
                                    RelationKind::Imports,
                                    module,
                                    location(node),
                                );
                            }
                        }
                    } else if !name.is_empty() {
                        // location(name_node), not location(node): a chained
                        // call (`a:f(x):f(y)`) has its outer and inner
                        // function_call both start at `a`, which would make
                        // two same-named chained calls collide into one
                        // indistinguishable row.
                        self.push_relation(owner, RelationKind::Calls, name, location(name_node));
                    }
                    // `money.gross(net):to_json()`: the call this one is
                    // chained on sits in the callee's table expression.
                    if let Some(table) = callee.child_by_field_name("table") {
                        self.visit(table, owner, depth + 1);
                    }
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, depth + 1);
                }
            }
            // An anonymous function not caught by the assignment-statement
            // special case below (e.g. passed inline as a callback
            // argument): no symbol to record, but calls inside it still
            // attach to whatever function currently owns this scope.
            "function_definition" => self.visit_function_body(node, owner, depth),
            "table_constructor" => self.visit_table(node, None, owner, depth),
            _ => self.visit_children(node, owner, depth + 1),
        }
    }

    fn visit_function_body(&mut self, function_node: Node, owner: SymbolId, depth: u32) {
        self.function_depth += 1;
        self.push_scope();
        let parameter = Binding {
            indexed: true,
            ..Binding::default()
        };
        if function_node
            .child_by_field_name("name")
            .is_some_and(|n| n.kind() == "method_index_expression")
        {
            self.declare("self", parameter.clone());
        }
        if let Some(params) = function_node.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            let names: Vec<String> = params
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "identifier")
                .map(|n| text(n, self.source).to_string())
                .collect();
            for name in names {
                self.declare(&name, parameter.clone());
            }
        }
        if let Some(body) = function_node.child_by_field_name("body") {
            self.visit_children(body, owner, depth + 1);
        }
        self.pop_scope();
        self.function_depth -= 1;
    }

    /// `local a, b` with no value. Inside a function the names are variables
    /// of it; at file level they are only scoped, because the assignment that
    /// defines them (`local M` … `M = {}`) is what declares the symbol.
    fn visit_declaration(&mut self, node: Node, owner: SymbolId, depth: u32) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        let Some(variables) = children.iter().find(|n| n.kind() == "variable_list") else {
            self.visit_children(node, owner, depth + 1);
            return;
        };
        let mut names_cursor = variables.walk();
        let names: Vec<Node> = variables
            .named_children(&mut names_cursor)
            .filter(|n| n.kind() == "identifier")
            .collect();
        for name in names {
            let in_function = !self.at_file_scope() && owner != FILE_MODULE;
            if in_function {
                self.push_loop_or_local(name, owner);
            }
            let binding = Binding {
                indexed: in_function,
                ..Binding::default()
            };
            self.declare(text(name, self.source), binding);
        }
    }

    /// A variable declared inside the function `owner`.
    fn push_loop_or_local(&mut self, name: Node, owner: SymbolId) {
        let function = self.function_name(owner);
        self.push_symbol(
            text(name, self.source).to_string(),
            SymbolKind::Variable,
            span(name, name),
            function,
        );
    }

    /// `for i = a, b do … end` and `for k, v in pairs(t) do … end`: the loop
    /// variables are locals of the loop, and variables of the enclosing
    /// function.
    fn visit_for(&mut self, node: Node, owner: SymbolId, depth: u32) {
        self.push_scope();
        if let Some(clause) = node.child_by_field_name("clause") {
            let mut names: Vec<Node> = Vec::new();
            if clause.kind() == "for_numeric_clause" {
                for field in ["start", "end", "step"] {
                    if let Some(expression) = clause.child_by_field_name(field) {
                        self.visit(expression, owner, depth + 1);
                    }
                }
                names.extend(clause.child_by_field_name("name"));
            } else {
                let mut cursor = clause.walk();
                let children: Vec<Node> = clause.named_children(&mut cursor).collect();
                for child in children {
                    if child.kind() == "variable_list" {
                        let mut inner = child.walk();
                        names.extend(
                            child
                                .named_children(&mut inner)
                                .filter(|n| n.kind() == "identifier"),
                        );
                    } else {
                        self.visit_children(child, owner, depth + 1);
                    }
                }
            }
            for name in names {
                if !self.at_file_scope() && owner != FILE_MODULE {
                    self.push_loop_or_local(name, owner);
                }
                let binding = Binding {
                    indexed: true,
                    ..Binding::default()
                };
                self.declare(text(name, self.source), binding);
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body, owner, depth + 1);
        }
        self.pop_scope();
    }

    /// `local M = {}` (or a plain global `M = {}`), and `local x = function()
    /// ... end` — Lua has no `class`/`module` keyword, so both patterns are
    /// only visible as an assignment whose right-hand side is a
    /// `table_constructor` or `function_definition`. Pairs `variable_list`
    /// and `expression_list` positionally (Lua's own multiple-assignment
    /// semantics); anything else on the right just gets recursed into for
    /// nested calls/requires, without producing a symbol.
    fn visit_assignment(&mut self, node: Node, owner: SymbolId, depth: u32) {
        // `variable_list`/`expression_list` are plain positional children of
        // `assignment_statement` in this grammar, not named fields on it
        // (only the identifiers *inside* each one carry field names) — found
        // by kind rather than `child_by_field_name`, which returns nothing here.
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let Some(variables) = children
            .iter()
            .find(|n| n.kind() == "variable_list")
            .copied()
        else {
            return;
        };
        let Some(values) = children
            .iter()
            .find(|n| n.kind() == "expression_list")
            .copied()
        else {
            return;
        };
        let mut var_cursor = variables.walk();
        let vars: Vec<Node> = variables
            .named_children(&mut var_cursor)
            .filter(|n| n.kind() != "attribute")
            .collect();
        let mut val_cursor = values.walk();
        let vals: Vec<Node> = values.named_children(&mut val_cursor).collect();

        let is_local = node
            .parent()
            .is_some_and(|p| p.kind() == "variable_declaration");
        for (i, value) in vals.iter().enumerate() {
            let matching_var = vars.get(i);
            // Every name is declared (or found to be a reassignment) once,
            // whatever the value turns out to be.
            let new = matching_var.is_none_or(|var| self.declares(*var, is_local));
            match value.kind() {
                "function_definition" => {
                    if let Some(var) = matching_var.filter(|_| new) {
                        let (name, parent, _) = target_name(*var, self.source);
                        // `T.f = function` is a method of T, as `function
                        // T.f()` is; it spans the target through `end`.
                        let kind = if parent.is_some() {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        };
                        let id = self.push_symbol(name, kind, span(*var, *value), parent);
                        self.visit_function_body(*value, id, depth);
                        continue;
                    }
                    self.visit_function_body(*value, owner, depth);
                }
                "table_constructor" => {
                    // A table bound to a name at file level is a module; a
                    // local table in a function, or one stored through an
                    // index (`t[#t + 1] = {}`), is just a value.
                    let module = match matching_var {
                        Some(var) if new && self.at_file_scope() && is_name_target(*var) => {
                            let (name, parent, _) = target_name(*var, self.source);
                            let qualified = parent
                                .as_ref()
                                .map_or_else(|| name.clone(), |p| format!("{p}.{name}"));
                            self.push_symbol(name, SymbolKind::Module, span(*var, *value), parent);
                            Some(qualified)
                        }
                        Some(var) if new => {
                            self.push_assigned(*var, *value, is_local, owner, false);
                            None
                        }
                        _ => None,
                    };
                    self.visit_table(*value, module, owner, depth);
                }
                _ => {
                    if let Some(var) = matching_var.filter(|_| new) {
                        if !self.mark_class_by_index(*var, *value)
                            && !self.push_class_from_setmetatable(*var, *value)
                            && !self.push_class_from_helper(*var, *value)
                            && !self.is_import_call(*value)
                        {
                            let literal = is_literal_value(*value);
                            self.push_assigned(*var, *value, is_local, owner, literal);
                            self.push_delegation(*var, *value);
                        }
                    }
                    self.visit(*value, owner, depth + 1);
                }
            }
        }
        // `local a, b = f()`: a trailing call supplies every remaining name.
        if vals.last().is_some_and(|v| v.kind() == "function_call") {
            if let Some(last) = vals.last() {
                for var in vars.iter().skip(vals.len()) {
                    if self.declares(*var, is_local) {
                        self.push_assigned(*var, *last, is_local, owner, false);
                    }
                }
            }
        }
        // A local is in scope after its own declaration, never inside the
        // values it is initialised from (`local x = x`).
        if is_local {
            for (i, var) in vars.iter().enumerate() {
                if var.kind() != "identifier" {
                    continue;
                }
                let binding = self.binding_for(vals.get(i).copied());
                self.declare(text(*var, self.source), binding);
            }
        }
    }

    /// What a local initialised from `value` stands for: the module it
    /// required, or the in-file name it aliases.
    fn binding_for(&self, value: Option<Node>) -> Binding {
        let mut binding = Binding {
            indexed: true,
            ..Binding::default()
        };
        let Some(value) = value else {
            return binding;
        };
        binding.import = self.import_of(value);
        if binding.import.is_none() && value.kind() == "identifier" {
            let name = text(value, self.source);
            binding.alias_of = Some(
                self.lookup(name)
                    .and_then(|b| b.alias_of.clone())
                    .unwrap_or_else(|| name.to_string()),
            );
        }
        binding
    }

    /// The module `node` evaluates to when that is provable from the source:
    /// a literal `require`, a local bound to one, or a field of either.
    fn import_of(&self, node: Node) -> Option<Import> {
        match node.kind() {
            "function_call" => self.required_module(node).map(|module| Import {
                module,
                field: None,
            }),
            "identifier" => self.lookup(text(node, self.source))?.import.clone(),
            "dot_index_expression" => {
                let table = node.child_by_field_name("table")?;
                let field = node.child_by_field_name("field")?;
                let base = self.import_of(table).filter(|i| i.field.is_none())?;
                Some(Import {
                    field: Some(text(field, self.source).to_string()),
                    ..base
                })
            }
            _ => None,
        }
    }

    /// The literal module a `require("m")` call loads, unless `require` has
    /// been rebound in this file.
    fn required_module(&self, call: Node) -> Option<String> {
        let callee = call.child_by_field_name("name")?;
        if callee.kind() != "identifier"
            || text(callee, self.source) != "require"
            || self.lookup("require").is_some()
        {
            return None;
        }
        require_argument(call, self.source)
    }

    /// A variable, constant or table field bound by an assignment. File-level
    /// targets are always indexed, whatever the value; inside a function a
    /// plain `local` name is, as a variable of that function, and a global's
    /// first assignment is, as a variable of no scope. Assignments to fields
    /// from within a function are not declarations.
    /// A computed `__…` slot (`Class.__index = Class`) is metatable wiring,
    /// not a field worth a symbol.
    fn push_assigned(
        &mut self,
        var: Node,
        end: Node,
        is_local: bool,
        owner: SymbolId,
        literal: bool,
    ) {
        if !is_name_target(var) {
            return;
        }
        let (name, parent, _) = target_name(var, self.source);
        if self.at_file_scope() {
            if !literal && name.starts_with("__") {
                return;
            }
            let is_const = var.next_named_sibling().is_some_and(|n| {
                n.kind() == "attribute" && text(n, self.source).trim() == "<const>"
            });
            let kind = if parent.is_some() {
                SymbolKind::Field
            } else if is_const {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };
            self.push_symbol(name, kind, span(var, end), parent);
        } else if is_local && var.kind() == "identifier" && owner != FILE_MODULE {
            let function = self.function_name(owner);
            self.push_symbol(name, SymbolKind::Variable, span(var, end), function);
        } else if !is_local && var.kind() == "identifier" {
            // A global first assigned inside a function belongs to no scope.
            self.push_symbol(name, SymbolKind::Variable, span(var, end), None);
        }
    }

    /// `require(...)`, `dofile(...)` or `loadfile(...)`: a name bound to one
    /// is an alias of the imported module, already an `Imports` relation, and
    /// would only duplicate the module's own symbol.
    fn is_import_call(&self, value: Node) -> bool {
        value.kind() == "function_call"
            && value.child_by_field_name("name").is_some_and(|callee| {
                callee.kind() == "identifier"
                    && matches!(text(callee, self.source), "require" | "dofile" | "loadfile")
            })
    }

    /// The table symbol (`Module` or already `Class`) a file-level name
    /// declared, promoted to a `Class`: a table is a class once a metatable
    /// is wired to it.
    fn promote_to_class(&mut self, name: &str) -> Option<SymbolId> {
        let symbol = self.symbols.iter_mut().rev().find(|s| {
            s.id != FILE_MODULE
                && s.parent.is_none()
                && s.name == name
                && matches!(s.kind, SymbolKind::Module | SymbolKind::Class)
        })?;
        symbol.kind = SymbolKind::Class;
        Some(symbol.id)
    }

    /// `Class.__index = Class` makes the table `Class` a class. Returns
    /// whether the assignment was that idiom (and so declares nothing else).
    fn mark_class_by_index(&mut self, var: Node, value: Node) -> bool {
        if var.kind() != "dot_index_expression" || !self.at_file_scope() {
            return false;
        }
        let (field, table, _) = target_name(var, self.source);
        if field != "__index" || value.kind() != "identifier" {
            return false;
        }
        let Some(table) = table else {
            return false;
        };
        if text(value, self.source) != table {
            return false;
        }
        self.promote_to_class(&table);
        true
    }

    /// `local Dog = setmetatable({}, { __index = Animal })` at file level
    /// declares the class `Dog` extending `Animal`; without a base it is an
    /// ordinary value. Returns whether a class was declared.
    fn push_class_from_setmetatable(&mut self, var: Node, value: Node) -> bool {
        if !self.at_file_scope() || var.kind() != "identifier" || value.kind() != "function_call" {
            return false;
        }
        let Some(callee) = value.child_by_field_name("name") else {
            return false;
        };
        if callee.kind() != "identifier" || text(callee, self.source) != "setmetatable" {
            return false;
        }
        let Some(base) = self.metatable_base(value, 1) else {
            return false;
        };
        let id = self.push_symbol(
            text(var, self.source).to_string(),
            SymbolKind::Class,
            span(var, value),
            None,
        );
        self.push_extends(id, base);
        true
    }

    /// `local Dog = class(Animal)`, `class("Dog", Animal)` and
    /// `Animal:extend()` / `Animal:subclass("Dog")` at file level declare the
    /// class `Dog`. The helper must be provable: `class` is not defined in
    /// this file, and a base (or the receiver of `extend`) is an imported
    /// module or a class of this file. Anything else stays an ordinary value.
    fn push_class_from_helper(&mut self, var: Node, value: Node) -> bool {
        if !self.at_file_scope() || var.kind() != "identifier" || value.kind() != "function_call" {
            return false;
        }
        let Some(callee) = value.child_by_field_name("name") else {
            return false;
        };
        let base = match callee.kind() {
            "identifier" if text(callee, self.source) == "class" => {
                if self.lookup("class").is_some_and(|b| b.import.is_none()) {
                    return false;
                }
                let Some(arguments) = value.child_by_field_name("arguments") else {
                    return false;
                };
                let mut cursor = arguments.walk();
                let found = arguments
                    .named_children(&mut cursor)
                    .find(|a| a.kind() != "string" && self.is_known_class(*a));
                found
            }
            "method_index_expression" => {
                let method = callee
                    .child_by_field_name("method")
                    .map(|m| text(m, self.source));
                let receiver = callee.child_by_field_name("table");
                match (method, receiver) {
                    (Some("extend" | "subclass"), Some(receiver))
                        if self.is_known_class(receiver) =>
                    {
                        Some(receiver)
                    }
                    _ => return false,
                }
            }
            _ => return false,
        };
        let id = self.push_symbol(
            text(var, self.source).to_string(),
            SymbolKind::Class,
            span(var, value),
            None,
        );
        if let Some(base) = base {
            self.push_extends(id, base);
        }
        true
    }

    /// Whether `node` is provably a class or a module: an imported module (or
    /// a field of one), or a class declared in this file.
    fn is_known_class(&self, node: Node) -> bool {
        if self.import_of(node).is_some() {
            return true;
        }
        node.kind() == "identifier" && {
            let name = text(node, self.source);
            self.symbols
                .iter()
                .any(|s| s.name == name && s.parent.is_none() && s.kind == SymbolKind::Class)
        }
    }

    /// The symbol a base expression stands for. A local bound by `require`
    /// names the required module (or the field taken from it); a local alias
    /// of an in-file name names that name; anything else is read as written.
    fn resolve_target(&self, node: Node) -> TargetRef {
        if let Some(import) = self.import_of(node) {
            let name = import.field.clone().unwrap_or_else(|| {
                import
                    .module
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            });
            return TargetRef {
                name,
                import: Some(import),
            };
        }
        if node.kind() == "identifier" {
            if let Some(alias) = self
                .lookup(text(node, self.source))
                .and_then(|b| b.alias_of.clone())
            {
                return TargetRef {
                    name: alias,
                    import: None,
                };
            }
        }
        TargetRef {
            name: base_name(node, self.source),
            import: None,
        }
    }

    /// An `Extends` (or other) relation to the base `node`, carrying the
    /// import it was reached through as resolution evidence.
    fn push_link(&mut self, from: SymbolId, kind: RelationKind, node: Node) {
        let target = self.resolve_target(node);
        let relation = self.relations.len();
        self.push_relation(from, kind, target.name.clone(), location(node));
        if self.relations.len() == relation {
            return;
        }
        if let Some(import) = target.import {
            let mut segments = import.module.rsplit('.');
            let stem = segments.next().map(str::to_string);
            let directory = segments.next().map(str::to_string);
            self.relation_targets.push(RelationTarget {
                relation,
                module: directory,
                target_module: stem,
                kind: import.field.is_none().then_some(SymbolKind::Module),
                ..Default::default()
            });
        }
    }

    fn push_extends(&mut self, from: SymbolId, base: Node) {
        self.push_link(from, RelationKind::Extends, base);
    }

    /// `setmetatable(T, { __index = function(_, k) return Base[k] end })`
    /// delegates lookups to `Base` without declaring inheritance: a
    /// `References` relation from `T` to `Base`. A function that computes its
    /// result proves no target and records nothing.
    fn push_delegation(&mut self, var: Node, value: Node) {
        if value.kind() != "function_call"
            || value
                .child_by_field_name("name")
                .is_none_or(|c| c.kind() != "identifier" || text(c, self.source) != "setmetatable")
        {
            return;
        }
        let name = text(var, self.source).to_string();
        let Some(from) = self.symbol_id_of(&name) else {
            return;
        };
        if let Some(table) = self.delegate_table(value) {
            self.push_link(from, RelationKind::References, table);
        }
    }

    fn symbol_id_of(&self, name: &str) -> Option<SymbolId> {
        self.symbols
            .iter()
            .rev()
            .find(|s| s.id != FILE_MODULE && s.parent.is_none() && s.name == name)
            .map(|s| s.id)
    }

    /// The table a functional `__index` of a `setmetatable` call's metatable
    /// reads from, when the function is exactly `return Table[key]` with the
    /// key being its second parameter.
    fn delegate_table<'t>(&self, call: Node<'t>) -> Option<Node<'t>> {
        let arguments = call.child_by_field_name("arguments")?;
        let mut cursor = arguments.walk();
        let metatable = arguments.named_children(&mut cursor).nth(1)?;
        if metatable.kind() != "table_constructor" {
            return None;
        }
        let mut fields = metatable.walk();
        let function = metatable.named_children(&mut fields).find_map(|field| {
            let name = field.child_by_field_name("name")?;
            if text(name, self.source) != "__index" {
                return None;
            }
            field
                .child_by_field_name("value")
                .filter(|v| v.kind() == "function_definition")
        })?;
        let parameters = function.child_by_field_name("parameters")?;
        let mut names = parameters.walk();
        let key = parameters.named_children(&mut names).nth(1)?;
        let body = function.child_by_field_name("body")?;
        let mut statements = body.walk();
        let mut statements = body.named_children(&mut statements);
        let (Some(statement), None) = (statements.next(), statements.next()) else {
            return None;
        };
        if statement.kind() != "return_statement" {
            return None;
        }
        let mut returned = statement.walk();
        let list = statement
            .named_children(&mut returned)
            .find(|n| n.kind() == "expression_list")?;
        let mut items = list.walk();
        let mut items = list.named_children(&mut items);
        let (Some(index), None) = (items.next(), items.next()) else {
            return None;
        };
        if index.kind() != "bracket_index_expression" {
            return None;
        }
        let looked_up = index.child_by_field_name("field")?;
        if looked_up.kind() != "identifier"
            || text(looked_up, self.source) != text(key, self.source)
        {
            return None;
        }
        index
            .child_by_field_name("table")
            .filter(|t| matches!(t.kind(), "identifier" | "dot_index_expression"))
    }

    /// `setmetatable(Cat, { __index = Animal })` as a statement: `Cat` is a
    /// class, extending `Animal` when a base is named.
    fn visit_setmetatable(&mut self, call: Node) {
        if !self.at_file_scope() {
            return;
        }
        let Some(arguments) = call.child_by_field_name("arguments") else {
            return;
        };
        let mut cursor = arguments.walk();
        let Some(first) = arguments.named_children(&mut cursor).next() else {
            return;
        };
        if first.kind() != "identifier" {
            return;
        }
        let name = text(first, self.source).to_string();
        if self.metatable_base(call, 1).is_none() {
            if let (Some(from), Some(table)) = (self.symbol_id_of(&name), self.delegate_table(call))
            {
                self.push_link(from, RelationKind::References, table);
            }
            return;
        }
        let Some(class) = self.promote_to_class(&name) else {
            return;
        };
        if let Some(base) = self.metatable_base(call, 1) {
            self.push_extends(class, base);
        }
    }

    /// The base-class node of a `setmetatable(x, metatable)` call: the
    /// metatable argument itself (`setmetatable(Bird, Animal)`) or the value
    /// of its `__index` field (`{ __index = Animal }`), when that is a name.
    fn metatable_base<'t>(&self, call: Node<'t>, argument: usize) -> Option<Node<'t>> {
        let arguments = call.child_by_field_name("arguments")?;
        let mut cursor = arguments.walk();
        let metatable = arguments.named_children(&mut cursor).nth(argument)?;
        match metatable.kind() {
            "identifier" | "dot_index_expression" => Some(metatable),
            "function_call" if self.required_module(metatable).is_some() => Some(metatable),
            "table_constructor" => {
                let mut fields = metatable.walk();
                let found = metatable.named_children(&mut fields).find_map(|field| {
                    let name = field.child_by_field_name("name")?;
                    if text(name, self.source) != "__index" {
                        return None;
                    }
                    field.child_by_field_name("value").filter(|v| {
                        matches!(v.kind(), "identifier" | "dot_index_expression")
                            || (v.kind() == "function_call" && self.required_module(*v).is_some())
                    })
                });
                found
            }
            _ => None,
        }
    }

    /// A table constructor's fields: in a module table, `name = function …
    /// end` is a method of the table and a literal value is a field; any
    /// other value is visited for calls. A key that is not an identifier is
    /// named by its source: `["api-key"]` by the string, `[10]` and
    /// `[prefix .. "x"]` by the bracketed expression as written, so no value
    /// is invented for a computed key.
    fn visit_table(&mut self, table: Node, module: Option<String>, owner: SymbolId, depth: u32) {
        let mut cursor = table.walk();
        let fields: Vec<Node> = table.named_children(&mut cursor).collect();
        for field in fields {
            let name = field
                .child_by_field_name("name")
                .and_then(|key| self.key_name(key));
            let value = field.child_by_field_name("value");
            match (&module, name, value) {
                (Some(table_name), Some(name), Some(value))
                    if value.kind() == "function_definition" =>
                {
                    let id = self.push_symbol(
                        name,
                        SymbolKind::Method,
                        location(field),
                        Some(table_name.clone()),
                    );
                    self.visit_function_body(value, id, depth);
                }
                (Some(table_name), Some(name), Some(value)) if is_literal_value(value) => {
                    self.push_symbol(
                        name,
                        SymbolKind::Field,
                        location(field),
                        Some(table_name.clone()),
                    );
                    self.visit_children(field, owner, depth + 1);
                }
                _ => self.visit_children(field, owner, depth + 1),
            }
        }
    }

    /// The symbol name of a table key, or `None` when it names nothing.
    fn key_name(&self, key: Node) -> Option<String> {
        let name = match key.kind() {
            "identifier" => text(key, self.source).to_string(),
            "string" => text(key.child_by_field_name("content")?, self.source).to_string(),
            _ => format!(
                "[{}]",
                text(key, self.source)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        };
        (!name.is_empty()).then_some(name)
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            ..Default::default()
        }
    }

    /// `dofile("p")` (runs the file) or `loadfile("p")` (compiles it, runs
    /// nothing) with a literal path, when the callee is that bare global.
    /// Both take their path relative to the process working directory, which
    /// the source cannot prove: not the repository root, not this file's
    /// directory. The path is kept exactly as written and never resolved.
    fn file_loader(&self, callee: Node, name: &str, call: Node) -> Option<(RelationKind, String)> {
        if callee.kind() != "identifier" || self.lookup(name).is_some() {
            return None;
        }
        let kind = match name {
            "dofile" => RelationKind::Imports,
            "loadfile" => RelationKind::References,
            _ => return None,
        };
        let path = require_argument(call, self.source).filter(|p| !p.is_empty())?;
        Some((kind, path))
    }

    /// `require("a.b.c")` loads `a/b/c.lua`: an `Imports` relation to the
    /// `Module` symbol named `c` of a file under a `b` path segment. A
    /// package's `a/b/init.lua` is named `b` (see [`module_name_for`]), so
    /// `require("a.b")` matches it the same way.
    ///
    /// Returns whether the relation is named exactly like `module_path`
    /// (a single-segment path), so the caller's plain name relation would
    /// only duplicate it.
    fn push_module_import(&mut self, owner: SymbolId, module_path: &str, loc: Location) -> bool {
        let segments: Vec<&str> = module_path.split('.').collect();
        let Some(name) = segments.last().filter(|n| !n.is_empty()) else {
            return false;
        };
        if segments.iter().any(|s| s.is_empty() || s.contains('/')) {
            return false;
        }
        let module = segments
            .len()
            .checked_sub(2)
            .and_then(|i| segments.get(i))
            .map(|m| m.to_string());
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name.to_string(), loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module,
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
        segments.len() == 1
    }
}

fn is_literal_value(node: Node) -> bool {
    matches!(node.kind(), "number" | "string" | "true" | "false" | "nil")
        || (node.kind() == "unary_expression"
            && node
                .child_by_field_name("operand")
                .is_some_and(|operand| operand.kind() == "number"))
}

/// The file-level module symbol: the walker pushes it first.
const FILE_MODULE: SymbolId = 0;

/// `start`'s first line through `end`'s last: `x = function … end` spans
/// its target and its body.
fn span(start: Node, end: Node) -> Location {
    let mut loc = location(start);
    loc.end_line = Some(end.end_position().row as u32 + 1);
    loc.byte_len = end.end_byte().saturating_sub(start.start_byte()) as u32;
    loc
}

/// A target that names something (`M`, `M.sub`), not an indexed slot
/// (`t[#t + 1]`).
fn is_name_target(node: Node) -> bool {
    matches!(node.kind(), "identifier" | "dot_index_expression")
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

/// The last segment of a base-class expression: `Animal` for both
/// `Animal` and `zoo.Animal`.
fn base_name(node: Node, source: &str) -> String {
    text(node, source)
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `require('module_name')`'s argument (also the path literal of `dofile` and
/// `loadfile`): the literal module name, or `None`
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
