//! HTTP join points (ADR-038): `HandleFunc("GET /x", h)` (Go 1.22) and the
//! gin, echo, fiber and chi `r.GET("/x", h)` routes declare an endpoint, and
//! `httptest.NewRequest("GET", "/x", nil)` uses one.

use mct_core::{JoinPoint, Location, SymbolId, SymbolKind, SymbolRecord};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

const FRAMEWORKS: [&str; 6] = [
    "net/http",
    "gin-gonic/gin",
    "labstack/echo",
    "gofiber/fiber",
    "go-chi/chi",
    "gorilla/mux",
];

/// The file imports a Go web package: the evidence a `.GET(..)` call is a route.
pub(super) fn mentions_framework(source: &str) -> bool {
    FRAMEWORKS.iter().any(|name| source.contains(name))
}

/// A route whose handler is still a name, resolved by [`resolve`].
pub(super) struct Route {
    method: String,
    path: String,
    handler: String,
    location: Location,
}

fn string_contents<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    let raw = text(node, source);
    match node.kind() {
        "interpreted_string_literal" => raw.strip_prefix('"')?.strip_suffix('"'),
        "raw_string_literal" => raw.strip_prefix('`')?.strip_suffix('`'),
        _ => None,
    }
}

fn arguments(call: Node) -> Vec<Node> {
    call.child_by_field_name("arguments")
        .map(|args| {
            let mut cursor = args.walk();
            args.named_children(&mut cursor).collect()
        })
        .unwrap_or_default()
}

fn method_name(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
        .contains(&upper.as_str())
        .then_some(upper)
}

/// `r.GET("/x", h)` and `mux.HandleFunc("GET /x", h)`.
pub(super) fn route_of(call: Node, function: Node, source: &str) -> Option<Route> {
    if function.kind() != "selector_expression" {
        return None;
    }
    let field = text(function.child_by_field_name("field")?, source);
    let args = arguments(call);
    let [path, .., handler] = args.as_slice() else {
        return None;
    };
    if handler.kind() != "identifier" {
        return None;
    }
    let path = string_contents(*path, source)?;
    let (method, path) = if matches!(field, "HandleFunc" | "Handle") {
        // A pattern without a method answers every method: no single value.
        let (method, path) = path.split_once(' ')?;
        (method_name(method)?, path.trim_start())
    } else {
        (method_name(field)?, path)
    };
    Some(Route {
        method,
        path: path.to_string(),
        handler: text(*handler, source).to_string(),
        location: location(call),
    })
}

/// `httptest.NewRequest("GET", "/x", nil)` or `http.NewRequest(http.MethodGet, "/x", nil)`.
pub(super) fn use_of(
    call: Node,
    function: Node,
    owner: SymbolId,
    source: &str,
) -> Option<JoinPoint> {
    if function.kind() != "selector_expression"
        || text(function.child_by_field_name("field")?, source) != "NewRequest"
    {
        return None;
    }
    let args = arguments(call);
    let (method, path) = (args.first()?, args.get(1)?);
    let method = match method.kind() {
        "selector_expression" => {
            let field = text(method.child_by_field_name("field")?, source);
            method_name(field.strip_prefix("Method")?)?
        }
        _ => method_name(string_contents(*method, source)?)?,
    };
    JoinPoint::http_uses(
        owner,
        &method,
        string_contents(*path, source)?,
        location(call),
    )
}

/// Join points for the routes whose handler is exactly one function of this
/// file (a Go function's parent is its package; a method is another kind).
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    routes.into_iter().filter_map(move |route| {
        let mut handlers = symbols
            .iter()
            .filter(|s| s.name == route.handler && s.kind == SymbolKind::Function);
        let (Some(handler), None) = (handlers.next(), handlers.next()) else {
            return None;
        };
        JoinPoint::http_declares(handler.id, &route.method, &route.path, route.location)
    })
}
