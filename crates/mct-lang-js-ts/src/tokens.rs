//! [`TokenSource`] for JavaScript and TypeScript, over the node kinds that
//! `tree-sitter-javascript` 0.25 and `tree-sitter-typescript` 0.23 share.
//! The grammar is chosen per file extension exactly as the parser chooses it
//! ([`crate::grammar_for`]).
//!
//! The kind tables were read off the grammars' parse trees, not guessed:
//!
//! - **Trivia:** `comment` and `hash_bang_line`.
//! - **Literals**, one token each: `string`, `template_string` (whatever its
//!   `string_fragment`/`escape_sequence`/`template_substitution` children),
//!   `number`, `regex`, `jsx_text`, and the constants `true`, `false`,
//!   `null` and `undefined`. Structural mode expands a template string that
//!   has a substitution, so the expressions inside it are tokens.
//! - **Identifiers:** `identifier`, `property_identifier`,
//!   `private_property_identifier` (`#x`), `shorthand_property_identifier`,
//!   `shorthand_property_identifier_pattern`, `statement_identifier` (a
//!   label) and `type_identifier`. Keywords are every other alphabetic leaf,
//!   so `this`, `super` and TypeScript's predefined types (`number`) are
//!   keywords.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span of the symbol the parser
//!   indexes for it, so the engine can match the two:
//!   `function_declaration` and `generator_function_declaration`, a class or
//!   object `method_definition`, and the bodyless TypeScript
//!   `method_signature` (interface or object type member) and
//!   `abstract_method_signature`, each named by its `name` field; a
//!   `variable_declarator` whose name is an identifier and whose value is an
//!   arrow function or function expression (`const f = () => {}`, spanning
//!   the declarator); a class field with such a value (`f = () => {}`,
//!   spanning the field); a CommonJS member export (`exports.f = function
//!   () {}`, spanning the function only); and an object `pair` with such a
//!   value (`{ f: () => {} }`, spanning the pair). The last two are spans for
//!   every such assignment or pair: one the parser records no symbol for is
//!   never a candidate, so it is never read.
//! - **Roles** (syntactic only): the names of function, class and method
//!   declarations are declarations; identifiers in `formal_parameters`
//!   (a JavaScript `...rest` through its `rest_pattern`), the `pattern` of a
//!   TypeScript parameter and the single `parameter` of `x => …` are
//!   parameters (a TypeScript `...rest` stays unclassified: its wrapper's
//!   parent also holds the default value, which is no parameter); the `property` of a
//!   member access and the `key` of a pair are members; the bare `function`
//!   of a call and the `constructor` of `new` are callees; a
//!   `type_identifier`, and any name inside a type annotation or type
//!   arguments, is a type.
//! - **Literal categories:** strings and templates (interpolated when they
//!   hold a substitution), numbers, booleans, `null`/`undefined`; a string
//!   that is a whole statement (`"use strict"`) is a docstring-like
//!   statement literal.
//! - **Callable context:** enclosing classes, functions and compound
//!   statements (`if`/`else`, `for`, `while`, `do`, `try`/`catch`/`finally`,
//!   `switch`/`case`) and TypeScript namespaces are labeled.
//! - **Body shape:** for the node-named callables, a statement is a flat
//!   call when it is an `expression_statement` holding only a
//!   `call_expression` whose subtree has no other call, `new`, function,
//!   `await`, ternary or assignment. Bound callables report no body shape.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const INTERPOLATED: &[(&str, &str)] = &[("template_string", "template_substitution")];

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &[
        "function_declaration",
        "generator_function_declaration",
        "method_definition",
        "method_signature",
        "abstract_method_signature",
    ],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "hash_bang_line"],
    literal_kinds: &[
        "string",
        "template_string",
        "number",
        "regex",
        "jsx_text",
        "true",
        "false",
        "null",
        "undefined",
    ],
    identifier_kinds: &[
        "identifier",
        "property_identifier",
        "private_property_identifier",
        "shorthand_property_identifier",
        "shorthand_property_identifier_pattern",
        "statement_identifier",
        "type_identifier",
    ],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("template_string", LiteralCategory::String),
        ("string_fragment", LiteralCategory::String),
        ("escape_sequence", LiteralCategory::String),
        ("jsx_text", LiteralCategory::String),
        ("number", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("null", LiteralCategory::Null),
        ("undefined", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: INTERPOLATED,
    statement_literal_parents: &["expression_statement"],
    kind_roles: &[("type_identifier", TokenRole::Type)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &["type_annotation", "type_arguments"],
    parent_roles: &[
        ("function_declaration", "name", TokenRole::Declaration),
        (
            "generator_function_declaration",
            "name",
            TokenRole::Declaration,
        ),
        ("class_declaration", "name", TokenRole::Declaration),
        ("method_definition", "name", TokenRole::Declaration),
        ("formal_parameters", "", TokenRole::Parameter),
        ("required_parameter", "pattern", TokenRole::Parameter),
        ("optional_parameter", "pattern", TokenRole::Parameter),
        ("arrow_function", "parameter", TokenRole::Parameter),
        ("member_expression", "property", TokenRole::Member),
        ("pair", "key", TokenRole::Member),
        ("call_expression", "function", TokenRole::Callee),
        ("new_expression", "constructor", TokenRole::Callee),
    ],
    role_wrappers: &["rest_pattern"],
    decorator_kinds: &[],
    context_kinds: &[
        ("class_declaration", "class"),
        ("abstract_class_declaration", "class"),
        ("class", "class"),
        ("internal_module", "namespace"),
        ("function_declaration", "function"),
        ("generator_function_declaration", "function"),
        ("method_definition", "function"),
        ("function_expression", "function"),
        ("arrow_function", "function"),
        ("if_statement", "if"),
        ("else_clause", "else"),
        ("for_statement", "for"),
        ("for_in_statement", "for"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("try_statement", "try"),
        ("catch_clause", "catch"),
        ("finally_clause", "finally"),
        ("switch_statement", "switch"),
        ("switch_case", "case"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["call_expression"],
    non_flat_kinds: &[
        "new_expression",
        "arrow_function",
        "function_expression",
        "await_expression",
        "ternary_expression",
        "assignment_expression",
        "augmented_assignment_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for JavaScript and TypeScript files.
pub struct JsTsTokenSource;

impl TokenSource for JsTsTokenSource {
    fn language_id(&self) -> &'static str {
        "javascript_typescript"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &crate::grammar_for(relative_path),
            &CLASSES,
            relative_path,
            source,
            bound_function,
        )
    }

    fn tokenize_structural(
        &self,
        relative_path: &str,
        source: &str,
    ) -> Result<TokenizedFile, ParseError> {
        let classes = TokenClasses {
            expanded_literal_kinds: INTERPOLATED,
            literal_kinds: &[
                "string",
                "template_string",
                "string_fragment",
                "escape_sequence",
                "number",
                "regex",
                "jsx_text",
                "true",
                "false",
                "null",
                "undefined",
            ],
            ..CLASSES
        };
        tokenize_source_with_callables(
            &crate::grammar_for(relative_path),
            &classes,
            relative_path,
            source,
            bound_function,
        )
    }
}

fn is_function_value(node: Node) -> bool {
    matches!(node.kind(), "arrow_function" | "function_expression")
}

/// A function bound to a name, with the span the parser's symbol has (see
/// the module documentation).
fn bound_function(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    let (name, span) = match node.kind() {
        "variable_declarator" => {
            node.child_by_field_name("value")
                .filter(|v| is_function_value(*v))?;
            let name = node
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")?;
            (name, node)
        }
        "field_definition" | "public_field_definition" => {
            node.child_by_field_name("value")
                .filter(|v| is_function_value(*v))?;
            let name = node
                .child_by_field_name("property")
                .or_else(|| node.child_by_field_name("name"))?;
            (name, node)
        }
        "pair" => {
            node.child_by_field_name("value")
                .filter(|v| is_function_value(*v))?;
            (node.child_by_field_name("key")?, node)
        }
        "arrow_function" | "function_expression" => {
            let assignment = node
                .parent()
                .filter(|p| p.kind() == "assignment_expression")?;
            if assignment.child_by_field_name("right").map(|r| r.id()) != Some(node.id()) {
                return None;
            }
            let target = assignment
                .child_by_field_name("left")
                .filter(|l| l.kind() == "member_expression")?;
            (target.child_by_field_name("property")?, node)
        }
        _ => return None,
    };
    Some(CallableSpan {
        name: source.get(name.byte_range())?.to_string(),
        lines: LineRange {
            start: span.start_position().row as u32 + 1,
            end: span.end_position().row as u32 + 1,
        },
        bytes: span.byte_range(),
        decorators: Vec::new(),
        enclosing: Vec::new(),
        body: None,
    })
}
