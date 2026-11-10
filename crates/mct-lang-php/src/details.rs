//! Declaration details (ADR-034) read from a PHP declaration's own syntax:
//! its header without attributes or body, parameters (variadic, defaults),
//! the return type, visibility and modifiers. A parameter or return type the
//! syntax leaves out is taken from the PHPDoc block right above (`@param T
//! $name`, `@return T`) with `doc` provenance; a written type always wins.

use mct_core::{
    Modifier, Parameter, ParameterKind, Provenance, SymbolDetail, SymbolId, TypeText, Visibility,
};
use tree_sitter::Node;

/// A function or method: the header up to its body or `;`.
pub(crate) fn function_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let doc = PhpDoc::above(node, source);
    let end = node
        .child_by_field_name("body")
        .map_or_else(|| first_start(node, &[";"]), |b| b.start_byte());
    let parameters = node.child_by_field_name("parameters");
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        parameters_known: parameters.is_some(),
        parameters: parameters.map_or_else(Vec::new, |p| parameter_list(p, &doc, source)),
        return_type: node
            .child_by_field_name("return_type")
            .map(|t| TypeText::syntax(slice(t, source)))
            .or_else(|| doc.returns.map(documented)),
        visibility: member_visibility(node, source),
        modifiers: modifiers(node),
        generics: None,
    }
}

/// A class, interface, trait or enum: the header up to its body.
pub(crate) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        modifiers: modifiers(node),
        ..Default::default()
    }
}

/// One `property_element` of a property declaration (`private int $a, $b;`):
/// the shared modifiers and type, then the element's variable.
pub(crate) fn property_detail(
    declaration: Node,
    element: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = first_start(declaration, &["property_element"]);
    let variable = element
        .child_by_field_name("name")
        .map(|n| slice(n, source))
        .unwrap_or_default();
    SymbolDetail {
        symbol,
        signature: header(declaration, end, source).map(|head| format!("{head} {variable}")),
        visibility: member_visibility(declaration, source),
        modifiers: modifiers(declaration),
        ..Default::default()
    }
}

/// A constructor parameter promoted to a property: its text up to the
/// variable, without attributes or default.
pub(crate) fn promoted_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("name")
        .map_or(node.end_byte(), |n| n.end_byte());
    SymbolDetail {
        symbol,
        signature: header(node, end, source),
        visibility: member_visibility(node, source),
        modifiers: modifiers(node),
        ..Default::default()
    }
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// The start of `node`'s first child of one of `kinds`, else its end.
fn first_start(node: Node, kinds: &[&str]) -> usize {
    children(node)
        .into_iter()
        .find(|c| kinds.contains(&c.kind()))
        .map_or(node.end_byte(), |c| c.start_byte())
}

/// `node`'s text up to `end`, from its first child that is not an
/// attribute list or comment (attributes come first).
fn header(node: Node, end: usize, source: &str) -> Option<String> {
    let start = children(node)
        .into_iter()
        .find(|c| !matches!(c.kind(), "attribute_list" | "comment"))
        .map_or(node.start_byte(), |c| c.start_byte());
    let text = source.get(start..end.max(start))?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The visibility keyword; a method or property without one is public (the
/// language default). A function or type has no visibility.
fn member_visibility(node: Node, source: &str) -> Option<Visibility> {
    let keyword = children(node)
        .into_iter()
        .find(|c| c.kind() == "visibility_modifier")
        .map(|v| slice(v, source));
    match keyword {
        Some(k) if k.starts_with("private") => Some(Visibility::Private),
        Some(k) if k.starts_with("protected") => Some(Visibility::Protected),
        Some(_) => Some(Visibility::Public),
        None => (node.kind() != "function_definition").then_some(Visibility::Public),
    }
}

fn modifiers(node: Node) -> Vec<Modifier> {
    children(node)
        .into_iter()
        .filter_map(|c| match c.kind() {
            "abstract_modifier" => Some(Modifier::Abstract),
            "final_modifier" => Some(Modifier::Final),
            "static_modifier" => Some(Modifier::Static),
            "readonly_modifier" => Some(Modifier::Readonly),
            _ => None,
        })
        .collect()
}

fn parameter_list(list: Node, doc: &PhpDoc, source: &str) -> Vec<Parameter> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|p| {
            matches!(
                p.kind(),
                "simple_parameter" | "variadic_parameter" | "property_promotion_parameter"
            )
        })
        .map(|p| {
            let name = p
                .child_by_field_name("name")
                .and_then(|n| n.named_child(0))
                .map(|n| slice(n, source).to_string());
            let type_text = p
                .child_by_field_name("type")
                .map(|t| TypeText::syntax(slice(t, source)))
                .or_else(|| {
                    name.as_deref()
                        .and_then(|name| doc.param(name))
                        .map(|t| documented(t.to_string()))
                });
            Parameter {
                name,
                type_text,
                default: p
                    .child_by_field_name("default_value")
                    .map(|d| slice(d, source).to_string()),
                kind: if p.kind() == "variadic_parameter" {
                    ParameterKind::Variadic
                } else {
                    ParameterKind::Positional
                },
            }
        })
        .collect()
}

fn documented(text: String) -> TypeText {
    TypeText {
        text,
        provenance: Provenance::Doc,
    }
}

/// The `@param` and `@return` types of the `/** … */` block that directly
/// precedes a declaration (only whitespace between them).
#[derive(Default)]
struct PhpDoc {
    params: Vec<(String, String)>,
    returns: Option<String>,
}

impl PhpDoc {
    fn above(node: Node, source: &str) -> Self {
        let Some(comment) = node.prev_sibling().filter(|c| c.kind() == "comment") else {
            return Self::default();
        };
        let text = slice(comment, source);
        let between = source
            .get(comment.end_byte()..node.start_byte())
            .unwrap_or("x");
        if !text.starts_with("/**") || !between.trim().is_empty() {
            return Self::default();
        }
        Self::parse(text)
    }

    fn parse(text: &str) -> Self {
        let mut doc = Self::default();
        let mut rest = text;
        while let Some(at) = rest.find('@') {
            rest = &rest[at + 1..];
            let (tag, after) = word(rest);
            match tag {
                "param" => {
                    let (ty, after) = type_token(after);
                    let (variable, _) = word(after);
                    let name = variable.trim_start_matches(['&', '.']);
                    if let (false, Some(name)) = (ty.starts_with('$'), name.strip_prefix('$')) {
                        if !ty.is_empty() {
                            doc.params.push((name.to_string(), ty.to_string()));
                        }
                    }
                }
                "return" if doc.returns.is_none() => {
                    let (ty, _) = type_token(after);
                    if !ty.is_empty() {
                        doc.returns = Some(ty.to_string());
                    }
                }
                _ => {}
            }
            rest = after;
        }
        doc
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

/// A PHPDoc type: the next word, where spaces inside `<…>`, `(…)`, `{…}`
/// or `[…]` do not end it (`array<string, int>`). A closing `*/` is not
/// part of it.
fn type_token(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let mut depth = 0_i32;
    let mut end = text.len();
    for (i, c) in text.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    let token = &text[..end];
    (token.strip_suffix("*/").unwrap_or(token), &text[end..])
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}
