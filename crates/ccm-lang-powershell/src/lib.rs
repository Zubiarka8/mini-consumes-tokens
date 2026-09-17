//! `LanguageParser` implementation for PowerShell, via
//! `tree-sitter-powershell`.
//!
//! Node/field names below (`function_statement`, `function_name`, `command`,
//! `command_name`, `assignment_expression`, etc.) come from inspecting
//! `tree-sitter-powershell` 0.26's actual parse trees
//! (`tree_sitter::Node::to_sexp` plus a field-aware dump), not from grammar
//! docs — expression statements in this grammar are wrapped in a long,
//! undocumented chain of precedence nodes (`logical_expression` →
//! `bitwise_expression` → ... → `unary_expression`) between a `pipeline` and
//! the leaf it actually contains, which this walker deliberately never
//! names: it dispatches purely on `Node::kind()` and falls through every
//! unrecognized wrapper via the same generic recursion, so it doesn't have
//! to enumerate that chain.
//!
//! Deliberate scope, matching what static AST analysis of a script can
//! actually know without executing it:
//! - Every `command` invocation is recorded as a `Calls` relation to its
//!   literal command name — a function defined elsewhere in the repo or an
//!   external cmdlet/executable alike — same "unresolved external name"
//!   precedent every other language plugin in this workspace follows.
//! - Dot-sourcing (`. .\lib.ps1`) and `Import-Module <name>` become
//!   `Imports` when the target is a literal argument.
//! - Only *top-level* `$var = ...` assignments become `Variable` symbols —
//!   one assigned inside a function body is almost always local scratch
//!   state and would just add noise. A variable's scope prefix
//!   (`$script:x`, `$global:x`, ...) is stripped from the recorded name.
//! - PowerShell classes (`class Foo { ... }`) and their methods are **not**
//!   indexed — out of scope for what this plugin was asked to cover
//!   (systems-scripting `.ps1`/`.psm1` files), and classes are rare in that
//!   style of script. Only `function`-style definitions are.

use ccm_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct PowerShellParser;

impl LanguageParser for PowerShellParser {
    fn language_id(&self) -> &'static str {
        "powershell"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["ps1", "psm1"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_powershell::LANGUAGE` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_powershell::LANGUAGE.into())
            .expect("tree-sitter-powershell grammar is statically valid");

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
        let module_id = walker.push_symbol(module_name.clone(), SymbolKind::Module, location(root), None);
        walker.visit_children(root, module_id, &module_name, module_id, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".psm1")
        .trim_end_matches(".ps1")
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

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|c| c.kind() == kind)
}

fn find_descendant<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_descendant(child, kind) {
            return Some(found);
        }
    }
    None
}

/// `$Global:Foo` / `$script:Baz` / plain `$Foo` → `"Foo"`/`"Baz"`/`"Foo"` —
/// strips the sigil and a recognized scope prefix so the same variable
/// referenced with or without an explicit scope indexes under one name.
fn clean_variable_name(raw: &str) -> String {
    let without_sigil = raw.strip_prefix('$').unwrap_or(raw);
    if let Some((scope, rest)) = without_sigil.split_once(':') {
        let known_scope = matches!(
            scope.to_ascii_lowercase().as_str(),
            "global" | "script" | "local" | "private" | "using" | "env"
        );
        if known_scope && !rest.is_empty() {
            return rest.to_string();
        }
    }
    without_sigil.to_string()
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, symbols: Vec::new(), relations: Vec::new(), next_id: 0 }
    }

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location, parent: Option<String>) -> SymbolId {
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

    /// `owner` is the innermost enclosing function (calls/imports attach to
    /// it); `scope_name` is that function's name, or the file's module name
    /// at top level, used as `parent` for symbols declared directly here;
    /// `module_id` is this file's own module symbol id, used to tell
    /// top-level statements apart from ones nested inside a function body.
    fn visit_children(&mut self, node: Node, owner: SymbolId, scope_name: &str, module_id: SymbolId, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, scope_name, module_id, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, scope_name: &str, module_id: SymbolId, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_statement" => {
                let Some(name_node) = find_child(node, "function_name") else {
                    return;
                };
                let name = text(name_node, self.source).to_string();
                let id = self.push_symbol(name.clone(), SymbolKind::Function, location(node), Some(scope_name.to_string()));
                if let Some(body_stmts) = find_child(node, "script_block")
                    .and_then(|sb| sb.child_by_field_name("script_block_body"))
                    .and_then(|body| body.child_by_field_name("statement_list"))
                {
                    self.visit_children(body_stmts, id, &name, module_id, depth + 1);
                }
            }
            "assignment_expression" => {
                if owner == module_id {
                    if let Some(var_node) = find_child(node, "left_assignment_expression")
                        .and_then(|lhs| find_descendant(lhs, "variable"))
                    {
                        let name = clean_variable_name(text(var_node, self.source));
                        if !name.is_empty() {
                            self.push_symbol(name, SymbolKind::Variable, location(var_node), Some(scope_name.to_string()));
                        }
                    }
                }
                // Recurse to catch calls hiding in the assigned value
                // (`$x = Get-Something arg`); re-visiting the already
                // consumed left-hand side just bottoms out on a leaf
                // `variable` node with no children.
                self.visit_children(node, owner, scope_name, module_id, depth + 1);
            }
            "command" => {
                if let Some(name_field) = node.child_by_field_name("command_name") {
                    if let Some((cmd_text, name_node)) = command_literal_name(name_field, self.source) {
                        let is_dot_source = find_child(node, "command_invokation_operator")
                            .map(|op| text(op, self.source) == ".")
                            .unwrap_or(false);
                        if is_dot_source {
                            self.push_relation(owner, RelationKind::Imports, cmd_text, location(name_node));
                        } else if cmd_text.eq_ignore_ascii_case("Import-Module") {
                            if let Some(module) = first_command_argument_text(node, self.source) {
                                self.push_relation(owner, RelationKind::Imports, module, location(node));
                            }
                        } else {
                            self.push_relation(owner, RelationKind::Calls, cmd_text, location(name_node));
                        }
                    }
                }
                // Arguments can hide a `$(...)` subexpression containing
                // further commands/calls — recurse into everything so those
                // are still picked up.
                self.visit_children(node, owner, scope_name, module_id, depth + 1);
            }
            _ => self.visit_children(node, owner, scope_name, module_id, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

/// A `command`'s `command_name` field is either the literal name directly
/// (`command_name` node holding the text itself, the common case) or —
/// specifically for dot-sourcing — a `command_name_expr` wrapping an inner
/// `command_name` that holds the target path instead.
fn command_literal_name<'a>(name_field: Node<'a>, source: &str) -> Option<(String, Node<'a>)> {
    match name_field.kind() {
        "command_name" => Some((text(name_field, source).to_string(), name_field)),
        "command_name_expr" => {
            let inner = find_child(name_field, "command_name")?;
            let trimmed = text(inner, source).trim_matches(|c| c == '"' || c == '\'');
            Some((trimmed.to_string(), inner))
        }
        _ => None,
    }
}

/// The literal text of a `command`'s first real argument (skipping
/// whitespace separators and named `-Parameter` flags), quotes trimmed —
/// used for `Import-Module`'s target name.
fn first_command_argument_text(command_node: Node, source: &str) -> Option<String> {
    let elements = command_node.child_by_field_name("command_elements")?;
    let mut cursor = elements.walk();
    let candidates: Vec<Node> = elements.named_children(&mut cursor).collect();
    let arg = candidates
        .into_iter()
        .find(|c| c.kind() != "command_argument_sep" && c.kind() != "command_parameter")?;
    let trimmed = text(arg, source).trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
