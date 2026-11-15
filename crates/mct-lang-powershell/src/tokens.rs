//! [`TokenSource`] for PowerShell, over the `tree-sitter-powershell` 0.26
//! node kinds (read off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `comment` (block comments and comment-based help too).
//! - **Literals**, one token each: `string_literal` (expandable, verbatim
//!   and here-strings alike), `integer_literal` and `real_literal`.
//!   `$true`, `$false` and `$null` are variables.
//! - **Identifiers:** `variable`, `function_name`, `command_name`,
//!   `simple_name` and `type_identifier`. A command's name is an
//!   identifier, as a call's callee is in other languages: the broad
//!   normalization compares the shape of commands, and the consistent one
//!   tells a different command apart. Keywords are every other alphabetic
//!   leaf (`function`, `foreach`, `return`).
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span and name of the symbol the
//!   parser indexes for it: a `function_statement` (`function` and
//!   `filter`), named by its `function_name`, and a
//!   `class_method_definition` (constructors too), named by its
//!   `simple_name`. Neither has a `name` field, so both come from the
//!   callback, and neither has a `body` field, so no body shape is
//!   reported.
//! - **Roles** (syntactic only): a function's or a method's name is a
//!   declaration; a script or method parameter's variable is a parameter;
//!   a member name is a member; a command's name is a callee; a
//!   `type_identifier` is a type.
//! - **Literal categories:** strings and numbers.
//! - **Callable context:** enclosing classes, enums, functions, methods,
//!   script blocks and compound statements are labeled.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{find_child, tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &[],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment"],
    literal_kinds: &["string_literal", "integer_literal", "real_literal"],
    identifier_kinds: &[
        "variable",
        "function_name",
        "command_name",
        "simple_name",
        "type_identifier",
    ],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("integer_literal", LiteralCategory::Number),
        ("real_literal", LiteralCategory::Number),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &[],
    kind_roles: &[
        ("function_name", TokenRole::Declaration),
        ("command_name", TokenRole::Callee),
        ("type_identifier", TokenRole::Type),
    ],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("class_method_definition", "", TokenRole::Declaration),
        ("script_parameter", "", TokenRole::Parameter),
        ("class_method_parameter", "", TokenRole::Parameter),
        ("member_name", "", TokenRole::Member),
    ],
    role_wrappers: &[],
    decorator_kinds: &[],
    context_kinds: &[
        ("class_statement", "class"),
        ("enum_statement", "enum"),
        ("function_statement", "function"),
        ("class_method_definition", "method"),
        ("script_block_expression", "scriptblock"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("foreach_statement", "foreach"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("switch_statement", "switch"),
        ("try_statement", "try"),
        ("catch_clause", "catch"),
        ("finally_clause", "finally"),
        ("trap_statement", "trap"),
    ],
    expression_statement_kinds: &[],
    call_kinds: &[],
    non_flat_kinds: &[],
    delimited_argument_kinds: &[],
};

/// Token source for PowerShell files.
pub struct PowerShellTokenSource;

impl TokenSource for PowerShellTokenSource {
    fn language_id(&self) -> &'static str {
        "powershell"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_powershell::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            callable,
        )
    }
}

/// Functions and class methods, named as the parser names them.
fn callable(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    let name = match node.kind() {
        "function_statement" => find_child(node, "function_name")?,
        "class_method_definition" => find_child(node, "simple_name")?,
        _ => return None,
    };
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
