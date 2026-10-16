//! [`TokenSource`] for Python, over the `tree-sitter-python` 0.25 node kinds.
//!
//! The kind tables below were read off the real grammar (its `node-types.json`
//! and parsed probes), not guessed:
//!
//! - **Trivia:** `comment` (a shebang line is one too) and `line_continuation`
//!   (the backslash that joins two lines).
//! - **Literals**, one token each: `string` (plain, raw, bytes, triple-quoted
//!   and f-strings, whatever their `string_start`/`string_content`/
//!   `interpolation`/`escape_sequence`/`string_end` children), `integer`,
//!   `float`, and the constants `true`, `false`, `none` (`True`, `False`,
//!   `None`) and `ellipsis` (`...`). The constants are literals, not keywords,
//!   so a clone detector sees `x = None` and `x = 0` as the same shape. Implicit
//!   concatenation (`"a" "b"`) is one literal per string.
//! - **Identifiers:** `identifier`, which also covers `self`, `cls`, builtin
//!   names and soft keywords used as names (`match`, `type`).
//! - **Keywords:** every other alphabetic leaf: `def`, `class`, `if`, `return`,
//!   `async`, `await`, `lambda`, `not`, `and`, `in`, `is`, `pass`, `break`, ...
//!
//! - **Blocks:** `block`, the body of every compound statement (indented or a
//!   one-line suite). The grammar's indent/dedent markers are hidden, so
//!   without a [`mct_core::TokenKind::BlockStart`]/`BlockEnd` pair around each
//!   `block`, a statement moved in or out of an `if` would leave the token
//!   sequence unchanged although the behavior changes. Indentation itself
//!   still produces no token, so a one-line suite and its indented form match.

//!
//! - **Roles** (syntactic only; nothing is resolved): a name inside a `type`
//!   node (parameter, return and variable annotations) is a type; the
//!   `attribute` of an attribute access and a keyword argument's name are
//!   members; the bare `function` of a call is a callee; names in
//!   `parameters`, `lambda_parameters`, `typed_parameter` and the `name` of a
//!   (typed) default parameter are parameters; a `function_definition` or
//!   `class_definition` name is a declaration. Splat parameters (`*args`,
//!   `**kwargs`, typed or not) are parameters too: `list_splat_pattern` and
//!   `dictionary_splat_pattern` are role wrappers, so the same patterns in an
//!   unpacking target (`head, *rest = items`, `for *init, last in pairs`) stay
//!   unclassified, as do the splat arguments of a call (`f(*args)`). Every
//!   local, global and import name stays unclassified.
//! - **Literal categories:** an f-string (a `string` with an `interpolation`
//!   child) is interpolated in every mode; a string that is a whole
//!   `expression_statement` is a docstring.
//! - **Callable context:** the `decorator`s of a `decorated_definition` attach
//!   to its function; enclosing classes, functions and compound statements
//!   (`if`/`elif`/`else`, `try`/`except`/`finally`, `with`, `for`, `while`,
//!   `match`/`case`) are labeled, a decorated class as `@dataclass class`.
//!   Only kinds are labeled: conditions, context managers and class names are
//!   not part of the label.
//! - **Body shape:** the statements of a function's `body` block, a leading or
//!   any other string statement (docstring) excluded. One is a flat call when
//!   it is an `expression_statement` holding only a `call` whose subtree has
//!   no other `call`, `lambda`, comprehension, generator,
//!   `conditional_expression`, `await` or `:=`; an f-string interpolation is
//!   searched like any other argument. `await f()`, `return f()` and
//!   `x = f()` are not flat calls. Callee names are never consulted.

use mct_core::{LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile};
use mct_tree_sitter::{tokenize_source, TokenClasses};

const INTERPOLATED: &[(&str, &str)] = &[("string", "interpolation")];

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_definition"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "line_continuation"],
    literal_kinds: &[
        "string", "integer", "float", "true", "false", "none", "ellipsis",
    ],
    identifier_kinds: &["identifier"],
    keyword_kinds: &[],
    block_kinds: &["block"],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("string_content", LiteralCategory::String),
        ("escape_sequence", LiteralCategory::String),
        ("integer", LiteralCategory::Number),
        ("float", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("none", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: INTERPOLATED,
    statement_literal_parents: &["expression_statement"],
    kind_roles: &[],
    primitive_spellings: &[
        ("identifier", "int"),
        ("identifier", "str"),
        ("identifier", "float"),
        ("identifier", "complex"),
        ("identifier", "bool"),
        ("identifier", "bytes"),
    ],
    annotation_sites: &[
        ("typed_parameter", "type"),
        ("typed_default_parameter", "type"),
        ("function_definition", "return_type"),
        ("assignment", "type"),
    ],
    annotation_wrappers: &["type"],
    type_kinds: &["type"],
    parent_roles: &[
        ("function_definition", "name", TokenRole::Declaration),
        ("class_definition", "name", TokenRole::Declaration),
        ("parameters", "", TokenRole::Parameter),
        ("lambda_parameters", "", TokenRole::Parameter),
        ("typed_parameter", "", TokenRole::Parameter),
        ("default_parameter", "name", TokenRole::Parameter),
        ("typed_default_parameter", "name", TokenRole::Parameter),
        ("attribute", "attribute", TokenRole::Member),
        ("keyword_argument", "name", TokenRole::Member),
        ("call", "function", TokenRole::Callee),
    ],
    role_wrappers: &["list_splat_pattern", "dictionary_splat_pattern"],
    decorator_kinds: &["decorator"],
    context_kinds: &[
        ("class_definition", "class"),
        ("function_definition", "function"),
        ("if_statement", "if"),
        ("elif_clause", "elif"),
        ("else_clause", "else"),
        ("try_statement", "try"),
        ("except_clause", "except"),
        ("finally_clause", "finally"),
        ("with_statement", "with"),
        ("for_statement", "for"),
        ("while_statement", "while"),
        ("match_statement", "match"),
        ("case_clause", "case"),
    ],
    expression_statement_kinds: &["expression_statement"],
    call_kinds: &["call"],
    non_flat_kinds: &[
        "lambda",
        "list_comprehension",
        "set_comprehension",
        "dictionary_comprehension",
        "generator_expression",
        "conditional_expression",
        "await",
        "named_expression",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for Python files.
pub struct PythonTokenSource;

impl TokenSource for PythonTokenSource {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source(
            &tree_sitter_python::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
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
                "string_content",
                "escape_sequence",
                "integer",
                "float",
                "true",
                "false",
                "none",
                "ellipsis",
            ],
            ..CLASSES
        };
        tokenize_source(
            &tree_sitter_python::LANGUAGE.into(),
            &classes,
            relative_path,
            source,
        )
    }
}
