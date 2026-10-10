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
//! `walker::Walker` handle all three without per-grammar branching — only
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
//! Other known limits: interface members are not symbols, and a TS
//! `namespace` is a `Module` symbol whose functions stay top-level (no
//! parent). An object literal is not a type, so its methods are `Function`s.
//!
//! CommonJS (`require`/`module.exports`) and ES modules (`import`/`export`)
//! are handled by two independent sets of match arms on the node kind found —
//! both can appear in the same file (a common real-world interop pattern),
//! and neither requires knowing up front which module system a file uses.

use mct_core::{LanguageParser, Location, ParseError, ParsedFile, SourceFile, SymbolKind};
use mct_tree_sitter::{first_error, location};
use tree_sitter::{Node, Parser};

mod walker;

use walker::Walker;

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
