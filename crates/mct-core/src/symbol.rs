/// Exact position of a symbol or reference within a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Byte length of the token, for callers that want the exact span.
    pub byte_len: u32,
    /// 1-based line number of the end of this symbol's node (e.g. the closing
    /// brace of a function/class body), when the `LanguageParser` populated
    /// it. `None` for relation sites (calls/references never carry one — only
    /// definitions do) and for any symbol from a parser not yet updated to
    /// populate it; nullable end-to-end (Rust field and SQL column alike) so
    /// existing parsers/indexes don't need a simultaneous flag day.
    pub end_line: Option<u32>,
}

/// Kind of a definable symbol. Deliberately a small, language-agnostic set —
/// each `LanguageParser` maps its own grammar's constructs onto these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Enum,
    Trait,
    TypeAlias,
    Module,
    Variable,
    Constant,
    Field,
    /// An HTML element with an `id` attribute — the only elements given a
    /// stable, searchable name (an element with only a `class` has no single
    /// unique name to index it under).
    Element,
    /// A CSS rule with a single simple selector (`.foo` or `#foo`, named
    /// exactly as written so it matches an HTML `Element`'s outgoing
    /// `References`). Compound/combinator selectors are not indexed — see
    /// `mct-lang-css`.
    Rule,
}

/// Within-file identifier used to link [`SymbolRelation`]s to the
/// [`SymbolRecord`]s a `LanguageParser` produced for the same file, before the
/// indexer assigns permanent database row ids.
pub type SymbolId = u32;

#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    /// Name of the enclosing symbol (e.g. the class containing a method), if any.
    pub parent: Option<String>,
}

/// Kind of relation between two symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationKind {
    Calls,
    Imports,
    Extends,
    Implements,
    References,
}

/// A relation from a symbol defined in this file to another symbol, which may
/// live in the same file, another file, or an unresolved external name (e.g. a
/// stdlib or third-party call) — resolution against the rest of the index
/// happens downstream, in the `mct-index` crate.
#[derive(Debug, Clone)]
pub struct SymbolRelation {
    pub from: SymbolId,
    pub kind: RelationKind,
    /// Name of the target symbol, as written at the call/reference site.
    pub to_name: String,
    pub location: Location,
}
