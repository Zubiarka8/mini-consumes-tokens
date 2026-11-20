//! HTTP join points (ADR-038): a Lapis `app:get("/x", handler)` route
//! declares an endpoint, and `ngx.location.capture("/x")` uses one. An
//! anonymous handler has no symbol, so it declares nothing.

use mct_core::{JoinPoint, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

/// The file names Lapis: the evidence `app:get(..)` is a route.
pub(super) fn mentions_lapis(source: &str) -> bool {
    source.contains("lapis")
}

pub(super) use mct_core::NamedRoute as Route;

fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string" {
        return None;
    }
    let raw = text(node, source);
    raw.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
}

fn arguments(call: Node) -> Vec<Node> {
    call.child_by_field_name("arguments")
        .map(|args| {
            let mut cursor = args.walk();
            args.named_children(&mut cursor).collect()
        })
        .unwrap_or_default()
}

/// `app:get("/x", handler)`.
pub(super) fn route_of(call: Node, callee: Node, source: &str) -> Option<Route> {
    if callee.kind() != "method_index_expression" {
        return None;
    }
    let method = text(callee.child_by_field_name("method")?, source).to_ascii_uppercase();
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return None;
    }
    let args = arguments(call);
    let [path, handler] = args.as_slice() else {
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

/// `ngx.location.capture("/x")`.
pub(super) fn use_of(call: Node, callee: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    if text(callee, source) != "ngx.location.capture" {
        return None;
    }
    let args = arguments(call);
    JoinPoint::http_uses(
        owner,
        "GET",
        literal(*args.first()?, source)?,
        location(call),
    )
}

/// Join points for the routes whose handler is exactly one function of this file.
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[mct_core::SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    mct_core::resolve_routes(routes, symbols, |s| s.kind == SymbolKind::Function)
}
