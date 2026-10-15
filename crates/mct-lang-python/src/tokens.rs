//! [`TokenSource`] for Python, over the `tree-sitter-python` 0.25 node kinds.
//!
//! The kind tables below were read off the real grammar (its `node-types.json`
//! and parsed probes), not guessed:
//!
//! - **Trivia:** `comment` (a shebang line is one too) and `line_continuation`
//!   (the backslash that joins two lines).
//! - **Literals**, one token each: `string` (plain, raw, bytes, triple-quoted
//!   and f-strings, whatever their `string_start`/`string_content`/
//!   `interpolation`/`escape_sequence`/`string_end` children), `integer`,
//!   `float`, and the constants `true`, `false`, `none` (`True`, `False`,
//!   `None`) and `ellipsis` (`...`). The constants are literals, not keywords,
//!   so a clone detector sees `x = None` and `x = 0` as the same shape. Implicit
//!   concatenation (`"a" "b"`) is one literal per string.
//! - **Identifiers:** `identifier`, which also covers `self`, `cls`, builtin
//!   names and soft keywords used as names (`match`, `type`).
//! - **Keywords:** every other alphabetic leaf: `def`, `class`, `if`, `return`,
//!   `async`, `await`, `lambda`, `not`, `and`, `in`, `is`, `pass`, `break`, ...
//!
//! - **Blocks:** `block`, the body of every compound statement (indented or a
//!   one-line suite). The grammar's indent/dedent markers are hidden, so
//!   without a [`mct_core::TokenKind::BlockStart`]/`BlockEnd` pair around each
//!   `block`, a statement moved in or out of an `if` would leave the token
//!   sequence unchanged although the behavior changes. Indentation itself
//!   still produces no token, so a one-line suite and its indented form match.

use mct_core::{ParseError, TokenSource, TokenizedFile};
use mct_tree_sitter::{tokenize_source, TokenClasses};

const CLASSES: TokenClasses = TokenClasses {
    trivia_kinds: &["comment", "line_continuation"],
    literal_kinds: &[
        "string", "integer", "float", "true", "false", "none", "ellipsis",
    ],
    identifier_kinds: &["identifier"],
    keyword_kinds: &[],
    block_kinds: &["block"],
};

/// Token source for Python files.
pub struct PythonTokenSource;

impl TokenSource for PythonTokenSource {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source(
            &tree_sitter_python::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
        )
    }
}
