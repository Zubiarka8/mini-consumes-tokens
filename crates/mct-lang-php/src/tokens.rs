//! [`TokenSource`] for PHP, over the `tree-sitter-php` 0.24 node kinds (read
//! off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `comment`, the opening `<?php` tag and inline HTML (a
//!   `text_interpolation`: `?>`, the text, up to the next tag).
//! - **Literals**, one token each: `string`, `encapsed_string`, `heredoc`,
//!   `nowdoc`, `integer`, `float`, `boolean` and `null`.
//! - **Identifiers:** `name`, so a variable is `$` and its name. Keywords
//!   are every other alphabetic leaf, so primitive types (`int`) are
//!   keywords.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span and name of the symbol the
//!   parser indexes for it: `function_definition` and `method_declaration`
//!   (named by their `name` field, attributes included, since they sit in
//!   the declaration), and a closure or arrow function assigned to a
//!   variable (`$cb = function () { … };`), whose `Function` symbol is
//!   named after the variable and spans the closure.
//! - **Roles** (syntactic only): callable names are declarations; a
//!   parameter's variable is a parameter; the `name` of a member access or
//!   of a member, null-safe or static call is a member; the bare
//!   `function` of a call is a callee; a `named_type` is a type.
//! - **Literal categories:** strings (interpolated for `"…{$x}…"` and a
//!   heredoc with a variable), numbers, booleans, `null`.
//! - **Callable context:** enclosing namespaces, classes, interfaces,
//!   traits, enums, functions, methods, closures and compound statements
//!   are labeled.
//! - **Body shape:** a statement is a flat call when it is an
//!   `expression_statement` holding only a call whose subtree has no other
//!   call, `new`, closure, conditional, assignment or `match`.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_definition", "method_declaration"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "php_tag", "text_interpolation"],
    literal_kinds: &[
        "string",
        "encapsed_string",
        "heredoc",
        "nowdoc",
        "integer",
        "float",
        "boolean",
        "null",
    ],
    identifier_kinds: &["name"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("encapsed_string", LiteralCategory::String),
        ("heredoc", LiteralCategory::String),
        ("nowdoc", LiteralCategory::String),
        ("integer", LiteralCategory::Number),
        ("float", LiteralCategory::Number),
        ("boolean", LiteralCategory::Boolean),
        ("null", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[
        ("encapsed_string", "variable_name"),
        ("encapsed_string", "member_access_expression"),
        ("encapsed_string", "subscript_expression"),
        ("heredoc_body", "variable_name"),
    ],
    statement_literal_parents: &[],
    kind_roles: &[],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &["named_type"],
    parent_roles: &[
        ("function_definition", "name", TokenRole::Declaration),
        ("method_declaration", "name", TokenRole::Declaration),
        ("simple_parameter", "", TokenRole::Parameter),
        ("property_promotion_parameter", "", TokenRole::Parameter),
        ("variadic_parameter", "", TokenRole::Parameter),
        ("member_access_expression", "name", TokenRole::Member),
        (
            "nullsafe_member_access_expression",
            "name",
            TokenRole::Member,
        ),
        ("member_call_expression", "name", TokenRole::Member),
        ("nullsafe_member_call_expression", "name", TokenRole::Member),
        ("scoped_call_expression", "name", TokenRole::Member),
        ("function_call_expression", "function", TokenRole::Callee),
    ],
    role_wrappers: &["variable_name"],
    decorator_kinds: &[],
    context_kinds: &[
        ("namespace_definition", "namespace"),
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("trait_declaration", "trait"),
        ("enum_declaration", "enum"),
        ("function_definition", "function"),
        ("method_declaration", "method"),
        ("anonymous_function", "closure"),
        ("arrow_function", "closure"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("foreach_statement", "foreach"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("switch_statement", "switch"),
        ("try_statement", "try"),
        ("catch_clause", "catch"),
        ("finally_clause", "finally"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &[
        "function_call_expression",
        "member_call_expression",
        "nullsafe_member_call_expression",
        "scoped_call_expression",
    ],
    non_flat_kinds: &[
        "object_creation_expression",
        "anonymous_function",
        "arrow_function",
        "conditional_expression",
        "assignment_expression",
        "augmented_assignment_expression",
        "match_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for PHP files.
pub struct PhpTokenSource;

impl TokenSource for PhpTokenSource {
    fn language_id(&self) -> &'static str {
        "php"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_php::LANGUAGE_PHP.into(),
            &CLASSES,
            relative_path,
            source,
            bound_closure,
        )
    }
}

/// `$cb = function () { … }` or `$cb = fn() => …`: the parser's `Function`
/// `cb`, which spans the closure.
fn bound_closure(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    if !matches!(node.kind(), "anonymous_function" | "arrow_function") {
        return None;
    }
    let assignment = node
        .parent()
        .filter(|p| p.kind() == "assignment_expression")?;
    if assignment.child_by_field_name("right")?.id() != node.id() {
        return None;
    }
    let name = crate::variable_text(assignment.child_by_field_name("left")?, source)?;
    Some(CallableSpan {
        name: name.to_string(),
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
