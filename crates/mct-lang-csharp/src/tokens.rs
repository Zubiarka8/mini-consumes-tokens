//! [`TokenSource`] for C#, over the `tree-sitter-c-sharp` 0.23 node kinds
//! (read off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `comment` (doc comments too) and `shebang_directive`.
//! - **Literals**, one token each: `string_literal`,
//!   `verbatim_string_literal`, `raw_string_literal`,
//!   `interpolated_string_expression`, `character_literal`,
//!   `integer_literal`, `real_literal`, `boolean_literal` and `null_literal`.
//! - **Identifiers:** `identifier`, a lambda's `implicit_parameter` (`x` in
//!   `x => …`) and `discard` (`_`). Keywords are every other alphabetic
//!   leaf, so `var` and predefined types (`int`) are keywords.
//! - **Blocks:** none. Braces are tokens of their own.
//! - **Callables**, each with the exact span and name of the symbol the
//!   parser indexes for it: `method_declaration`, `constructor_declaration`
//!   and `local_function_statement` (named by their `name` field,
//!   attributes included, since they sit in the declaration); a destructor
//!   (`~Cart`), an operator (`operator +`) and a conversion operator
//!   (`operator int`), named as the parser names them; and a local bound
//!   to a lambda or an anonymous method (`Func<int> f = () => …;`), whose
//!   `Function` symbol spans its `variable_declarator`.
//! - **Roles** (syntactic only): callable names are declarations; the names
//!   of a `parameter` and an `implicit_parameter` are parameters; the
//!   `name` of a member access is a member; the bare `function` of an
//!   invocation is a callee; a type written in a declaration's `type` or
//!   `returns` field, or inside a generic, qualified, array or nullable
//!   type, is a type.
//! - **Literal categories:** strings (interpolated for `$"…{x}…"`) and
//!   characters, numbers, booleans, `null`.
//! - **Callable context:** enclosing types, namespaces, methods, local
//!   functions, lambdas and compound statements are labeled.
//! - **Body shape:** a statement is a flat call when it is an
//!   `expression_statement` holding only an `invocation_expression` whose
//!   subtree has no other invocation, object creation, lambda, anonymous
//!   method, conditional, assignment, `switch` or `await` expression.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_source_with_callables, TokenClasses};
use tree_sitter::Node;

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &[
        "method_declaration",
        "constructor_declaration",
        "local_function_statement",
    ],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "shebang_directive"],
    literal_kinds: &[
        "string_literal",
        "verbatim_string_literal",
        "raw_string_literal",
        "interpolated_string_expression",
        "character_literal",
        "integer_literal",
        "real_literal",
        "boolean_literal",
        "null_literal",
    ],
    identifier_kinds: &["identifier", "implicit_parameter", "discard"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string_literal", LiteralCategory::String),
        ("verbatim_string_literal", LiteralCategory::String),
        ("raw_string_literal", LiteralCategory::String),
        ("interpolated_string_expression", LiteralCategory::String),
        ("character_literal", LiteralCategory::String),
        ("integer_literal", LiteralCategory::Number),
        ("real_literal", LiteralCategory::Number),
        ("boolean_literal", LiteralCategory::Boolean),
        ("null_literal", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[("interpolated_string_expression", "interpolation")],
    statement_literal_parents: &[],
    kind_roles: &[("implicit_parameter", TokenRole::Parameter)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[
        "generic_name",
        "qualified_name",
        "array_type",
        "nullable_type",
    ],
    parent_roles: &[
        ("method_declaration", "name", TokenRole::Declaration),
        ("constructor_declaration", "name", TokenRole::Declaration),
        ("destructor_declaration", "name", TokenRole::Declaration),
        ("local_function_statement", "name", TokenRole::Declaration),
        ("parameter", "name", TokenRole::Parameter),
        ("member_access_expression", "name", TokenRole::Member),
        ("invocation_expression", "function", TokenRole::Callee),
        ("method_declaration", "returns", TokenRole::Type),
        ("local_function_statement", "type", TokenRole::Type),
        ("operator_declaration", "type", TokenRole::Type),
        ("conversion_operator_declaration", "type", TokenRole::Type),
        ("parameter", "type", TokenRole::Type),
        ("variable_declaration", "type", TokenRole::Type),
        ("property_declaration", "type", TokenRole::Type),
        ("object_creation_expression", "type", TokenRole::Type),
    ],
    role_wrappers: &[],
    decorator_kinds: &[],
    context_kinds: &[
        ("namespace_declaration", "namespace"),
        ("class_declaration", "class"),
        ("struct_declaration", "struct"),
        ("interface_declaration", "interface"),
        ("record_declaration", "record"),
        ("enum_declaration", "enum"),
        ("method_declaration", "method"),
        ("constructor_declaration", "constructor"),
        ("local_function_statement", "function"),
        ("lambda_expression", "lambda"),
        ("anonymous_method_expression", "lambda"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("foreach_statement", "foreach"),
        ("while_statement", "while"),
        ("do_statement", "do"),
        ("try_statement", "try"),
        ("catch_clause", "catch"),
        ("finally_clause", "finally"),
        ("switch_statement", "switch"),
        ("switch_section", "case"),
        ("using_statement", "using"),
        ("lock_statement", "lock"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["invocation_expression"],
    non_flat_kinds: &[
        "object_creation_expression",
        "implicit_object_creation_expression",
        "lambda_expression",
        "anonymous_method_expression",
        "conditional_expression",
        "assignment_expression",
        "switch_expression",
        "await_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for C# files.
pub struct CSharpTokenSource;

impl TokenSource for CSharpTokenSource {
    fn language_id(&self) -> &'static str {
        "csharp"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_c_sharp::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            parser_named,
        )
    }
}

/// Callables the parser names itself, or whose symbol is not the node a
/// `name` field sits on: `~Cart`, `operator +`, `operator int`, and a local
/// bound to a lambda (its `variable_declarator`).
fn parser_named(node: Node<'_>, source: &str) -> Option<CallableSpan> {
    let field = |name: &str| {
        node.child_by_field_name(name)
            .and_then(|n| source.get(n.byte_range()))
    };
    let name = match node.kind() {
        "destructor_declaration" => format!("~{}", field("name")?),
        "operator_declaration" => format!("operator {}", field("operator")?),
        "conversion_operator_declaration" => {
            let target = node.child_by_field_name("type")?;
            let base = crate::type_base_name(target, source)
                .or_else(|| source.get(target.byte_range()).map(str::to_string))?;
            format!("operator {base}")
        }
        "variable_declarator" if is_local_lambda(node) => field("name")?.to_string(),
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

/// `Func<int> f = () => …;` in a `local_declaration_statement`, the only
/// place the parser indexes a lambda binding.
fn is_local_lambda(declarator: Node<'_>) -> bool {
    crate::bound_lambda(declarator).is_some()
        && declarator
            .parent()
            .and_then(|declaration| declaration.parent())
            .is_some_and(|statement| statement.kind() == "local_declaration_statement")
}
