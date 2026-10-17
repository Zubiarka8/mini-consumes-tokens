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
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{first_error, location};
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

        let mut tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree".to_string(),
            })?;

        // tree-sitter-bash rejects literal semicolons in double-quoted
        // ${name:+word}. Reparse that precise grammar gap with same-width
        // word characters, retaining original source for extraction.
        if tree.root_node().has_error() {
            if let Some(contents) =
                quoted_alternative_parse_buffer(tree.root_node(), &file.contents)
            {
                if let Some(reparsed) = parser.parse(&contents, None) {
                    tree = reparsed;
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
        let mut module_location = location(root);
        // The root node of a file ending in a newline ends at column 0 of
        // the (empty) line after the last one; the file's last line is the
        // one before that.
        let end = root.end_position();
        if end.column == 0 && end.row > 0 {
            module_location.end_line = Some(end.row as u32);
        }
        let module_id = walker.push_symbol(
            module_name.clone(),
            SymbolKind::Module,
            module_location,
            None,
        );
        walker.visit_children(root, module_id, &module_name, module_id, 0);
        Ok(walker.finish())
    }
}

/// Only repairs the observed grammar gap. The reparsed tree must still pass
/// the normal syntax check, including errors in nested command substitutions.
fn quoted_alternative_parse_buffer(root: Node, source: &str) -> Option<String> {
    let mut bytes = source.as_bytes().to_vec();
    let mut changed = false;
    let mut pending = vec![(root, 0)];
    while let Some((node, depth)) = pending.pop() {
        if depth >= MAX_TRAVERSAL_DEPTH {
            continue;
        }
        if node.is_error() && text(node, source) == ";" {
            let concatenation = node.parent()?;
            let expansion = concatenation.parent()?;
            let string = expansion.parent()?;
            let mut cursor = expansion.walk();
            let has_alternative = expansion
                .children(&mut cursor)
                .any(|child| child.kind() == ":+");
            if concatenation.kind() == "concatenation"
                && expansion.kind() == "expansion"
                && string.kind() == "string"
                && has_alternative
                && expansion.child(0).is_some_and(|child| child.kind() == "${")
                && expansion
                    .child(expansion.child_count().saturating_sub(1))
                    .is_some_and(|child| child.kind() == "}" && !child.is_missing())
            {
                if let Some(byte) = bytes.get_mut(node.start_byte()) {
                    *byte = b'_';
                    changed = true;
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() {
                pending.push((child, depth + 1));
            }
        }
    }
    changed.then(|| String::from_utf8(bytes).ok()).flatten()
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

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    /// This file's repository-relative path, the base of relative sources.
    path: &'a str,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            symbols: Vec::new(),
            relations: Vec::new(),
            relation_targets: Vec::new(),
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
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, scope_name, module_id, depth + 1);
        }
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
                let id = self.push_symbol(
                    name.clone(),
                    SymbolKind::Function,
                    location(node),
                    Some(scope_name.to_string()),
                );
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
                        self.push_symbol(
                            name,
                            SymbolKind::Variable,
                            location(node),
                            Some(scope_name.to_string()),
                        );
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
                                    self.push_source_target(owner, &path, location(node));
                                    self.push_relation(
                                        owner,
                                        RelationKind::Imports,
                                        path,
                                        location(node),
                                    );
                                }
                            }
                            _ => {
                                self.push_relation(
                                    owner,
                                    RelationKind::Calls,
                                    cmd_text.to_string(),
                                    location(word_node),
                                );
                            }
                        }
                    }
                }
                // Arguments can hide a command substitution (`$(...)` or
                // `` `...` ``) containing further commands/calls — recurse
                // into everything so those are still picked up. A prefix
                // assignment (`LC_ALL=C sort`) only sets that one command's
                // environment, so it is not a script-level variable: walk
                // its value, but don't record it as a symbol.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_assignment" {
                        if let Some(value) = child.child_by_field_name("value") {
                            self.visit(value, owner, scope_name, module_id, depth + 2);
                        }
                    } else {
                        self.visit(child, owner, scope_name, module_id, depth + 1);
                    }
                }
            }
            _ => self.visit_children(node, owner, scope_name, module_id, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            ..Default::default()
        }
    }

    /// `source dir/x.sh` depends on the file `x.sh`: an `Imports` relation
    /// to its `Module` symbol (see [`source_target`] for what is proven).
    fn push_source_target(&mut self, owner: SymbolId, spec: &str, loc: Location) {
        let Some((name, target)) = source_target(self.path, spec) else {
            return;
        };
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, name, loc);
        self.relation_targets
            .push(RelationTarget { relation, ..target });
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
/// Expressions that evaluate to the sourcing script's own directory.
const SCRIPT_DIR_PREFIXES: [&str; 7] = [
    "$(dirname \"$0\")",
    "$(dirname $0)",
    "$(dirname \"${BASH_SOURCE[0]}\")",
    "$(dirname ${BASH_SOURCE[0]})",
    "${BASH_SOURCE[0]%/*}",
    "${BASH_SOURCE%/*}",
    "${0%/*}",
];

/// The relation name and evidence for `source SPEC` written in `from_file`,
/// or `None` when the file name proves nothing (not a `.sh`/`.bash` name, or
/// a computed one).
///
/// - A literal relative path, or one after a script-directory idiom
///   (`$(dirname "$0")/lib.sh`), is pinned to the exact file, read against the
///   sourcing script's directory (a plain relative `source` is really
///   resolved against the working directory and `PATH`; a mismatch leaves it
///   unresolved).
/// - A path through any other variable (`$LIB_DIR/log.sh`) proves only the
///   file stem and, if literal, its parent directory.
/// - An absolute or `~` path proves nothing about the repository.
fn source_target(from_file: &str, spec: &str) -> Option<(String, RelationTarget)> {
    let spec = spec.trim();
    let file = spec.rsplit('/').next().unwrap_or(spec);
    let stem = file
        .strip_suffix(".bash")
        .or_else(|| file.strip_suffix(".sh"))?;
    if stem.is_empty() || file.contains(['$', '`', '*', '{', '(', ' ']) {
        return None;
    }
    let module_kind = Some(SymbolKind::Module);
    let script_relative = SCRIPT_DIR_PREFIXES
        .iter()
        .find_map(|prefix| spec.strip_prefix(prefix))
        .filter(|rest| rest.starts_with('/'))
        .map(|rest| format!(".{rest}"));
    let literal = !spec.contains(['$', '`', '*', '{', '(', '~']) && !spec.starts_with('/');
    let relative = script_relative.or_else(|| {
        literal.then(|| {
            if spec.starts_with("./") || spec.starts_with("../") {
                spec.to_string()
            } else {
                format!("./{spec}")
            }
        })
    });
    if let Some(relative) = relative {
        let resolved = mct_core::resolve_reference_path(from_file, &relative)?;
        return Some((
            stem.to_string(),
            RelationTarget {
                path: Some(resolved),
                kind: module_kind,
                ..Default::default()
            },
        ));
    }
    if spec.starts_with(['/', '~']) {
        return None;
    }
    let dir = spec
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or_default();
    let parent = dir.rsplit('/').next().filter(|d| {
        !d.is_empty() && !d.contains(['$', '`', '*', '{', '(', '~']) && *d != "." && *d != ".."
    });
    Some((
        stem.to_string(),
        RelationTarget {
            module: parent.map(str::to_string),
            kind: module_kind,
            ..Default::default()
        },
    ))
}

fn first_argument_text(command_node: Node, source: &str) -> Option<String> {
    let arg = command_node.child_by_field_name("argument")?;
    let trimmed = text(arg, source).trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
