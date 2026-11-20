//! HTTP join points (ADR-038): Spring `@GetMapping`-style annotations declare
//! an endpoint for their function, with the class's `@RequestMapping` prefix.
//! Ktor's `get("/x") { .. }` has no named handler, so it declares nothing. A
//! Retrofit `@GET("/x")` interface function uses an endpoint.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::{find_child, location};
use tree_sitter::Node;

use super::{annotation_class_name, text};

/// The file imports Spring: the evidence its mapping annotations are routes.
pub(super) fn mentions_spring(source: &str) -> bool {
    source.contains("springframework")
}

fn mapping_method(annotation: &str) -> Option<&'static str> {
    Some(match annotation {
        "GetMapping" => "GET",
        "PostMapping" => "POST",
        "PutMapping" => "PUT",
        "PatchMapping" => "PATCH",
        "DeleteMapping" => "DELETE",
        _ => return None,
    })
}

fn annotations(declaration: Node) -> Vec<Node> {
    let Some(modifiers) = find_child(declaration, "modifiers") else {
        return Vec::new();
    };
    let mut cursor = modifiers.walk();
    modifiers
        .named_children(&mut cursor)
        .filter(|m| m.kind() == "annotation")
        .collect()
}

/// A string literal without a `$` template.
fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string_literal" {
        return None;
    }
    let raw = text(node, source).strip_prefix('"')?.strip_suffix('"')?;
    (!raw.contains('$')).then_some(raw)
}

fn value_arguments<'a>(annotation: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = annotation.walk();
    let call = annotation
        .named_children(&mut cursor)
        .find(|part| part.kind() == "constructor_invocation");
    let Some(call) = call else {
        return Vec::new();
    };
    let Some(arguments) = find_child(call, "value_arguments") else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    arguments.named_children(&mut cursor).collect()
}

/// The argument's name (`value =`, `path =`, `method =`) and its expression.
fn argument<'a>(argument: Node<'a>, source: &str) -> (Option<String>, Option<Node<'a>>) {
    let mut cursor = argument.walk();
    let parts: Vec<Node> = argument.named_children(&mut cursor).collect();
    match parts.as_slice() {
        [name, value] if matches!(name.kind(), "simple_identifier" | "identifier") => {
            (Some(text(*name, source).to_string()), Some(*value))
        }
        [value] => (None, Some(*value)),
        _ => (None, None),
    }
}

/// The one literal path an annotation maps; `Ok("")` for no path, `Err` when
/// it is several paths or not a literal.
fn mapped_path<'a>(annotation: Node, source: &'a str) -> Result<&'a str, ()> {
    let mut path = None;
    for arg in value_arguments(annotation) {
        let (name, value) = argument(arg, source);
        if name
            .as_deref()
            .is_some_and(|n| !matches!(n, "value" | "path"))
        {
            continue;
        }
        let value = value.ok_or(())?;
        if path.replace(literal(value, source).ok_or(())?).is_some() {
            return Err(());
        }
    }
    Ok(path.unwrap_or(""))
}

fn request_mapping_method(annotation: Node, source: &str) -> Option<String> {
    value_arguments(annotation).into_iter().find_map(|arg| {
        let (name, value) = argument(arg, source);
        let value = value?;
        (name.as_deref() == Some("method"))
            .then(|| {
                text(value, source)
                    .strip_prefix("RequestMethod.")
                    .map(str::to_string)
            })
            .flatten()
    })
}

fn annotation_name<'a>(annotation: Node, source: &'a str) -> &'a str {
    annotation_class_name(annotation).map_or("", |name| text(name, source))
}

fn class_prefix<'a>(function: Node, source: &'a str) -> Result<&'a str, ()> {
    let mut parent = function.parent();
    while let Some(node) = parent {
        if node.kind() == "class_declaration" {
            for annotation in annotations(node) {
                if annotation_name(annotation, source) == "RequestMapping" {
                    return mapped_path(annotation, source);
                }
            }
            return Ok("");
        }
        parent = node.parent();
    }
    Ok("")
}

fn join(prefix: &str, path: &str) -> String {
    let mut joined = prefix.trim_end_matches('/').to_string();
    if !path.is_empty() && !path.starts_with('/') {
        joined.push('/');
    }
    joined.push_str(path);
    joined
}

/// The endpoints a function's mapping annotations declare.
pub(super) fn declarations(function: Node, source: &str, symbol: SymbolId) -> Vec<JoinPoint> {
    let Ok(prefix) = class_prefix(function, source) else {
        return Vec::new();
    };
    let mut points = Vec::new();
    for annotation in annotations(function) {
        let name = annotation_name(annotation, source);
        let method = mapping_method(name).map(str::to_string).or_else(|| {
            (name == "RequestMapping")
                .then(|| request_mapping_method(annotation, source))
                .flatten()
        });
        let (Some(method), Ok(path)) = (method, mapped_path(annotation, source)) else {
            continue;
        };
        let full = join(prefix, path);
        if !full.is_empty() {
            points.extend(JoinPoint::http_declares(
                symbol,
                &method,
                &full,
                location(annotation),
            ));
        }
    }
    points
}

/// The file imports Retrofit: the evidence its `@GET("/x")` is a request.
pub(super) fn mentions_retrofit(source: &str) -> bool {
    source.contains("retrofit2")
}

/// A Retrofit interface function `@GET("/x")` uses the endpoint it names. A
/// path without a leading slash is relative to a base URL the file does not
/// state, so it gives nothing.
pub(super) fn retrofit_use(function: Node, source: &str, symbol: SymbolId) -> Option<JoinPoint> {
    let mut verbs = annotations(function).into_iter().filter_map(|annotation| {
        let verb = ["GET", "POST", "PUT", "PATCH", "DELETE"]
            .into_iter()
            .find(|verb| *verb == annotation_name(annotation, source))?;
        Some((annotation, verb))
    });
    let (Some((annotation, verb)), None) = (verbs.next(), verbs.next()) else {
        return None;
    };
    let path = mapped_path(annotation, source).ok()?;
    JoinPoint::http_uses(symbol, verb, path, location(annotation))
}
