//! [`TokenSource`] for Java, over the `tree-sitter-java` 0.23 node kinds
//! (read off its `node-types.json`):
//!
//! - **Trivia:** `line_comment` and `block_comment`.
//! - **Literals**, one token each: `string_literal` (text blocks too),
//!   `character_literal`, the integer and floating-point literal kinds,
//!   `true`, `false` and `null_literal`.
//! - **Identifiers:** `identifier` and `type_identifier`. Keywords are every
//!   other alphabetic leaf, so `this`, `super` and primitive types (`int`)
//!   are keywords.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span of the symbol the parser
//!   indexes for it: `method_declaration` and `constructor_declaration`
//!   (named by their `name` field, annotations included, since they sit in
//!   the declaration's `modifiers`), and a record component, whose implicit
//!   accessor the parser indexes over the component's `formal_parameter`
//!   (a one-line candidate, never grouped).
//! - **Roles** (syntactic only): method and constructor names are
//!   declarations; the names of a `formal_parameter`, a `spread_parameter`
//!   (`String... args`, through its `variable_declarator`) and a lambda's
//!   parameters are parameters; the `field` of a field access is a member;
//!   the `name` of a method invocation is a callee, with or without a
//!   receiver (`f()` and `a.f()`); a `type_identifier` is a type.
//! - **Literal categories:** strings (interpolated when they hold a
//!   `string_interpolation`) and characters, numbers, booleans, `null`.
//! - **Callable context:** enclosing types, methods, constructors, lambdas
//!   and compound statements are labeled.
//! - **Body shape:** a statement is a flat call when it is an
//!   `expression_statement` holding only a `method_invocation` whose subtree
//!   has no other invocation, object creation, lambda, method reference,
//!   ternary, assignment or `switch` expression.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const INTEGERS: [&str; 4] = [
    "decimal_integer_literal",
    "hex_integer_literal",
    "octal_integer_literal",
    "binary_integer_literal",
];

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["method_declaration", "constructor_declaration"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["line_comment", "block_comment"],
    literal_kinds: &[
        "string_literal",
        "character_literal",
        INTEGERS[0],
        INTEGERS[1],
        INTEGERS[2],
        INTEGERS[3],
        "decimal_floating_point_literal",
        "hex_floating_point_literal",
        "true",
        "false",
        "null_literal",
    ],
    identifier_kinds: &["identifier", "type_identifier"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("character_literal", LiteralCategory::String),
        (INTEGERS[0], LiteralCategory::Number),
        (INTEGERS[1], LiteralCategory::Number),
        (INTEGERS[2], LiteralCategory::Number),
        (INTEGERS[3], LiteralCategory::Number),
        ("decimal_floating_point_literal", LiteralCategory::Number),
        ("hex_floating_point_literal", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("null_literal", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[("string_literal", "string_interpolation")],
    statement_literal_parents: &[],
    kind_roles: &[("type_identifier", TokenRole::Type)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("method_declaration", "name", TokenRole::Declaration),
        ("constructor_declaration", "name", TokenRole::Declaration),
        ("formal_parameter", "name", TokenRole::Parameter),
        ("spread_parameter", "", TokenRole::Parameter),
        ("inferred_parameters", "", TokenRole::Parameter),
        ("lambda_expression", "parameters", TokenRole::Parameter),
        ("field_access", "field", TokenRole::Member),
        ("method_invocation", "name", TokenRole::Callee),
    ],
    role_wrappers: &["variable_declarator"],
    decorator_kinds: &[],
    context_kinds: &[
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("enum_declaration", "enum"),
        ("record_declaration", "record"),
        ("method_declaration", "method"),
        ("constructor_declaration", "constructor"),
        ("lambda_expression", "lambda"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("enhanced_for_statement", "for"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("try_statement", "try"),
        ("try_with_resources_statement", "try"),
        ("catch_clause", "catch"),
        ("finally_clause", "finally"),
        ("switch_expression", "switch"),
        ("switch_block_statement_group", "case"),
        ("switch_rule", "case"),
        ("synchronized_statement", "synchronized"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["method_invocation"],
    non_flat_kinds: &[
        "object_creation_expression",
        "lambda_expression",
        "method_reference",
        "ternary_expression",
        "assignment_expression",
        "switch_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for Java files.
pub struct JavaTokenSource;

impl TokenSource for JavaTokenSource {
    fn language_id(&self) -> &'static str {
        "java"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_java::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            record_component,
        )
    }
}

/// `record P(int x)`: the parser indexes the accessor `x()` over the
/// component's `formal_parameter` (unless the record declares `x()` itself,
/// when this span is never a candidate's).
fn record_component(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    if node.kind() != "formal_parameter"
        || node
            .parent()
            .and_then(|list| list.parent())
            .is_none_or(|owner| owner.kind() != "record_declaration")
    {
        return None;
    }
    let name = node.child_by_field_name("name")?;
    Some(CallableSpan {
        name: source.get(name.byte_range())?.to_string(),
        lines: LineRange {
            start: node.start_position().row as u32 + 1,
            end: node.end_position().row as u32 + 1,
        },
        bytes: node.byte_range(),
        decorators: Vec::new(),
        enclosing: Vec::new(),
        body: None,
    })
}
