//! [`TokenSource`] for Go, over the `tree-sitter-go` 0.25 node kinds (read
//! off its `node-types.json`):
//!
//! - **Trivia:** `comment`.
//! - **Literals**, one token each: `interpreted_string_literal`,
//!   `raw_string_literal`, `rune_literal`, `int_literal`, `float_literal`,
//!   `imaginary_literal`, and the constants `true`, `false`, `nil` and
//!   `iota`.
//! - **Identifiers:** `identifier`, `field_identifier`, `type_identifier`,
//!   `package_identifier`, `label_name` and `blank_identifier` (`_`).
//!   Keywords are every other alphabetic leaf.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span of the symbol the parser
//!   indexes for it: `function_declaration` and `method_declaration` (named
//!   by their `name` field), an interface's `method_elem`, and
//!   `var f = func(…) {…}`, whose symbol, and so whose span, is the name
//!   alone (a one-token candidate, never grouped).
//! - **Roles** (syntactic only): function and method names are
//!   declarations; the names of a `parameter_declaration` or
//!   `variadic_parameter_declaration` (receivers included) are parameters;
//!   the `field` of a selector is a member; the bare `function` of a call is
//!   a callee; a `type_identifier` is a type.
//! - **Literal categories:** strings and runes, numbers, booleans, `nil`.
//! - **Callable context:** enclosing functions, function literals and
//!   compound statements (`if`, `for`, `switch`, `select` and their cases)
//!   are labeled.
//! - **Body shape:** not reported. A Go `block` holds one `statement_list`,
//!   so the shared reader, which counts a body's direct children, would see
//!   one statement in every function.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_declaration", "method_declaration", "method_elem"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment"],
    literal_kinds: &[
        "interpreted_string_literal",
        "raw_string_literal",
        "rune_literal",
        "int_literal",
        "float_literal",
        "imaginary_literal",
        "true",
        "false",
        "nil",
        "iota",
    ],
    identifier_kinds: &[
        "identifier",
        "field_identifier",
        "type_identifier",
        "package_identifier",
        "label_name",
        "blank_identifier",
    ],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("interpreted_string_literal", LiteralCategory::String),
        ("raw_string_literal", LiteralCategory::String),
        ("rune_literal", LiteralCategory::String),
        ("int_literal", LiteralCategory::Number),
        ("float_literal", LiteralCategory::Number),
        ("imaginary_literal", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("nil", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &[],
    kind_roles: &[("type_identifier", TokenRole::Type)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("function_declaration", "name", TokenRole::Declaration),
        ("method_declaration", "name", TokenRole::Declaration),
        ("method_elem", "name", TokenRole::Declaration),
        ("parameter_declaration", "name", TokenRole::Parameter),
        (
            "variadic_parameter_declaration",
            "name",
            TokenRole::Parameter,
        ),
        ("selector_expression", "field", TokenRole::Member),
        ("call_expression", "function", TokenRole::Callee),
    ],
    role_wrappers: &[],
    decorator_kinds: &[],
    context_kinds: &[
        ("function_declaration", "func"),
        ("method_declaration", "func"),
        ("func_literal", "func"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("expression_switch_statement", "switch"),
        ("type_switch_statement", "switch"),
        ("select_statement", "select"),
        ("expression_case", "case"),
        ("type_case", "case"),
        ("communication_case", "case"),
        ("default_case", "default"),
    ],
    expression_statement_kinds: &[],
    call_kinds: &[],
    non_flat_kinds: &[],
    delimited_argument_kinds: &[],
};

/// Token source for Go files.
pub struct GoTokenSource;

impl TokenSource for GoTokenSource {
    fn language_id(&self) -> &'static str {
        "go"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_go::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            func_value_name,
        )
    }
}

/// `var f = func(…) {…}`: the parser's `Function` symbol spans the name.
fn func_value_name(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    crate::func_value(node)?;
    let name = node.child_by_field_name("name")?;
    Some(CallableSpan {
        name: source.get(name.byte_range())?.to_string(),
        lines: LineRange {
            start: name.start_position().row as u32 + 1,
            end: name.end_position().row as u32 + 1,
        },
        bytes: name.byte_range(),
        decorators: Vec::new(),
        enclosing: Vec::new(),
        body: None,
    })
}
