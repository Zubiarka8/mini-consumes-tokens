//! HTTP route declarations: Actix Web method attributes, which attach
//! directly to their handler, and axum `.route("/path", get(handler))` calls.

use mct_core::{http_endpoint, JoinKind, JoinPoint, JoinRole, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

pub(super) use mct_core::NamedRoute as Route;

fn method(name: &str) -> Option<&'static str> {
    Some(match name {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "head" => "HEAD",
        "options" => "OPTIONS",
        "trace" => "TRACE",
        "connect" => "CONNECT",
        _ => return None,
    })
}

/// The contents of a plain or raw string literal.
fn string_contents<'a>(literal: Node, source: &'a str) -> Option<&'a str> {
    let raw = text(literal, source);
    match literal.kind() {
        "string_literal" => raw.strip_prefix('"')?.strip_suffix('"'),
        "raw_string_literal" => {
            let (prefix, contents) = raw.split_once('"')?;
            contents
                .strip_suffix(prefix.strip_prefix('r')?)?
                .strip_suffix('"')
        }
        _ => None,
    }
}

pub(super) fn declaration(item: Node, source: &str, symbol: SymbolId) -> Option<JoinPoint> {
    let attribute = item.named_child(0)?;
    let name = text(attribute.named_child(0)?, source);
    let method = method(name.trim_start_matches("::").strip_prefix("actix_web::")?)?;
    let args = attribute.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut tokens = args.children(&mut cursor).filter(|child| !child.is_extra());
    tokens.next()?; // Opening parenthesis.
    let literal = tokens.next()?;
    if !matches!(tokens.next()?.kind(), "," | ")") {
        return None;
    }
    Some(JoinPoint {
        symbol,
        kind: JoinKind::HttpEndpoint,
        value: http_endpoint(method, string_contents(literal, source)?)?,
        role: JoinRole::Declares,
        location: location(attribute),
    })
}

/// The routes an axum `.route("/path", get(a).post(b))` call declares. A
/// path that is not a literal, or a handler that is not a plain name, gives
/// none.
pub(super) fn axum_routes(call: Node, source: &str) -> Vec<Route> {
    let mut routes = Vec::new();
    let Some(function) = call.child_by_field_name("function") else {
        return routes;
    };
    let is_route = function.kind() == "field_expression"
        && function
            .child_by_field_name("field")
            .is_some_and(|field| text(field, source) == "route");
    let Some(arguments) = call.child_by_field_name("arguments").filter(|_| is_route) else {
        return routes;
    };
    let mut cursor = arguments.walk();
    let mut args = arguments.named_children(&mut cursor);
    let (Some(path), Some(chain), None) = (args.next(), args.next(), args.next()) else {
        return routes;
    };
    let Some(path) = string_contents(path, source) else {
        return routes;
    };
    let location = location(call);
    let mut node = Some(chain);
    while let Some(call) = node.filter(|n| n.kind() == "call_expression") {
        let Some(function) = call.child_by_field_name("function") else {
            break;
        };
        // `get(h)` or `routing::get(h)` ends the chain; `.post(h)` and
        // `.layer(..)` continue it through their receiver.
        let (name, next) = match function.kind() {
            "identifier" => (text(function, source), None),
            "scoped_identifier" => (
                function
                    .child_by_field_name("name")
                    .map_or("", |n| text(n, source)),
                None,
            ),
            "field_expression" => (
                function
                    .child_by_field_name("field")
                    .map_or("", |n| text(n, source)),
                function.child_by_field_name("value"),
            ),
            _ => break,
        };
        if let Some(method) = method(name) {
            if let Some(handler) = sole_identifier(call, source) {
                routes.push(Route {
                    method: method.to_string(),
                    path: path.to_string(),
                    handler: handler.to_string(),
                    location,
                });
            }
        }
        node = next;
    }
    routes
}

fn sole_identifier<'a>(call: Node, source: &'a str) -> Option<&'a str> {
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let mut args = arguments.named_children(&mut cursor);
    let (Some(handler), None) = (args.next(), args.next()) else {
        return None;
    };
    (handler.kind() == "identifier").then(|| text(handler, source))
}

/// Join points for the routes whose handler is exactly one free function of
/// this file.
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[mct_core::SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    mct_core::resolve_routes(routes, symbols, |s| {
        s.parent.is_none() && s.kind == mct_core::SymbolKind::Function
    })
}
