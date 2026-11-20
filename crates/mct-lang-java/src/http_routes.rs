//! HTTP join points (ADR-038): Spring `@GetMapping`-style annotations declare
//! an endpoint for their method (with the class's `@RequestMapping` prefix),
//! and a MockMvc `get("/x")` request builder uses one. JAX-RS `@GET` with
//! `@Path` declares one too, and a Retrofit `@GET("/x")` interface method uses
//! one.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::{find_child, location};
use tree_sitter::Node;

use super::text;

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

fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string_literal" {
        return None;
    }
    text(node, source).strip_prefix('"')?.strip_suffix('"')
}

fn annotations<'a>(declaration: Node<'a>) -> Vec<Node<'a>> {
    let Some(modifiers) = find_child(declaration, "modifiers") else {
        return Vec::new();
    };
    let mut cursor = modifiers.walk();
    modifiers
        .children(&mut cursor)
        .filter(|m| matches!(m.kind(), "marker_annotation" | "annotation"))
        .collect()
}

fn annotation_name<'a>(annotation: Node, source: &'a str) -> &'a str {
    annotation
        .child_by_field_name("name")
        .map_or("", |name| text(name, source))
}

/// The one literal path an annotation maps: `("/x")`, `(value = "/x")` or
/// `(path = "/x")`. `Ok("")` for no path at all, `Err` when it is not a single
/// literal (several paths, a constant).
fn mapped_path<'a>(annotation: Node, source: &'a str) -> Result<&'a str, ()> {
    let Some(args) = annotation.child_by_field_name("arguments") else {
        return Ok("");
    };
    let mut cursor = args.walk();
    let mut path = None;
    for arg in args.named_children(&mut cursor) {
        let value = match arg.kind() {
            "element_value_pair" => {
                let key = arg
                    .child_by_field_name("key")
                    .map_or("", |k| text(k, source));
                if !matches!(key, "value" | "path") {
                    continue;
                }
                arg.child_by_field_name("value").ok_or(())?
            }
            _ => arg,
        };
        if path.replace(literal(value, source).ok_or(())?).is_some() {
            return Err(());
        }
    }
    Ok(path.unwrap_or(""))
}

/// `@RequestMapping(method = RequestMethod.X)` of a method.
fn request_mapping_method(annotation: Node, source: &str) -> Option<String> {
    let args = annotation.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let pair = args.named_children(&mut cursor).find(|arg| {
        arg.kind() == "element_value_pair"
            && arg
                .child_by_field_name("key")
                .is_some_and(|k| text(k, source) == "method")
    })?;
    let value = text(pair.child_by_field_name("value")?, source);
    let name = value.strip_prefix("RequestMethod.")?;
    Some(name.to_string())
}

/// The path prefix of the class around `method`: `Ok("")` without a class
/// `@RequestMapping`, `Err` when that mapping is not one literal.
fn class_prefix<'a>(method: Node, source: &'a str) -> Result<&'a str, ()> {
    let mut parent = method.parent();
    while let Some(node) = parent {
        if matches!(node.kind(), "class_declaration" | "interface_declaration") {
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

/// The endpoints a method's mapping annotations declare.
pub(super) fn declarations(method: Node, source: &str, symbol: SymbolId) -> Vec<JoinPoint> {
    let Ok(prefix) = class_prefix(method, source) else {
        return Vec::new();
    };
    let mut points = Vec::new();
    for annotation in annotations(method) {
        let name = annotation_name(annotation, source);
        let Some(http_method) = mapping_method(name).map(str::to_string).or_else(|| {
            (name == "RequestMapping")
                .then(|| request_mapping_method(annotation, source))
                .flatten()
        }) else {
            continue;
        };
        let Ok(path) = mapped_path(annotation, source) else {
            continue;
        };
        let full = join(prefix, path);
        if full.is_empty() {
            continue;
        }
        points.extend(JoinPoint::http_declares(
            symbol,
            &http_method,
            &full,
            location(annotation),
        ));
    }
    points
}

/// MockMvc's `get("/x")`, `post("/x")`: a request builder with a literal path.
pub(super) fn use_of(call: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    let name = text(call.child_by_field_name("name")?, source);
    let method = mapping_method(&format!("{}Mapping", capitalize(name)))?;
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let first = args.named_children(&mut cursor).next()?;
    JoinPoint::http_uses(owner, method, literal(first, source)?, location(call))
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

/// The file imports JAX-RS: the evidence its `@GET` and `@Path` are a route.
pub(super) fn mentions_jaxrs(source: &str) -> bool {
    source.contains("javax.ws.rs") || source.contains("jakarta.ws.rs")
}

/// The file imports Retrofit: the evidence its `@GET("/x")` is a request.
pub(super) fn mentions_retrofit(source: &str) -> bool {
    source.contains("retrofit2")
}

/// The one `@Path("..")` literal on a declaration: `Ok(None)` without one,
/// `Err` when it is not a single literal.
fn path_annotation<'a>(declaration: Node, source: &'a str) -> Result<Option<&'a str>, ()> {
    for annotation in annotations(declaration) {
        if annotation_name(annotation, source) == "Path" {
            return mapped_path(annotation, source).map(Some);
        }
    }
    Ok(None)
}

/// The HTTP method of a JAX-RS or Retrofit annotation (`@GET`, `@POST`..).
fn verb(annotation: &str) -> Option<&'static str> {
    ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .into_iter()
        .find(|verb| *verb == annotation)
}

/// The one HTTP method annotation of `method`, with the node it is on.
fn sole_verb<'a>(method: Node<'a>, source: &str) -> Option<(Node<'a>, &'static str)> {
    let mut verbs = annotations(method)
        .into_iter()
        .filter_map(|a| Some((a, verb(annotation_name(a, source))?)));
    let (Some(found), None) = (verbs.next(), verbs.next()) else {
        return None;
    };
    Some(found)
}

/// The endpoint a JAX-RS resource method declares: its HTTP method annotation
/// with the class `@Path` and its own `@Path`.
pub(super) fn jaxrs_declaration(method: Node, source: &str, symbol: SymbolId) -> Option<JoinPoint> {
    let (annotation, verb) = sole_verb(method, source)?;
    let mut class = method.parent();
    while let Some(node) =
        class.filter(|n| !matches!(n.kind(), "class_declaration" | "interface_declaration"))
    {
        class = node.parent();
    }
    let prefix = path_annotation(class?, source).ok()?.unwrap_or("");
    let own = path_annotation(method, source).ok()?.unwrap_or("");
    JoinPoint::http_declares(symbol, verb, &join(prefix, own), location(annotation))
}

/// A Retrofit interface method `@GET("/x")` uses the endpoint it names. A
/// path without a leading slash is relative to a base URL the file does not
/// state, so it gives nothing.
pub(super) fn retrofit_use(method: Node, source: &str, symbol: SymbolId) -> Option<JoinPoint> {
    let (annotation, verb) = sole_verb(method, source)?;
    annotation.child_by_field_name("arguments")?;
    let path = mapped_path(annotation, source).ok()?;
    JoinPoint::http_uses(symbol, verb, path, location(annotation))
}
