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
    /// Writer-declared depth of this symbol, where the language has one
    /// (e.g. 1..6 for a Markdown ATX heading's `#`..`######`). Language-
    /// neutral and optional: every `LanguageParser` besides `mct-lang-md`
    /// leaves this `None`. Deliberately not derived from `parent` nesting —
    /// a document can skip levels (`##` directly followed by `####`), so
    /// nesting depth and declared level are not interchangeable.
    pub level: Option<u32>,
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

/// Parser-supplied evidence about which symbol a [`SymbolRelation`] targets,
/// beyond its bare `to_name`. Attached to a relation by index through
/// [`crate::ParsedFile::relation_targets`]; a relation with no entry is
/// unqualified (an empty constraint, not proof of anything).
///
/// The index resolves a relation against definitions named `to_name`,
/// restricted to the source file's language and then narrowed by every
/// field set here. Exactly one remaining candidate is `resolved`; several
/// are `ambiguous` (all kept as candidates, none picked); none is
/// `unresolved`. `external` is reported only when `external` is set.
/// Set a field only when the source proves it — a guess here turns into a
/// wrong resolved edge downstream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelationTarget {
    /// Index of the relation in [`crate::ParsedFile::relations`].
    pub relation: usize,
    /// Name of the symbol the target is declared in, matched against a
    /// candidate's `parent` (`A` for `A::run`, the impl type for
    /// `self.run()`). A parent written with generic arguments (`A<T>`)
    /// matches the bare qualifier `A`.
    pub qualifier: Option<String>,
    /// Repository-relative path (forward slashes) of the file the target is
    /// declared in, e.g. this same file for a lexically scoped call, or a
    /// note path for a Markdown link.
    pub path: Option<String>,
    /// Module path segment the source names the target through (`rand` in
    /// `rand::random()`, `m` in `crate::m::f()`). A candidate qualifies
    /// only if declared under a path component of that name (`m.rs`,
    /// `m/…`, `-` read as `_`) or inside a symbol of that name, so an
    /// unrelated same-named definition elsewhere — or none at all, for a
    /// third-party module — never resolves it.
    pub module: Option<String>,
    /// Reached through a receiver whose type the parser can't prove
    /// (`x.f()`): only a method can be the target, and even a single such
    /// candidate is reported ambiguous rather than resolved.
    pub member: bool,
    /// Language id the target is declared in, when the source names another
    /// language explicitly (an HTML `class` naming a CSS rule). `None` means
    /// the source file's own language — spelling alone never links two
    /// languages.
    pub language: Option<String>,
    /// The parser has positive evidence the target lies outside the indexed
    /// repository (e.g. Rust's `std::`). Never set merely because no
    /// definition matched.
    pub external: bool,
}
