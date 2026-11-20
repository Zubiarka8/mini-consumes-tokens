//! HTTP join points (ADR-038) for cpp-httplib: `svr.Get("/x", handler)`
//! declares an endpoint and `cli.Get("/x")` uses one. A lambda handler has no
//! symbol, so it declares nothing. Crow's `CROW_ROUTE(app, "/x")(handler)`
//! declares one too.

use mct_core::{JoinPoint, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

/// The file names cpp-httplib: the evidence `.Get(..)` is a web call.
pub(super) fn mentions_httplib(source: &str) -> bool {
    source.contains("httplib")
}

/// The file uses Crow: the evidence `CROW_ROUTE(..)` is a route.
pub(super) fn mentions_crow(source: &str) -> bool {
    source.contains("CROW_ROUTE")
}

/// `CROW_ROUTE(app, "/x")(handler)`, or with one
/// `.methods(crow::HTTPMethod::Post)` between. Without `.methods` a Crow route
/// answers `GET`; an inline lambda or a method list gives nothing.
pub(super) fn crow_route_of(call: Node, function: Node, source: &str) -> Option<Route> {
    if function.kind() != "call_expression" {
        return None;
    }
    let mut args = call.child_by_field_name("arguments")?.walk();
    let mut handlers = call
        .child_by_field_name("arguments")?
        .named_children(&mut args);
    let (Some(handler), None) = (handlers.next(), handlers.next()) else {
        return None;
    };
    (handler.kind() == "identifier").then_some(())?;
    let mut method = "GET";
    let mut route = function;
    let route = loop {
        let callee = route.child_by_field_name("function")?;
        match callee.kind() {
            "identifier" if text(callee, source) == "CROW_ROUTE" => break route,
            "field_expression"
                if text(callee.child_by_field_name("field")?, source) == "methods" =>
            {
                let args = route.child_by_field_name("arguments")?;
                let mut cursor = args.walk();
                let mut named = args.named_children(&mut cursor);
                let (Some(verb), None) = (named.next(), named.next()) else {
                    return None;
                };
                let verb = text(verb, source).rsplit("::").next()?;
                method = crow_method(verb)?;
                route = callee.child_by_field_name("argument")?;
                if route.kind() != "call_expression" {
                    return None;
                }
            }
            _ => return None,
        }
    };
    let args = route.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut named = args.named_children(&mut cursor);
    let (Some(_app), Some(path), None) = (named.next(), named.next(), named.next()) else {
        return None;
    };
    Some(Route {
        method: method.to_string(),
        path: literal(path, source)?.to_string(),
        handler: text(handler, source).to_string(),
        location: location(call),
    })
}

fn crow_method(name: &str) -> Option<&'static str> {
    Some(match name {
        "Get" => "GET",
        "Post" => "POST",
        "Put" => "PUT",
        "Patch" => "PATCH",
        "Delete" => "DELETE",
        _ => return None,
    })
}

pub(super) use mct_core::NamedRoute as Route;

fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string_literal" {
        return None;
    }
    text(node, source).strip_prefix('"')?.strip_suffix('"')
}

fn method(name: &str) -> Option<&'static str> {
    Some(match name {
        "Get" => "GET",
        "Post" => "POST",
        "Put" => "PUT",
        "Patch" => "PATCH",
        "Delete" => "DELETE",
        "Options" => "OPTIONS",
        _ => return None,
    })
}

fn call_parts<'a>(
    call: Node<'a>,
    function: Node,
    source: &str,
) -> Option<(&'static str, Vec<Node<'a>>)> {
    if function.kind() != "field_expression" {
        return None;
    }
    let method = method(text(function.child_by_field_name("field")?, source))?;
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    Some((method, args.named_children(&mut cursor).collect()))
}

/// `svr.Get("/x", handler)`.
pub(super) fn route_of(call: Node, function: Node, source: &str) -> Option<Route> {
    let (method, args) = call_parts(call, function, source)?;
    let [path, handler] = args.as_slice() else {
        return None;
    };
    (handler.kind() == "identifier").then_some(())?;
    Some(Route {
        method: method.to_string(),
        path: literal(*path, source)?.to_string(),
        handler: text(*handler, source).to_string(),
        location: location(call),
    })
}

/// `cli.Get("/x")`: a client request with a literal path.
pub(super) fn use_of(
    call: Node,
    function: Node,
    owner: SymbolId,
    source: &str,
) -> Option<JoinPoint> {
    let (method, args) = call_parts(call, function, source)?;
    let [path] = args.as_slice() else {
        return None;
    };
    JoinPoint::http_uses(owner, method, literal(*path, source)?, location(call))
}

/// Join points for the routes whose handler is exactly one function of this file.
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[mct_core::SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    mct_core::resolve_routes(routes, symbols, |s| s.kind == SymbolKind::Function)
}
