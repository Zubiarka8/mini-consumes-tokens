//! HTTP join points (ADR-038): ASP.NET Core `[HttpGet("x")]` attributes (with
//! the controller's `[Route]` prefix) and minimal-API `app.MapGet("/x", h)`
//! declare an endpoint, and `HttpClient.GetAsync("/x")` uses one. Routing is
//! case-insensitive in ASP.NET; matching here is exact, so a differently
//! cased client path gives no link. A `[controller]` token gives none either.

use mct_core::{JoinPoint, Location, SymbolId, SymbolKind, SymbolRecord};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

/// The file names ASP.NET or an HTTP client: the evidence the calls are web calls.
pub(super) fn mentions_web(source: &str) -> bool {
    ["AspNetCore", "HttpClient", "Microsoft.AspNetCore"]
        .iter()
        .any(|name| source.contains(name))
}

fn http_method(name: &str) -> Option<&'static str> {
    let name = name.strip_suffix("Attribute").unwrap_or(name);
    Some(match name.strip_prefix("Http").unwrap_or(name) {
        "Get" => "GET",
        "Post" => "POST",
        "Put" => "PUT",
        "Patch" => "PATCH",
        "Delete" => "DELETE",
        "Head" => "HEAD",
        "Options" => "OPTIONS",
        _ => return None,
    })
}

fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string_literal" {
        return None;
    }
    text(node, source).strip_prefix('"')?.strip_suffix('"')
}

/// The attributes of `declaration`, as `(name, first argument)`.
fn attributes<'t, 's>(declaration: Node<'t>, source: &'s str) -> Vec<(&'s str, Option<Node<'t>>)> {
    let mut found = Vec::new();
    let mut cursor = declaration.walk();
    for list in declaration
        .children(&mut cursor)
        .filter(|n| n.kind() == "attribute_list")
    {
        let mut list_cursor = list.walk();
        for attribute in list
            .named_children(&mut list_cursor)
            .filter(|n| n.kind() == "attribute")
        {
            let Some(name) = attribute.child_by_field_name("name") else {
                continue;
            };
            let first = attribute
                .named_children(&mut attribute.walk())
                .find(|n| n.kind() == "attribute_argument_list")
                .and_then(|args| args.named_child(0))
                .and_then(|arg| arg.named_child(0));
            found.push((text(name, source), first));
        }
    }
    found
}

fn route_of_attribute<'t, 's>(
    attributes: &[(&'s str, Option<Node<'t>>)],
    names: &[&str],
    source: &'s str,
) -> Result<Option<&'s str>, ()> {
    let mut route = None;
    for (name, argument) in attributes {
        let name = name.strip_suffix("Attribute").unwrap_or(name);
        if !names.contains(&name) {
            continue;
        }
        let path = match argument {
            None => "",
            Some(argument) => literal(*argument, source).ok_or(())?,
        };
        if route.replace(path).is_some() {
            return Err(());
        }
    }
    Ok(route)
}

fn class_prefix<'a>(method: Node, source: &'a str) -> Result<&'a str, ()> {
    let mut parent = method.parent();
    while let Some(node) = parent {
        if node.kind() == "class_declaration" {
            return Ok(
                route_of_attribute(&attributes(node, source), &["Route"], source)?.unwrap_or(""),
            );
        }
        parent = node.parent();
    }
    Ok("")
}

fn full_path(prefix: &str, path: &str) -> Option<String> {
    let joined = if let Some(absolute) = path.strip_prefix("~/") {
        format!("/{absolute}")
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}/{}", prefix.trim_matches('/'), path)
    };
    // A route token (`[controller]`) is a name this parser cannot expand.
    let token =
        joined.contains("[controller]") || joined.contains("[action]") || joined.contains("[area]");
    (!token).then(|| joined.replace("//", "/"))
}

/// The endpoints a controller action's `[HttpGet]`-style attributes declare.
pub(super) fn declarations(method: Node, source: &str, symbol: SymbolId) -> Vec<JoinPoint> {
    let Ok(prefix) = class_prefix(method, source) else {
        return Vec::new();
    };
    let attributes = attributes(method, source);
    let Ok(own_route) = route_of_attribute(&attributes, &["Route"], source) else {
        return Vec::new();
    };
    let mut points = Vec::new();
    for (name, argument) in &attributes {
        let Some(http_method) = http_method(name).filter(|_| name.starts_with("Http")) else {
            continue;
        };
        let path = match argument {
            None => own_route.unwrap_or(""),
            Some(argument) => match literal(*argument, source) {
                Some(path) => path,
                None => continue,
            },
        };
        if let Some(full) = full_path(prefix, path) {
            points.extend(JoinPoint::http_declares(
                symbol,
                http_method,
                &full,
                location(method),
            ));
        }
    }
    points
}

/// A minimal-API route whose handler is still a name, resolved by [`resolve`].
pub(super) struct Route {
    method: &'static str,
    path: String,
    handler: String,
    location: Location,
}

fn arguments(call: Node) -> Vec<Node> {
    let Some(list) = call.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter_map(|arg| arg.named_child(0))
        .collect()
}

/// `app.MapGet("/x", Handler)`.
pub(super) fn route_of(call: Node, function: Node, source: &str) -> Option<Route> {
    if function.kind() != "member_access_expression" {
        return None;
    }
    let name = text(function.child_by_field_name("name")?, source);
    let method = http_method(name.strip_prefix("Map")?)?;
    let args = arguments(call);
    let [path, .., handler] = args.as_slice() else {
        return None;
    };
    (handler.kind() == "identifier").then_some(())?;
    Some(Route {
        method,
        path: literal(*path, source)?.to_string(),
        handler: text(*handler, source).to_string(),
        location: location(call),
    })
}

/// `client.GetAsync("/x")`: an HTTP client call with a literal path.
pub(super) fn use_of(
    call: Node,
    function: Node,
    owner: SymbolId,
    source: &str,
) -> Option<JoinPoint> {
    if function.kind() != "member_access_expression" {
        return None;
    }
    let name = text(function.child_by_field_name("name")?, source);
    let method = http_method(name.strip_suffix("Async")?)?;
    let args = arguments(call);
    JoinPoint::http_uses(
        owner,
        method,
        literal(*args.first()?, source)?,
        location(call),
    )
}

/// Join points for the routes whose handler is exactly one method or local
/// function of this file.
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    routes.into_iter().filter_map(move |route| {
        let mut handlers = symbols.iter().filter(|s| {
            s.name == route.handler && matches!(s.kind, SymbolKind::Method | SymbolKind::Function)
        });
        let (Some(handler), None) = (handlers.next(), handlers.next()) else {
            return None;
        };
        JoinPoint::http_declares(handler.id, route.method, &route.path, route.location)
    })
}
