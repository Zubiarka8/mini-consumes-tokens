use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{module_location, tokenize_source_with_callables, TokenClasses};

/// Token stream of a `.gd` file for duplicate detection. Functions, methods,
/// `_init` and lambdas assigned to a name are its callable spans.
pub struct GdscriptTokenSource;

impl TokenSource for GdscriptTokenSource {
    fn language_id(&self) -> &'static str {
        "gdscript"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source_with_callables(
            &tree_sitter_gdscript::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
            special_callable,
        )
    }
}

fn special_callable(node: tree_sitter::Node<'_>, source: &str) -> Option<CallableSpan> {
    let name = match node.kind() {
        "constructor_definition" => "_init",
        "variable_statement"
        | "const_statement"
        | "export_variable_statement"
        | "onready_variable_statement" => {
            node.child_by_field_name("value")
                .filter(|value| value.kind() == "lambda")?;
            node.child_by_field_name("name")?
                .utf8_text(source.as_bytes())
                .ok()?
        }
        "assignment" => {
            node.child_by_field_name("right")
                .filter(|value| value.kind() == "lambda")?;
            node.child_by_field_name("left")
                .filter(|left| left.kind() == "identifier")?
                .utf8_text(source.as_bytes())
                .ok()?
        }
        "lambda" => {
            if node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "const_statement"
                        | "variable_statement"
                        | "export_variable_statement"
                        | "onready_variable_statement"
                ) && parent.child_by_field_name("value") == Some(node)
                    || parent.kind() == "assignment"
                        && parent.child_by_field_name("right") == Some(node)
                        && parent
                            .child_by_field_name("left")
                            .is_some_and(|left| left.kind() == "identifier")
            }) {
                return None;
            }
            node.child_by_field_name("name")?
                .utf8_text(source.as_bytes())
                .ok()?
        }
        _ => return None,
    };
    let loc = module_location(node);
    Some(CallableSpan {
        name: name.into(),
        lines: LineRange {
            start: loc.line,
            end: loc.end_line.unwrap_or(loc.line),
        },
        bytes: node.byte_range(),
        decorators: Vec::new(),
        enclosing: Vec::new(),
        body: None,
    })
}

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_definition"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "line_continuation", "region_start", "region_end"],
    literal_kinds: &[
        "string",
        "string_name",
        "node_path",
        "integer",
        "float",
        "true",
        "false",
        "null",
    ],
    identifier_kinds: &["identifier", "name"],
    keyword_kinds: &[],
    block_kinds: &["body", "class_body", "match_body"],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("string_name", LiteralCategory::String),
        ("node_path", LiteralCategory::String),
        ("integer", LiteralCategory::Number),
        ("float", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("null", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &["expression_statement"],
    kind_roles: &[("name", TokenRole::Declaration)],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &["type"],
    parent_roles: &[
        ("parameters", "", TokenRole::Parameter),
        ("attribute", "", TokenRole::Member),
    ],
    role_wrappers: &[
        "typed_parameter",
        "default_parameter",
        "typed_default_parameter",
        "variadic_parameter",
    ],
    decorator_kinds: &["annotation"],
    context_kinds: &[
        ("class_definition", "class"),
        ("function_definition", "func"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("while_statement", "while"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["call", "attribute_call", "base_call"],
    non_flat_kinds: &["lambda", "await_expression", "conditional_expression"],
    delimited_argument_kinds: &[],
};
