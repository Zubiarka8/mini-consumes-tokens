//! [`TokenSource`] for Bash, over the `tree-sitter-bash` 0.25 node kinds
//! (read off its `node-types.json` and a parse-tree dump):
//!
//! - **Trivia:** `comment` (the shebang too).
//! - **Literals**, one token each: `string`, `raw_string`, `ansi_c_string`,
//!   `translated_string`, `number` and a here-document's `heredoc_body`.
//! - **Identifiers:** `word` and `variable_name`. A shell command is all
//!   words, so a command's name and its plain arguments are identifiers,
//!   as a call's callee is in other languages: the broad normalization
//!   compares the shape of commands, and the consistent one tells a
//!   different command apart. Keywords are every other alphabetic leaf
//!   (`if`, `local`, `do`).
//! - **Blocks:** none. Braces and `do`/`done` are tokens of their own.
//! - **Callables:** `function_definition`, in both forms (`function f` and
//!   `f()`), named by its `name` field with the exact span of the symbol
//!   the parser indexes for it.
//! - **Roles** (syntactic only): a function's name is a declaration; a
//!   command's name is a callee; the variable of an assignment or a `for`
//!   loop is a declaration.
//! - **Literal categories:** strings (interpolated for `"…$x…"` and a
//!   here-document with an expansion) and numbers.
//! - **Callable context:** enclosing functions, conditionals, loops, `case`
//!   items and subshells are labeled.
//! - **Body shape:** a statement is a flat call when it is a `command`
//!   whose subtree holds no command or process substitution, arithmetic or
//!   nested command.

use mct_core::{LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile};
use mct_tree_sitter::{tokenize_source, TokenClasses};

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_definition"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment"],
    literal_kinds: &[
        "string",
        "raw_string",
        "ansi_c_string",
        "translated_string",
        "number",
        "heredoc_body",
    ],
    identifier_kinds: &["word", "variable_name"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("raw_string", LiteralCategory::String),
        ("ansi_c_string", LiteralCategory::String),
        ("translated_string", LiteralCategory::String),
        ("heredoc_body", LiteralCategory::String),
        ("number", LiteralCategory::Number),
    ],
    interpolated_literal_kinds: &[
        ("string", "simple_expansion"),
        ("string", "expansion"),
        ("string", "command_substitution"),
        ("heredoc_body", "simple_expansion"),
        ("heredoc_body", "expansion"),
        ("heredoc_body", "command_substitution"),
    ],
    statement_literal_parents: &[],
    kind_roles: &[],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("function_definition", "name", TokenRole::Declaration),
        ("command_name", "", TokenRole::Callee),
        ("variable_assignment", "name", TokenRole::Declaration),
        ("for_statement", "variable", TokenRole::Declaration),
    ],
    role_wrappers: &[],
    decorator_kinds: &[],
    context_kinds: &[
        ("function_definition", "function"),
        ("if_statement", "if"),
        ("for_statement", "for"),
        ("c_style_for_statement", "for"),
        ("while_statement", "while"),
        ("case_statement", "case"),
        ("case_item", "case"),
        ("subshell", "subshell"),
    ],
    expression_statement_kinds: &[],
    call_kinds: &["command"],
    non_flat_kinds: &[
        "command_substitution",
        "process_substitution",
        "arithmetic_expansion",
    ],
    delimited_argument_kinds: &[],
};

/// Token source for Bash and POSIX shell files.
pub struct BashTokenSource;

impl TokenSource for BashTokenSource {
    fn language_id(&self) -> &'static str {
        "bash"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        tokenize_source(
            &tree_sitter_bash::LANGUAGE.into(),
            &CLASSES,
            relative_path,
            source,
        )
    }
}
