//! Declaration details (ADR-034) for a Lua function: its header up to the
//! parameter list, its parameters (`function T:m()` declares the implicit
//! `self` receiver, `...` is variadic) and, since Lua writes no types, the
//! LuaLS annotations in the `---` comments right above it (`---@param name
//! type`, `---@return type`), stored with `doc` provenance. Lua has no
//! visibility or modifier syntax.

use mct_core::{Parameter, ParameterKind, Provenance, SymbolDetail, SymbolId, TypeText};
use tree_sitter::Node;

/// `statement` is what the header starts at and the annotations precede:
/// the `function_declaration`, the assignment (`local` included) or the
/// table field. The header runs from `start` (the statement's start, or
/// the assignment target's in a multiple assignment) to the end of the
/// parameters `function` holds.
pub(crate) fn function_detail(
    statement: Node,
    start: usize,
    function: Node,
    method: bool,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let annotations = Annotations::above(statement, source);
    let parameters = function.child_by_field_name("parameters");
    let end = parameters.map_or(function.end_byte(), |p| p.end_byte());
    let mut list = Vec::new();
    if method {
        list.push(Parameter {
            name: Some("self".to_string()),
            type_text: None,
            default: None,
            kind: ParameterKind::Receiver,
        });
    }
    if let Some(parameters) = parameters {
        let mut cursor = parameters.walk();
        for p in parameters.named_children(&mut cursor) {
            let (name, kind) = match p.kind() {
                "identifier" => (
                    Some(slice(p, source).to_string()),
                    ParameterKind::Positional,
                ),
                "vararg_expression" => (None, ParameterKind::Variadic),
                _ => continue,
            };
            let key = name.as_deref().unwrap_or("...");
            list.push(Parameter {
                type_text: annotations.param(key).map(documented),
                name,
                default: None,
                kind,
            });
        }
    }
    let signature = source.get(start..end.max(start)).map(str::trim);
    SymbolDetail {
        symbol,
        signature: signature.filter(|s| !s.is_empty()).map(str::to_string),
        parameters_known: parameters.is_some(),
        parameters: list,
        return_type: (!annotations.returns.is_empty())
            .then(|| documented(&annotations.returns.join(", "))),
        ..Default::default()
    }
}

fn documented(text: &str) -> TypeText {
    TypeText {
        text: text.to_string(),
        provenance: Provenance::Doc,
    }
}

/// The `@param` and `@return` annotations of the `---` comment lines
/// directly above a statement (only whitespace between them).
#[derive(Default)]
struct Annotations {
    params: Vec<(String, String)>,
    returns: Vec<String>,
}

impl Annotations {
    fn above(statement: Node, source: &str) -> Self {
        let mut lines = Vec::new();
        let mut next_start = statement.start_byte();
        let mut sibling = statement.prev_sibling();
        while let Some(comment) = sibling.filter(|c| c.kind() == "comment") {
            let between = source.get(comment.end_byte()..next_start).unwrap_or("x");
            let text = slice(comment, source);
            if !between.trim().is_empty() || !text.starts_with("---") {
                break;
            }
            lines.push(text);
            next_start = comment.start_byte();
            sibling = comment.prev_sibling();
        }
        lines.reverse();
        let mut annotations = Self::default();
        for line in lines {
            let body = line.trim_start_matches('-').trim_start();
            if let Some(rest) = body.strip_prefix("@param") {
                let (name, rest) = word(rest);
                let (ty, _) = type_token(rest);
                if !ty.is_empty() {
                    let name = name.trim_end_matches('?');
                    annotations.params.push((name.to_string(), ty.to_string()));
                }
            } else if let Some(rest) = body.strip_prefix("@return") {
                let (ty, _) = type_token(rest);
                if !ty.is_empty() {
                    annotations.returns.push(ty.to_string());
                }
            }
        }
        annotations
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.as_str())
    }
}

/// The next whitespace-separated word of `text`, and what follows it.
fn word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// A LuaLS type: the next word, where spaces inside `<…>`, `(…)`, `{…}` or
/// `[…]` do not end it (`table<string, integer>`), nor does the space after
/// a function type's `):` (`fun(x: integer): boolean`).
fn type_token(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let mut depth = 0_i32;
    let mut end = text.len();
    let mut previous = ' ';
    for (i, c) in text.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 && previous != ':' => {
                end = i;
                break;
            }
            _ => {}
        }
        if !c.is_whitespace() {
            previous = c;
        }
    }
    (text[..end].trim_end(), &text[end..])
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
