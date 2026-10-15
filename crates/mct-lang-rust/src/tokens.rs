//! [`TokenSource`] for Rust, over the `tree-sitter-rust` 0.24 node kinds.
//!
//! The kind tables below were read off the real grammar (its `node-types.json`
//! and parsed probes), not guessed:
//!
//! - **Trivia:** `line_comment` and `block_comment` (doc comments are
//!   `line_comment`s with `doc_comment` children, dropped with them),
//!   `shebang`, and `visibility_modifier` (`pub`, `pub(crate)`,
//!   `pub(in path)`): a visibility decides who may use an item, not what it
//!   does, so a private function and its `pub` copy must compare equal.
//! - **Literals**, one token each: `string_literal` (also byte and C strings,
//!   with their `string_content`/`escape_sequence` children),
//!   `raw_string_literal`, `char_literal` (also byte chars), `integer_literal`,
//!   `float_literal` and `boolean_literal`. A negative literal pattern `-1` is
//!   the punctuation `-` followed by the literal.
//! - **Identifiers:** `identifier`, `type_identifier`, `field_identifier`,
//!   `shorthand_field_identifier`, `metavariable` (`$x`) and `primitive_type`
//!   (`i32`, `str`: a type's name, and names containing digits could not be
//!   keywords under the letters-only rule). A lifetime or loop label is the
//!   punctuation `'` followed by an identifier.
//! - **Keywords:** every other alphabetic leaf, including `self`, `super`,
//!   `crate`, `mut` and `_`, plus the single token `macro_rules!`.
//!
//! Macro invocation bodies (`token_tree`) are tokenized like ordinary code:
//! the grammar already splits them into identifiers, literals and punctuation.
//! No block markers: every Rust block has its own `{`/`}` tokens.

use mct_core::{ParseError, TokenSource, TokenizedFile};
use mct_tree_sitter::{tokenize_source, TokenClasses};

const CLASSES: TokenClasses = TokenClasses {
    trivia_kinds: &[
        "line_comment",
        "block_comment",
        "shebang",
        "visibility_modifier",
    ],
    literal_kinds: &[
        "string_literal",
        "raw_string_literal",
        "char_literal",
        "integer_literal",
        "float_literal",
        "boolean_literal",
    ],
    identifier_kinds: &[
        "identifier",
        "type_identifier",
        "field_identifier",
        "shorthand_field_identifier",
        "metavariable",
        "primitive_type",
    ],
    keyword_kinds: &["macro_rules!"],
    block_kinds: &[],
};

/// Token source for Rust files.
pub struct RustTokenSource;

impl TokenSource for RustTokenSource {
    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source(
            &tree_sitter_rust::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
        )
    }
}
