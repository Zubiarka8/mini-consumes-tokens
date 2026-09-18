//! Core domain model and the `LanguageParser` plugin trait.
//!
//! This crate is intentionally language-agnostic and storage-agnostic: it knows
//! nothing about tree-sitter grammars, SQLite, or MCP. Adding support for a new
//! language means implementing [`LanguageParser`] in a new crate and registering
//! it with a [`LanguageRegistry`] — no changes here.

mod error;
mod registry;
mod symbol;

pub use error::ParseError;
pub use registry::LanguageRegistry;
pub use symbol::{
    Location, RelationKind, SymbolId, SymbolKind, SymbolRecord, SymbolRelation,
};

/// Recursion-depth ceiling every `mct-lang-*` AST walker must enforce while
/// descending into a node's children. Parsing runs over arbitrary,
/// potentially adversarial source (see [`LanguageParser::parse`]); tree-sitter
/// happily produces deeply right- or left-nested trees for pathological input
/// (e.g. thousands of nested parentheses), and an unguarded recursive walker
/// crashes the process with a native stack overflow well before that. 256
/// comfortably covers real-world code (deeply nested real syntax is rare
/// past a few dozen levels) while keeping each walker's native stack usage
/// bounded regardless of input.
pub const MAX_TRAVERSAL_DEPTH: u32 = 256;

/// Hard ceiling on how many hops a graph-traversal MCP query (`find_calls`,
/// `find_callers`, `find_references`, `impact_analysis`) is allowed to walk,
/// regardless of the caller-supplied `depth` parameter. Without this, a
/// pathological or malicious `depth` value could make the BFS fan out across
/// the entire relation graph. 32 comfortably covers any real blast-radius
/// investigation while keeping worst-case query cost bounded.
pub const MAX_QUERY_DEPTH: u32 = 32;

/// A single source file to be parsed, relative to the project root.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path relative to the project root, using forward slashes regardless of OS,
    /// so that the index is portable across platforms.
    pub relative_path: String,
    pub contents: String,
}

/// Everything extracted from one source file: its symbols and the relations
/// between them (or to symbols in other files, resolved by name).
#[derive(Debug, Clone, Default)]
pub struct ParsedFile {
    pub symbols: Vec<SymbolRecord>,
    pub relations: Vec<SymbolRelation>,
}

/// Implemented once per supported language, in its own crate. The indexer
/// resolves which parser to use for a given file purely by extension via
/// [`LanguageParser::file_extensions`] — callers never need to know or specify
/// the language of a file up front.
pub trait LanguageParser: Send + Sync {
    /// Stable identifier stored in the `language` column of the index
    /// (e.g. `"rust"`, `"python"`). Must be unique across all registered parsers.
    fn language_id(&self) -> &'static str;

    /// File extensions this parser handles, without the leading dot
    /// (e.g. `["rs"]`, `["py", "pyi"]`).
    fn file_extensions(&self) -> &'static [&'static str];

    /// Parse one file's contents into symbols and relations.
    ///
    /// Must never execute or evaluate any part of `file.contents` — parsing is
    /// purely static (AST-based) for every language. A syntax error in the
    /// input must be returned as [`ParseError::Syntax`], not a panic: this
    /// method runs over arbitrary, untrusted third-party source code.
    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError>;
}
