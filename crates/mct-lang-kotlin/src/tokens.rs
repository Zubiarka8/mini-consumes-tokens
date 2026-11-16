//! [`TokenSource`] for Kotlin, over the `tree-sitter-kotlin-ng` 1.1 node
//! kinds (read off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `line_comment`, `block_comment` and `shebang`.
//! - **Literals**, one token each: `string_literal`,
//!   `multiline_string_literal`, `character_literal`, `number_literal` and
//!   `float_literal`. The grammar parses `true`, `false` and `null` as plain
//!   `identifier`s, so they are identifier tokens here.
//! - **Identifiers:** `identifier`. Keywords are every other alphabetic leaf.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span of the symbol the parser
//!   indexes for it: every `function_declaration` (member, top-level, local,
//!   extension or interface, annotations included, since they sit in its
//!   `modifiers`), and `val f = { … }` / `val f = fun(…) { … }` outside a
//!   class body, whose `Function` symbol spans the whole `property_declaration`.
//!   A bound lambda in a class body is a field, so it has no span.
//! - **Roles** (syntactic only): function names are declarations; the names
//!   of a `parameter`, a `class_parameter` and a lambda's parameters are
//!   parameters; an identifier inside a `user_type` is a type; the bare
//!   callee of a call (`f()`) is a callee. A `navigation_expression` has no
//!   field names, so neither its member (`a.f`) nor a qualified callee
//!   (`a.f()`) gets a role.
//! - **Literal categories:** strings (interpolated when they hold an
//!   `${…}` `interpolation`; the grammar keeps `$x` as plain string content)
//!   and characters, numbers.
//! - **Callable context:** enclosing classes, objects, functions, lambdas,
//!   constructors, `init` blocks and compound statements are labeled.
//! - **Body shape:** not reported. A `function_body` holds one `block` (or
//!   one expression), so the shared reader, which counts a body's direct
//!   children, would see one statement in every function.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_declaration"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["line_comment", "block_comment", "shebang"],
    literal_kinds: &[
        "string_literal",
        "multiline_string_literal",
        "character_literal",
        "number_literal",
        "float_literal",
    ],
    identifier_kinds: &["identifier"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("multiline_string_literal", LiteralCategory::String),
        ("character_literal", LiteralCategory::String),
        ("number_literal", LiteralCategory::Number),
        ("float_literal", LiteralCategory::Number),
    ],
    interpolated_literal_kinds: &[
        ("string_literal", "interpolation"),
        ("multiline_string_literal", "interpolation"),
    ],
    statement_literal_parents: &[],
    kind_roles: &[],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &["user_type"],
    parent_roles: &[
        ("function_declaration", "name", TokenRole::Declaration),
        ("parameter", "", TokenRole::Parameter),
        ("class_parameter", "", TokenRole::Parameter),
        ("lambda_parameters", "", TokenRole::Parameter),
        ("call_expression", "", TokenRole::Callee),
    ],
    role_wrappers: &["variable_declaration"],
    decorator_kinds: &[],
    context_kinds: &[
        ("class_declaration", "class"),
        ("object_declaration", "object"),
        ("object_literal", "object"),
        ("companion_object", "companion"),
        ("function_declaration", "fun"),
        ("anonymous_function", "fun"),
        ("lambda_literal", "lambda"),
        ("secondary_constructor", "constructor"),
        ("anonymous_initializer", "init"),
        ("if_expression", "if"),
        ("when_expression", "when"),
        ("when_entry", "case"),
        ("for_statement", "for"),
        ("while_statement", "while"),
        ("do_while_statement", "do"),
        ("try_expression", "try"),
        ("catch_block", "catch"),
        ("finally_block", "finally"),
    ],
    expression_statement_kinds: &[],
    call_kinds: &[],
    non_flat_kinds: &[],
    delimited_argument_kinds: &[],
};

/// Token source for Kotlin files.
pub struct KotlinTokenSource;

impl TokenSource for KotlinTokenSource {
    fn language_id(&self) -> &'static str {
        "kotlin"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_kotlin_ng::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            named_lambda,
        )
    }
}

/// `val f = { … }` outside a class body: the parser's `Function` spans the
/// whole `property_declaration`. Inside a class (its primary constructor,
/// body, `init` blocks, accessors and secondary constructors) the same
/// binding is a field, unless a `fun` in between makes it a local.
fn named_lambda(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    if node.kind() != "property_declaration" || in_class_scope(node) {
        return None;
    }
    let (name, _) = crate::named_lambda(node)?;
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

/// Whether the nearest enclosing `fun` or type is a type: the parser visits
/// a type's members with the type as their parent, a function's body with
/// none.
fn in_class_scope(node: Node<'_>) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_declaration" => return false,
            "class_declaration" | "object_declaration" => return true,
            _ => current = ancestor.parent(),
        }
    }
    false
}
