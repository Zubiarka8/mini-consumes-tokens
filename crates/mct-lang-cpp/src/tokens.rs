//! [`TokenSource`] for C++, over the `tree-sitter-cpp` 0.23 node kinds (read
//! off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `comment`.
//! - **Literals**, one token each: `string_literal`, `raw_string_literal`,
//!   `char_literal`, `number_literal`, `user_defined_literal`, `true`,
//!   `false` and `null` (`nullptr`, `NULL`).
//! - **Identifiers:** `identifier`, `field_identifier`, `type_identifier`,
//!   `namespace_identifier` and `statement_identifier`. Keywords are every
//!   other alphabetic leaf, so primitive types (`int`) and `auto` are
//!   keywords.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span and name of the symbol the
//!   parser indexes for it: a `function_definition` (named like the parser:
//!   the last segment of `Cart::total`, or `operator==`, `~Cart`); a
//!   prototype (`int total() const;`) outside a function body, over the
//!   whole declaration; `auto f = [](…) { … };`, whose `Function` symbol
//!   spans its `init_declarator`; and a function-like macro
//!   (`preproc_function_def`, named by its `name` field). A declaration
//!   with two function declarators (`int f(), g();`) gets a span for the
//!   first only, so the second is skipped.
//! - **Roles** (syntactic only): the name in a `function_declarator` is a
//!   declaration; the name in a `parameter_declaration` (through `&`, `*`)
//!   is a parameter; the `field` of a field expression is a member; the
//!   bare `function` of a call is a callee; a `type_identifier` is a type.
//! - **Literal categories:** strings and characters, numbers, booleans,
//!   `null`.
//! - **Callable context:** enclosing namespaces, classes, structs, unions,
//!   functions, lambdas and compound statements are labeled.
//! - **Body shape:** a statement is a flat call when it is an
//!   `expression_statement` holding only a `call_expression` whose subtree
//!   has no other call, `new`, lambda, conditional or assignment.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["preproc_function_def"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment"],
    literal_kinds: &[
        "string_literal",
        "raw_string_literal",
        "char_literal",
        "number_literal",
        "user_defined_literal",
        "true",
        "false",
        "null",
    ],
    identifier_kinds: &[
        "identifier",
        "field_identifier",
        "type_identifier",
        "namespace_identifier",
        "statement_identifier",
    ],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("raw_string_literal", LiteralCategory::String),
        ("char_literal", LiteralCategory::String),
        ("number_literal", LiteralCategory::Number),
        ("user_defined_literal", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("null", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &[],
    kind_roles: &[("type_identifier", TokenRole::Type)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("function_declarator", "declarator", TokenRole::Declaration),
        ("preproc_function_def", "name", TokenRole::Declaration),
        ("parameter_declaration", "", TokenRole::Parameter),
        ("field_expression", "field", TokenRole::Member),
        ("call_expression", "function", TokenRole::Callee),
    ],
    role_wrappers: &["reference_declarator", "pointer_declarator"],
    decorator_kinds: &[],
    context_kinds: &[
        ("namespace_definition", "namespace"),
        ("class_specifier", "class"),
        ("struct_specifier", "struct"),
        ("union_specifier", "union"),
        ("function_definition", "function"),
        ("lambda_expression", "lambda"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("for_range_loop", "for"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("switch_statement", "switch"),
        ("case_statement", "case"),
        ("try_statement", "try"),
        ("catch_clause", "catch"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["call_expression"],
    non_flat_kinds: &[
        "new_expression",
        "lambda_expression",
        "conditional_expression",
        "assignment_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for C++ files.
pub struct CppTokenSource;

impl TokenSource for CppTokenSource {
    fn language_id(&self) -> &'static str {
        "cpp"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_cpp::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            callable,
        )
    }
}

/// Definitions, prototypes and named lambdas, named as the parser names
/// them (see the module docs).
fn callable(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    let name = match node.kind() {
        "function_definition" => node
            .child_by_field_name("declarator")
            .and_then(crate::find_function_declarator)
            .map(|declarator| function_name(declarator, source))?,
        "declaration" | "field_declaration" if !in_function_body(node) => {
            let mut cursor = node.walk();
            let declarators: Vec<Node<'_>> = node
                .children_by_field_name("declarator", &mut cursor)
                .collect();
            let named_lambda =
                |d: &Node<'_>| node.kind() == "declaration" && crate::named_lambda(*d).is_some();
            declarators
                .iter()
                .filter(|d| !named_lambda(d))
                .find_map(|d| crate::find_function_declarator(*d))
                .map(|declarator| function_name(declarator, source))?
        }
        "init_declarator" if node.parent().is_some_and(|p| p.kind() == "declaration") => {
            let (name, _) = crate::named_lambda(node)?;
            source.get(name.byte_range())?.to_string()
        }
        _ => return None,
    };
    Some(CallableSpan {
        name,
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

/// The parser's name for a function: the last segment of a qualified name
/// (`total` in `shop::Cart::total`), else the declarator's whole text
/// (`operator==`, `~Cart`); empty when there is none.
fn function_name(declarator: Node<'_>, source: &str) -> String {
    let Some(name) = declarator.child_by_field_name("declarator") else {
        return String::new();
    };
    if name.kind() == "qualified_identifier" {
        return crate::split_qualified(name, source, None).0;
    }
    source
        .get(name.byte_range())
        .unwrap_or_default()
        .to_string()
}

/// Whether the nearest scope is a function's body (where the parser indexes
/// no prototype): a definition or a named lambda before any class or
/// namespace.
fn in_function_body(node: Node<'_>) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_definition" => return true,
            "lambda_expression" if is_named_lambda(ancestor) => return true,
            "class_specifier" | "struct_specifier" | "union_specifier" | "namespace_definition" => {
                return false
            }
            _ => current = ancestor.parent(),
        }
    }
    false
}

/// `f = [](…) { … }` or `f{[] { … }}` in a `declaration`: the lambda the
/// parser walks as the body of the `Function` `f`.
fn is_named_lambda(lambda: Node<'_>) -> bool {
    let mut parent = lambda.parent();
    if parent.is_some_and(|p| p.kind() == "initializer_list") {
        parent = parent.and_then(|p| p.parent());
    }
    parent.is_some_and(|declarator| {
        crate::named_lambda(declarator).is_some_and(|(_, found)| found.id() == lambda.id())
            && declarator
                .parent()
                .is_some_and(|declaration| declaration.kind() == "declaration")
    })
}
