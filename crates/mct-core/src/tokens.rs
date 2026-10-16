//! Lexical token streams for on-demand source analyses (issue #207).
//!
//! Deliberately separate from [`crate::LanguageParser`]: indexing never
//! tokenizes, and a language without a [`TokenSource`] simply isn't covered by
//! the analyses that need one. A language crate opts in by implementing this
//! trait next to its parser; nothing here knows about tree-sitter.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ParseError;

/// Lexical category of one source token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A reserved word of the language (`fn`, `if`, `return`, `def`, …),
    /// including keyword-like literals the grammar treats as keywords
    /// (`true`, `None`, `self`) only when the grammar exposes them as such.
    Keyword,
    /// A name: variable, function, type, field, label, lifetime, macro name.
    Identifier,
    /// A literal value: number, string, char, byte string, boolean or null
    /// literal. Ordinary literals are atomic; structural token sources may
    /// emit literal fragments around interpolated expressions instead.
    Literal,
    /// An operator or structural punctuation (`+`, `->`, `{`, `;`, `,`, `::`).
    Punctuation,
    /// Synthetic, zero-width start of a block that only layout delimits (a
    /// Python indented block), so the same statements nested differently
    /// never yield the same token sequence. Not source text: it has empty
    /// [`Token::text`] and does not count as a token of the source.
    BlockStart,
    /// Synthetic, zero-width end of the block opened by the matching
    /// [`TokenKind::BlockStart`].
    BlockEnd,
}

impl TokenKind {
    /// Whether this kind is a synthetic block marker rather than source text.
    pub fn is_block_marker(self) -> bool {
        matches!(self, Self::BlockStart | Self::BlockEnd)
    }
}

/// Syntactic role of a token, read off its position in the syntax tree.
///
/// Roles describe syntax, never semantics: no binding, import, alias or type
/// is resolved. A token source that cannot tell a role leaves
/// [`TokenRole::Unclassified`], which analyses must treat as "unknown", not as
/// "ordinary variable".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TokenRole {
    /// No role rule applied: locals, references, paths, keywords, punctuation.
    #[default]
    Unclassified,
    /// Name declared by a callable definition.
    Declaration,
    /// Name declared in a parameter list.
    Parameter,
    /// Name selected from another value: attribute, field, method or keyword
    /// argument name.
    Member,
    /// Bare name in the callee position of a call or macro invocation.
    Callee,
    /// Name in an explicit type position (annotation or type identifier).
    Type,
    /// A literal, with its lexical category.
    Literal(LiteralCategory),
}

impl TokenRole {
    /// Stable kebab-case name for reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unclassified => "unclassified",
            Self::Declaration => "declaration",
            Self::Parameter => "parameter",
            Self::Member => "member",
            Self::Callee => "callee",
            Self::Type => "type",
            Self::Literal(category) => category.as_str(),
        }
    }
}

/// Lexical category of a literal, decided by its grammar node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiteralCategory {
    String,
    /// A string containing executable interpolation (a Python f-string).
    InterpolatedString,
    /// A string that is a whole expression statement (a Python docstring):
    /// its value is not used by the code around it.
    Docstring,
    Number,
    Boolean,
    Null,
    Character,
    /// A literal kind with no category entry, such as Python's `...`.
    Other,
}

impl LiteralCategory {
    /// Stable kebab-case name for reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::InterpolatedString => "interpolated-string",
            Self::Docstring => "docstring",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Null => "null",
            Self::Character => "character",
            Self::Other => "other-literal",
        }
    }
}

/// One token of a source file. Comments and whitespace are never tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Syntactic role; [`TokenRole::Unclassified`] when the source cannot tell.
    pub role: TokenRole,
    /// A recognized primitive spelling used as a complete direct annotation.
    /// Names may be shadowed: this is lexical evidence, never a resolved type.
    pub primitive_spelling: bool,
    /// The token's exact source text; empty for a block marker.
    pub text: String,
    /// 1-based line of the token's first byte (the same convention as
    /// [`crate::Location::line`]).
    pub line: u32,
    /// 1-based line of the token's last byte; differs from `line` only for a
    /// token spanning lines, such as a multi-line string literal.
    pub end_line: u32,
    /// Byte offset of the token's first byte in the tokenized source.
    pub start_byte: usize,
    /// Byte offset just past the token's last byte.
    pub end_byte: usize,
}

/// An inclusive, 1-based line range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// Exact callable node boundaries, independent of an index's line-based
/// locations, plus the syntax around the callable that its span excludes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableSpan {
    pub name: String,
    pub lines: LineRange,
    pub bytes: std::ops::Range<usize>,
    /// Decorators or outer attributes attached to the callable (`@cache`,
    /// `#[inline]`), in source order, whitespace collapsed. Not resolved: an
    /// imported alias and its original are different texts.
    pub decorators: Vec<String>,
    /// Enclosing constructs that the language's token source tracks, outermost
    /// first, each a stable label prefixed by that construct's own decorators
    /// or attributes (`class`, `#[cfg(test)] mod`, `if`, `try`). Empty for a
    /// callable at the top level.
    pub enclosing: Vec<String>,
    /// Statement shape of the callable's own body; `None` when the token
    /// source does not report one or the callable has no body.
    pub body: Option<BodyShape>,
}

/// How many statements sit directly in a callable's body, and how many of
/// them are a single flat call. Read off the syntax tree only: callee names
/// are never looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BodyShape {
    /// Direct statements of the body, a final expression included and
    /// docstrings excluded.
    pub statements: usize,
    /// Of `statements`, those that are exactly one call (or macro invocation)
    /// whose arguments contain no further call, closure, comprehension,
    /// conditional or block, and no nested macro delimiters.
    pub flat_calls: usize,
}

impl BodyShape {
    /// A non-empty body made only of flat calls.
    pub fn is_call_only(self) -> bool {
        self.statements > 0 && self.flat_calls == self.statements
    }
}

/// Tokens and optional exact callable boundaries from the same parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenizedFile {
    pub tokens: Vec<Token>,
    /// Line ranges covered by syntax-error or missing-token nodes. Tokens
    /// inside them are unreliable; an analysis must not treat a span that
    /// overlaps one as correctly tokenized.
    pub error_lines: Vec<LineRange>,
    /// `None` for legacy sources; `Some` promises exhaustive callable spans.
    /// Attributes/decorators outside the callable node are excluded from the
    /// span and reported in [`CallableSpan::decorators`] instead.
    pub callables: Option<Vec<CallableSpan>>,
}

/// Produces the token stream of one language's source files.
///
/// Implemented in the language's own crate, never in `mct-index`. Like
/// [`crate::LanguageParser::parse`], it runs over arbitrary, untrusted source:
/// it must be a purely static walk that never executes input, never panics
/// and bounds its recursion by [`crate::MAX_TRAVERSAL_DEPTH`].
pub trait TokenSource: Send + Sync {
    /// The same id as the language's [`crate::LanguageParser::language_id`],
    /// so an indexed file's `language` selects its token source.
    fn language_id(&self) -> &'static str;

    /// Every non-comment token of `source`, in source order, plus a
    /// [`TokenKind::BlockStart`]/[`TokenKind::BlockEnd`] pair around each
    /// block that only indentation delimits.
    ///
    /// Localized syntax errors are reported through
    /// [`TokenizedFile::error_lines`], not as an `Err`. `Err` is reserved for
    /// a file that cannot be tokenized at all (the grammar returns no tree);
    /// use [`ParseError::Syntax`] with `relative_path` and line `1`.
    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError>;

    /// Structural analyses may retain expressions embedded in literals.
    /// Sources without interpolated syntax can use the ordinary stream.
    fn tokenize_structural(
        &self,
        relative_path: &str,
        source: &str,
    ) -> Result<TokenizedFile, ParseError> {
        self.tokenize(relative_path, source)
    }
}

/// Lookup from language id to its [`TokenSource`].
#[derive(Default, Clone)]
pub struct TokenSourceRegistry {
    by_language: HashMap<&'static str, Arc<dyn TokenSource>>,
}

impl TokenSourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `source` under its [`TokenSource::language_id`].
    ///
    /// # Panics
    /// Panics if the language already has a token source — a startup-time
    /// configuration error, never reachable from repository content.
    pub fn register(&mut self, source: Arc<dyn TokenSource>) {
        let language = source.language_id();
        let previous = self.by_language.insert(language, source);
        assert!(
            previous.is_none(),
            "language `{language}` already has a registered token source"
        );
    }

    pub fn for_language(&self, language: &str) -> Option<&Arc<dyn TokenSource>> {
        self.by_language.get(language)
    }

    /// Registered language ids, sorted.
    pub fn language_ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.by_language.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    struct Fixed(&'static str);

    impl TokenSource for Fixed {
        fn language_id(&self) -> &'static str {
            self.0
        }

        fn tokenize(&self, _: &str, _: &str) -> Result<TokenizedFile, ParseError> {
            Ok(TokenizedFile::default())
        }
    }

    #[test]
    fn only_the_block_kinds_are_block_markers() {
        assert!(TokenKind::BlockStart.is_block_marker());
        assert!(TokenKind::BlockEnd.is_block_marker());
        for kind in [
            TokenKind::Keyword,
            TokenKind::Identifier,
            TokenKind::Literal,
            TokenKind::Punctuation,
        ] {
            assert!(!kind.is_block_marker(), "{kind:?}");
        }
    }

    #[test]
    fn sources_are_looked_up_by_language_and_listed_sorted() {
        let mut registry = TokenSourceRegistry::new();
        registry.register(Arc::new(Fixed("rust")));
        registry.register(Arc::new(Fixed("python")));
        assert_eq!(registry.language_ids(), ["python", "rust"]);
        assert_eq!(registry.for_language("rust").unwrap().language_id(), "rust");
        assert!(registry.for_language("go").is_none());
    }

    #[test]
    #[should_panic(expected = "already has a registered token source")]
    fn a_second_source_for_one_language_is_rejected() {
        let mut registry = TokenSourceRegistry::new();
        registry.register(Arc::new(Fixed("rust")));
        registry.register(Arc::new(Fixed("rust")));
    }
}
