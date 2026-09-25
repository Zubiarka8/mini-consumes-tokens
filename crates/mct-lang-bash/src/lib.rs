//! `LanguageParser` implementation for Bash/POSIX shell scripts, via
//! `tree-sitter-bash`.
//!
//! Node/field names below (`function_definition`, `command`, `command_name`,
//! `variable_assignment`, `declaration_command`, etc.) come from inspecting
//! `tree-sitter-bash` 0.25's actual parse trees (`tree_sitter::Node::to_sexp`),
//! not from grammar docs.
//!
//! Deliberate scope, matching what static AST analysis of a shell script can
//! actually know without executing it:
//! - Every `command` invocation is recorded as a `Calls` relation to its
//!   literal command name, whether that command turns out to be a function
//!   defined elsewhere in the repo (resolved downstream by `mct-index`) or
//!   an external program (`grep`, `curl`, ...) — same "unresolved external
//!   name" precedent every other language plugin in this workspace follows.
//! - `source file` / `. file` become `Imports` when the argument is a
//!   literal path (optionally containing an unexpanded `$VAR` reference,
//!   recorded as written) — a fully computed path (`source "$(find_lib)"`)
//!   has nothing static to record.
//! - Only *top-level* variable assignments become `Variable` symbols,
//!   including ones wrapped in `export`/`readonly`/`local`/`declare`
//!   (`declaration_command`) — a variable assigned inside a function body is
//!   almost always function-local scratch state and would just add noise.
//! - A command invoked through a variable or substitution (`$CMD arg`) has
//!   no static name to record and is silently skipped, same as an
//!   unresolvable callee in `mct-lang-go`/`mct-lang-js-ts`.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

pub struct BashParser;

impl LanguageParser for BashParser {
    fn language_id(&self) -> &'static str {
        "bash"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["sh", "bash"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_bash::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("tree-sitter-bash grammar is statically valid");

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
        .trim_end_matches(".bash")
        .trim_end_matches(".sh")
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
        Self { source, symbols: Vec::new(), relations: Vec::new(), next_id: 0 }
    }

    fn push_symbol(&mut self, name: String, kind: SymbolKind, location: Location, parent: Option<String>) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord { id, name, kind, location, parent, level: None });
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
            // Covers both `function foo() { ... }` and `foo() { ... }` —
            // same node kind either way, `name`/`body` fields don't depend
            // on which form was used.
            "function_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    return;
                }
                let id = self.push_symbol(name.clone(), SymbolKind::Function, location(node), Some(scope_name.to_string()));
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, &name, module_id, depth + 1);
                }
            }
            // Bare `FOO=bar`, or one wrapped in `declaration_command` for
            // `export`/`readonly`/`local`/`declare`/`typeset` — the wrapper
            // itself isn't matched here, it just falls through to the
            // default arm below and recurses straight into this node.
            "variable_assignment" => {
                if owner == module_id {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = text(name_node, self.source).to_string();
                        self.push_symbol(name, SymbolKind::Variable, location(node), Some(scope_name.to_string()));
                    }
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, owner, scope_name, module_id, depth + 1);
                }
            }
            "command" => {
                if let Some(name_field) = node.child_by_field_name("name") {
                    if let Some(word_node) = literal_word_child(name_field) {
                        let cmd_text = text(word_node, self.source);
                        match cmd_text {
                            "source" | "." => {
                                if let Some(path) = first_argument_text(node, self.source) {
                                    self.push_relation(owner, RelationKind::Imports, path, location(node));
                                }
                            }
                            _ => {
                                self.push_relation(owner, RelationKind::Calls, cmd_text.to_string(), location(word_node));
                            }
                        }
                    }
                }
                // Arguments can hide a command substitution (`$(...)` or
                // `` `...` ``) containing further commands/calls — recurse
                // into everything so those are still picked up.
                self.visit_children(node, owner, scope_name, module_id, depth + 1);
            }
            _ => self.visit_children(node, owner, scope_name, module_id, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

/// A `command`'s `name` field is a `command_name` node wrapping either a
/// literal `word` (a real command/function name) or something dynamic (a
/// variable expansion, string, etc.) — only the former has a static name
/// worth recording.
fn literal_word_child(command_name_node: Node) -> Option<Node> {
    let mut cursor = command_name_node.walk();
    let child = command_name_node.named_children(&mut cursor).next()?;
    if child.kind() == "word" {
        Some(child)
    } else {
        None
    }
}

/// The literal text of a `command`'s first `argument` field, quotes
/// trimmed — used for `source`/`.`'s target path.
fn first_argument_text(command_node: Node, source: &str) -> Option<String> {
    let arg = command_node.child_by_field_name("argument")?;
    let trimmed = text(arg, source).trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
