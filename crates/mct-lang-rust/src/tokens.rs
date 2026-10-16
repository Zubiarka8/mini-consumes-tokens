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
//!
//! - **Roles** (syntactic only; nothing is resolved): `type_identifier` and
//!   `primitive_type` are types; `field_identifier` (fields and method calls)
//!   and `shorthand_field_identifier` are members; the bare `function` of a
//!   call and the `macro` of an invocation are callees; a `parameter` pattern
//!   and a closure parameter are parameters; a function name is a
//!   declaration. Path segments (`a::b`), `mut` patterns and locals stay
//!   unclassified. Format strings inside macros are ordinary string literals:
//!   their `{name}` placeholders are not analyzed.
//! - **Callable context:** outer `attribute_item`s (`#[test]`, `#[cfg(..)]`)
//!   before a function attach to it, across comments; enclosing modules,
//!   `extern` blocks, impls, traits, functions, closures and control
//!   expressions are labeled, an attributed module as `#[cfg(test)] mod`.
//!   Only kinds are labeled: impl targets, trait names and conditions are not.
//!   Inner attributes (`#![..]`) are not collected.
//! - **Body shape:** the named children of a function's `body` block, its tail
//!   expression included and attributes and comments excluded. One is a flat
//!   call when it is a `call_expression` or `macro_invocation`, bare or in an
//!   `expression_statement`, whose subtree has no other call or invocation, no
//!   closure, block, control expression, `?` or `.await`, and no `token_tree`
//!   holding another one: a macro's arguments are unparsed, so `info!("{}",
//!   x)` is flat but `info!("{}", f(x))` is not. Method chains contain a
//!   nested call. `let`, `return` and `f()?` are not flat calls. Callee names
//!   are never consulted. A function signature has no body and no shape.

use mct_core::{LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile};
use mct_tree_sitter::{tokenize_source, TokenClasses};

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_item", "function_signature_item"],
    expanded_literal_kinds: &[],
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
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("raw_string_literal", LiteralCategory::String),
        ("char_literal", LiteralCategory::Character),
        ("integer_literal", LiteralCategory::Number),
        ("float_literal", LiteralCategory::Number),
        ("boolean_literal", LiteralCategory::Boolean),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &[],
    kind_roles: &[
        ("type_identifier", TokenRole::Type),
        ("primitive_type", TokenRole::Type),
        ("field_identifier", TokenRole::Member),
        ("shorthand_field_identifier", TokenRole::Member),
    ],
    primitive_spellings: &[
        ("primitive_type", "bool"),
        ("primitive_type", "char"),
        ("primitive_type", "str"),
        ("primitive_type", "i8"),
        ("primitive_type", "i16"),
        ("primitive_type", "i32"),
        ("primitive_type", "i64"),
        ("primitive_type", "i128"),
        ("primitive_type", "isize"),
        ("primitive_type", "u8"),
        ("primitive_type", "u16"),
        ("primitive_type", "u32"),
        ("primitive_type", "u64"),
        ("primitive_type", "u128"),
        ("primitive_type", "usize"),
        ("primitive_type", "f32"),
        ("primitive_type", "f64"),
    ],
    annotation_sites: &[
        ("parameter", "type"),
        ("function_item", "return_type"),
        ("function_signature_item", "return_type"),
        ("let_declaration", "type"),
    ],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("function_item", "name", TokenRole::Declaration),
        ("function_signature_item", "name", TokenRole::Declaration),
        ("parameter", "pattern", TokenRole::Parameter),
        ("closure_parameters", "", TokenRole::Parameter),
        ("call_expression", "function", TokenRole::Callee),
        ("macro_invocation", "macro", TokenRole::Callee),
    ],
    role_wrappers: &[],
    decorator_kinds: &["attribute_item"],
    context_kinds: &[
        ("mod_item", "mod"),
        ("foreign_mod_item", "extern"),
        ("impl_item", "impl"),
        ("trait_item", "trait"),
        ("function_item", "fn"),
        ("closure_expression", "closure"),
        ("if_expression", "if"),
        ("else_clause", "else"),
        ("match_expression", "match"),
        ("loop_expression", "loop"),
        ("for_expression", "for"),
        ("while_expression", "while"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["call_expression", "macro_invocation"],
    non_flat_kinds: &[
        "closure_expression",
        "if_expression",
        "match_expression",
        "block",
        "loop_expression",
        "while_expression",
        "for_expression",
        "unsafe_block",
        "async_block",
        "const_block",
        "gen_block",
        "try_block",
        "try_expression",
        "await_expression",
    ],
    delimited_argument_kinds: &["token_tree"],
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
