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
//!   `Imports` when the target is a literal argument. A computed target
//!   (`Import-Module (Join-Path $PSScriptRoot 'X.psm1')`,
//!   `. "$PSScriptRoot/lib.ps1"`) is not evaluated and records no import.
//! - Only *top-level* `$var = ...` assignments become `Variable` symbols —
//!   one assigned inside a function or class body is almost always local
//!   scratch state and would just add noise, and `$x.Name = ...` or
//!   `$x[0] = ...` changes an existing value rather than declaring `x`.
//!   A variable's scope prefix
//!   (`$script:x`, `$global:x`, ...) is stripped from the recorded name.
//! - A `class` is a `Class`, its properties `Field`s and its methods
//!   (constructors included, named after the class) `Method`s; an `enum` is
//!   an `Enum` whose members are `Field`s. A method owns the commands its
//!   body runs. A command invocation names a function, cmdlet or executable,
//!   never a method, so its target must be a `Function`.

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolDetail, SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_child, first_error, location, module_location};
use tree_sitter::{Node, Parser};

mod details;

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

        let mut tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree".to_string(),
            })?;

        if tree.root_node().has_error() {
            let mut normalized = file.contents.as_bytes().to_vec();
            if normalize_identifier_comma_arguments(
                tree.root_node(),
                &file.contents,
                &mut normalized,
                0,
            ) {
                if let Some(reparsed) = parser.parse(&normalized, None) {
                    if !reparsed.root_node().has_error() {
                        tree = reparsed;
                    }
                }
            }
        }

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
        let mut walker = Walker::new(&file.contents, &file.relative_path);
        let module_id = walker.push_symbol(
            module_name.clone(),
            SymbolKind::Module,
            module_location(root),
            None,
        );
        walker.visit_children(root, module_id, &module_name, module_id, 0);
        Ok(walker.finish())
    }
}

/// Repair only the pinned grammar's missing separator between bare identifiers
/// in command arguments. PowerShell permits comma lists for cmdlets and literal
/// commas for native commands. A full reparse must accept the entire file.
/// One-byte substitutions preserve locations; extraction reads original text.
fn normalize_identifier_comma_arguments(
    node: Node,
    source: &str,
    normalized: &mut [u8],
    depth: u32,
) -> bool {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return false;
    }
    let mut changed = false;
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    if node.kind() == "command_elements" {
        for group in children.windows(3) {
            let [previous, separator, next] = group else {
                continue;
            };
            if previous.kind() != "generic_token"
                || separator.kind() != "command_argument_sep"
                || next.kind() != "generic_token"
                || separator.start_byte() != separator.end_byte()
                || previous.end_byte() != separator.start_byte()
                || next.start_byte() != separator.end_byte()
                || separator.child_count() != 1
                || !separator
                    .child(0)
                    .is_some_and(|child| child.is_missing() && child.kind() == " ")
            {
                continue;
            }
            let before = text(*previous, source).trim_start_matches(',');
            let after = text(*next, source);
            if simple_identifier(before) && after.strip_prefix(',').is_some_and(simple_identifier) {
                if let Some(byte) = normalized.get_mut(next.start_byte()) {
                    *byte = b'_';
                    changed = true;
                }
            }
        }
    }
    for child in children {
        changed |= normalize_identifier_comma_arguments(child, source, normalized, depth + 1);
    }
    changed
}

fn simple_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn module_name_for(relative_path: &str) -> String {
    mct_tree_sitter::module_name(relative_path, &[".psm1", ".ps1"])
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
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

/// Whether `var` sits under a `member_access` or `element_access` within the
/// assignment target `lhs`: `$x.Name = …` and `$x[0] = …` change a value of
/// `x` rather than assign `x` itself.
fn is_member_or_element_target(lhs: Node, var: Node) -> bool {
    let mut node = var.parent();
    while let Some(current) = node {
        if current.id() == lhs.id() {
            return false;
        }
        if matches!(current.kind(), "member_access" | "element_access") {
            return true;
        }
        node = current.parent();
    }
    false
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

#[derive(Default)]
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    details: Vec<SymbolDetail>,
    /// This file's repository-relative path, the base of relative sources.
    path: &'a str,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            ..Self::default()
        }
    }

    fn push_symbol(
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

    /// `owner` is the innermost enclosing function (calls/imports attach to
    /// it); `scope_name` is that function's name, or the file's module name
    /// at top level, used as `parent` for symbols declared directly here;
    /// `module_id` is this file's own module symbol id, used to tell
    /// top-level statements apart from ones nested inside a function body.
    fn visit_children(
        &mut self,
        node: Node,
        owner: SymbolId,
        scope_name: &str,
        module_id: SymbolId,
        depth: u32,
    ) {
        mct_tree_sitter::for_each_child(node, |child| {
            self.visit(child, owner, scope_name, module_id, depth + 1);
        });
    }

    fn visit(
        &mut self,
        node: Node,
        owner: SymbolId,
        scope_name: &str,
        module_id: SymbolId,
        depth: u32,
    ) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "function_statement" => {
                let Some(name_node) = find_child(node, "function_name") else {
                    return;
                };
                let name = text(name_node, self.source).to_string();
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Function,
                    location(node),
                    Some(scope_name.to_string()),
                );
                self.details
                    .push(details::function_detail(node, id, self.source));
                // The whole block: a plain statement list or a
                // `begin`/`process`/`end` named block list, plus `param()`.
                if let Some(block) = find_child(node, "script_block") {
                    self.visit_children(block, id, &name, module_id, depth + 1);
                }
            }
            "class_statement" => self.visit_class(node, scope_name, module_id, depth),
            "enum_statement" => self.visit_enum(node, scope_name),
            "assignment_expression" => {
                // Inside a class the owner is the class or a method, so an
                // assignment there is a property value or a method local.
                if owner == module_id {
                    if let Some((lhs, var_node)) = find_child(node, "left_assignment_expression")
                        .and_then(|lhs| Some((lhs, find_descendant(lhs, "variable")?)))
                        .filter(|(lhs, var_node)| !is_member_or_element_target(*lhs, *var_node))
                    {
                        let name = clean_variable_name(text(var_node, self.source));
                        if !name.is_empty() {
                            let id = self.push_symbol(
                                name,
                                SymbolKind::Variable,
                                location(var_node),
                                Some(scope_name.to_string()),
                            );
                            self.details
                                .push(details::variable_detail(lhs, id, self.source));
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
                    if let Some((cmd_text, name_node)) =
                        command_literal_name(name_field, self.source)
                    {
                        let is_dot_source = find_child(node, "command_invokation_operator")
                            .map(|op| text(op, self.source) == ".")
                            .unwrap_or(false);
                        if is_dot_source {
                            self.push_file_import(owner, &cmd_text, location(name_node));
                            self.push_relation(
                                owner,
                                RelationKind::Imports,
                                cmd_text,
                                location(name_node),
                            );
                        } else if cmd_text.eq_ignore_ascii_case("Import-Module") {
                            for module in first_command_argument_texts(node, self.source) {
                                self.push_file_import(owner, &module, location(node));
                                self.push_relation(
                                    owner,
                                    RelationKind::Imports,
                                    module,
                                    location(node),
                                );
                            }
                        } else {
                            let relation = self.relations.len();
                            self.push_relation(
                                owner,
                                RelationKind::Calls,
                                cmd_text,
                                location(name_node),
                            );
                            self.relation_targets.push(RelationTarget {
                                relation,
                                kind: Some(SymbolKind::Function),
                                ..Default::default()
                            });
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

    /// `class Name : Base, IFoo { … }`: the first `simple_name` is the
    /// class's own name, the rest its bases. A property initializer runs as
    /// the class; a method body as the method.
    fn visit_class(&mut self, node: Node, scope_name: &str, module_id: SymbolId, depth: u32) {
        let Some(name_node) = find_child(node, "simple_name") else {
            return;
        };
        let class_name = text(name_node, self.source).to_string();
        let class_id = self.push_symbol(
            class_name.clone(),
            SymbolKind::Class,
            location(node),
            Some(scope_name.to_string()),
        );
        self.details
            .push(details::header_detail(node, class_id, self.source));
        mct_tree_sitter::for_each_child(node, |member| match member.kind() {
            "class_property_definition" => {
                let Some(variable) = find_child(member, "variable") else {
                    return;
                };
                let name = clean_variable_name(text(variable, self.source));
                if name.is_empty() {
                    return;
                }
                let id = self.push_symbol(
                    name,
                    SymbolKind::Field,
                    location(member),
                    Some(class_name.clone()),
                );
                self.details
                    .push(details::property_detail(member, variable, id, self.source));
                self.visit_children(member, class_id, &class_name, module_id, depth + 1);
            }
            "class_method_definition" => {
                let Some(method_name) = find_child(member, "simple_name") else {
                    return;
                };
                let name = text(method_name, self.source).to_string();
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Method,
                    location(member),
                    Some(class_name.clone()),
                );
                self.details
                    .push(details::method_detail(member, id, self.source));
                if let Some(block) = find_child(member, "script_block") {
                    self.visit_children(block, id, &name, module_id, depth + 1);
                }
            }
            _ => {}
        });
    }

    /// `enum Name { A; B = 2 }`: each `enum_member` is a field of the enum.
    fn visit_enum(&mut self, node: Node, scope_name: &str) {
        let Some(name_node) = find_child(node, "simple_name") else {
            return;
        };
        let enum_name = text(name_node, self.source).to_string();
        let enum_id = self.push_symbol(
            enum_name.clone(),
            SymbolKind::Enum,
            location(node),
            Some(scope_name.to_string()),
        );
        self.details
            .push(details::header_detail(node, enum_id, self.source));
        mct_tree_sitter::for_each_child(node, |member| {
            if member.kind() != "enum_member" {
                return;
            }
            let Some(member_name) = find_child(member, "simple_name") else {
                return;
            };
            let id = self.push_symbol(
                text(member_name, self.source).to_string(),
                SymbolKind::Field,
                location(member),
                Some(enum_name.clone()),
            );
            self.details
                .push(details::member_detail(member, id, self.source));
        });
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            symbol_details: self.details,
            ..Default::default()
        }
    }

    /// Dot-sourcing or `Import-Module` of a script/module file by relative
    /// path depends on that file: an `Imports` relation to its `Module`
    /// symbol, pinned to the exact path. The path is read against the
    /// importing script's directory (PowerShell resolves it against the
    /// working directory); a mismatch leaves the import unresolved.
    fn push_file_import(&mut self, owner: SymbolId, spec: &str, loc: Location) {
        let spec = spec.replace('\\', "/");
        let file = spec.rsplit('/').next().unwrap_or(&spec);
        let Some(stem) = file
            .strip_suffix(".psm1")
            .or_else(|| file.strip_suffix(".ps1"))
            .filter(|stem| !stem.is_empty())
        else {
            return;
        };
        if spec.contains(['$', '`', '*', '(', '~']) || spec.starts_with('/') || spec.contains(':') {
            return;
        }
        let relative = if spec.starts_with("./") || spec.starts_with("../") {
            spec.clone()
        } else {
            format!("./{spec}")
        };
        let Some(resolved) = mct_core::resolve_reference_path(self.path, &relative) else {
            return;
        };
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, stem.to_string(), loc);
        self.relation_targets.push(RelationTarget {
            relation,
            path: Some(resolved),
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
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
fn first_command_argument_texts(command_node: Node, source: &str) -> Vec<String> {
    first_command_argument_node(command_node)
        .map(|arg| {
            let raw = text(arg, source);
            if arg.kind() == "generic_token"
                && raw.contains(',')
                && raw.split(',').all(simple_identifier)
            {
                return raw.split(',').map(str::to_string).collect();
            }
            let trimmed = raw.trim_matches(|c| c == '"' || c == '\'');
            if trimmed.is_empty() || trimmed.contains('$') || trimmed.starts_with(['(', '@']) {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        })
        .unwrap_or_default()
}

fn first_command_argument_node(command_node: Node) -> Option<Node> {
    let elements = command_node.child_by_field_name("command_elements")?;
    let mut cursor = elements.walk();
    let candidates: Vec<Node> = elements.named_children(&mut cursor).collect();
    let arg = candidates
        .into_iter()
        .find(|c| c.kind() != "command_argument_sep" && c.kind() != "command_parameter")?;
    Some(arg)
}
