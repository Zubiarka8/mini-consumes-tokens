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
    /// literal. One token per literal, even when the grammar splits it into
    /// several nodes (string fragments, escapes, interpolations' delimiters).
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

/// One token of a source file. Comments and whitespace are never tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
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

/// A whole file's tokens, in source order, plus where the grammar could not
/// make sense of the source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenizedFile {
    pub tokens: Vec<Token>,
    /// Line ranges covered by syntax-error or missing-token nodes. Tokens
    /// inside them are unreliable; an analysis must not treat a span that
    /// overlaps one as correctly tokenized.
    pub error_lines: Vec<LineRange>,
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
