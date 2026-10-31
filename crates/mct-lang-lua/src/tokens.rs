//! [`TokenSource`] for Lua, over the `tree-sitter-lua` 0.5 node kinds.
//!
//! The kind tables were read off the grammar's `node-types.json`, not guessed:
//!
//! - **Trivia:** `comment` (line and long `--[[ ]]` comments) and
//!   `hash_bang_line`.
//! - **Literals**, one token each: `string` (short and long `[[ ]]` strings,
//!   with their `string_content`/`escape_sequence` children), `number`, and the
//!   constants `true`, `false` and `nil`.
//! - **Identifiers:** `identifier`. Keywords are every other alphabetic leaf.
//! - **Blocks:** none. Lua delimits blocks with `do`/`then`/`end`, `repeat`/
//!   `until` and `function`/`end` keywords that are tokens of their own.
//! - **Callables:** `function_declaration` (`function f`, `local function f`,
//!   `function M.f`, `function M:f`) and a `function_definition` bound by an
//!   assignment (`local f = function() end`, `M.f = function() end`). A
//!   callable is named after the symbol the parser indexes for it: the last
//!   segment of a dotted or method name, so `M.f` and `M:f` are `f`. The
//!   anonymous ones have no enclosing-construct labels or body shape.
//! - **Roles** (syntactic only): the name of a `function_declaration` is a
//!   declaration, the `field` of a dotted access and the `method` of a method
//!   access are members, the `name` of a call is a callee, and the names in
//!   `parameters` are parameters.
//! - **Literal categories:** strings, numbers, booleans and `nil`.
//!   Lua has no docstring statement.
//! - **Callable context:** enclosing `function_declaration`s and compound
//!   statements (`if`, `for`, `while`, `repeat`, `do`) are labeled.
//! - **Body shape:** a statement is a flat call when it is a `function_call`
//!   whose subtree holds no other call or anonymous function.

use mct_core::{
    CallableSpan, LineRange, LiteralCategory, ParseError, TokenRole, TokenSource, TokenizedFile,
};
use mct_tree_sitter::{tokenize_tree, TokenClasses};
use tree_sitter::{Node, Parser};

const CLASSES: TokenClasses = TokenClasses {
    callable_kinds: &["function_declaration"],
    expanded_literal_kinds: &[],
    trivia_kinds: &["comment", "hash_bang_line"],
    literal_kinds: &["string", "number", "true", "false", "nil"],
    identifier_kinds: &["identifier"],
    keyword_kinds: &[],
    block_kinds: &[],
    literal_categories: &[
        ("string", LiteralCategory::String),
        ("string_content", LiteralCategory::String),
        ("escape_sequence", LiteralCategory::String),
        ("number", LiteralCategory::Number),
        ("true", LiteralCategory::Boolean),
        ("false", LiteralCategory::Boolean),
        ("nil", LiteralCategory::Null),
    ],
    interpolated_literal_kinds: &[],
    statement_literal_parents: &[],
    kind_roles: &[],
    primitive_spellings: &[],
    annotation_sites: &[],
    annotation_wrappers: &[],
    type_kinds: &[],
    parent_roles: &[
        ("function_declaration", "name", TokenRole::Declaration),
        ("parameters", "", TokenRole::Parameter),
        ("dot_index_expression", "field", TokenRole::Member),
        ("method_index_expression", "method", TokenRole::Member),
        ("function_call", "name", TokenRole::Callee),
    ],
    role_wrappers: &[],
    decorator_kinds: &[],
    context_kinds: &[
        ("function_declaration", "function"),
        ("if_statement", "if"),
        ("elseif_statement", "elseif"),
        ("else_statement", "else"),
        ("for_statement", "for"),
        ("while_statement", "while"),
        ("repeat_statement", "repeat"),
        ("do_statement", "do"),
    ],
    expression_statement_kinds: &[],
    call_kinds: &["function_call"],
    non_flat_kinds: &["function_definition"],
    delimited_argument_kinds: &[],
};

/// Token source for Lua files.
pub struct LuaTokenSource;

impl TokenSource for LuaTokenSource {
    fn language_id(&self) -> &'static str {
        "lua"
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        let failure = |message: &str| ParseError::Syntax {
            path: relative_path.to_string(),
            line: 1,
            message: message.to_string(),
        };
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .map_err(|_| failure("tree-sitter grammar could not be loaded"))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| failure("tree-sitter produced no parse tree"))?;
        let mut file = tokenize_tree(&tree, source, &CLASSES);
        if let Some(callables) = file.callables.as_mut() {
            for callable in callables.iter_mut() {
                callable.name = last_segment(&callable.name).to_string();
            }
            collect_assigned_functions(tree.root_node(), source, callables);
            callables.sort_by_key(|c| (c.lines.start, c.bytes.start));
        }
        Ok(file)
    }
}

/// `M.f` and `M:f` name the symbol `f`.
fn last_segment(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name)
}

/// Adds a callable for every `target = function … end` pair, spanning the
/// target through `end` exactly as the parser's symbol does.
fn collect_assigned_functions(root: Node, source: &str, out: &mut Vec<CallableSpan>) {
    let mut cursor = root.walk();
    // Iterative, so nesting depth costs heap, never stack.
    'walk: loop {
        let node = cursor.node();
        if node.kind() == "assignment_statement" {
            push_assigned_functions(node, source, out);
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
}

fn push_assigned_functions(assignment: Node, source: &str, out: &mut Vec<CallableSpan>) {
    let mut cursor = assignment.walk();
    let children: Vec<Node> = assignment.children(&mut cursor).collect();
    let (Some(variables), Some(values)) = (
        children.iter().find(|n| n.kind() == "variable_list"),
        children.iter().find(|n| n.kind() == "expression_list"),
    ) else {
        return;
    };
    let mut variable_cursor = variables.walk();
    let targets: Vec<Node> = variables
        .named_children(&mut variable_cursor)
        .filter(|n| n.kind() != "attribute")
        .collect();
    let mut value_cursor = values.walk();
    for (target, value) in targets.iter().zip(values.named_children(&mut value_cursor)) {
        if value.kind() != "function_definition" {
            continue;
        }
        let name_node = match target.kind() {
            "identifier" => Some(*target),
            "dot_index_expression" => target.child_by_field_name("field"),
            _ => None,
        };
        let Some(name) = name_node.and_then(|n| source.get(n.byte_range())) else {
            continue;
        };
        out.push(CallableSpan {
            name: name.to_string(),
            lines: LineRange {
                start: target.start_position().row as u32 + 1,
                end: value.end_position().row as u32 + 1,
            },
            bytes: target.start_byte()..value.end_byte(),
            decorators: Vec::new(),
            enclosing: Vec::new(),
            body: None,
        });
    }
}
