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

/// The literal `method = ".."` of an options table: `GET` without one, `None`
/// when the table sets it to something that is not a string literal.
fn table_method(table: Option<&Node>, source: &str) -> Option<String> {
    let Some(table) = table else {
        return Some("GET".to_string());
    };
    if table.kind() != "table_constructor" {
        return None;
    }
    let mut cursor = table.walk();
    let mut method = None;
    for field in table.named_children(&mut cursor) {
        let Some(name) = field.child_by_field_name("name") else {
            // A positional or computed entry cannot be told from `method`.
            return None;
        };
        if text(name, source) == "method" {
            let value = field.child_by_field_name("value")?;
            method = Some(literal(value, source)?.to_ascii_uppercase());
        }
    }
    Some(method.unwrap_or_else(|| "GET".to_string()))
}

/// `ngx.location.capture("/x")`, or lua-resty-http's
/// `httpc:request_uri("http://localhost/x", { method = "POST" })`.
pub(super) fn use_of(call: Node, callee: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    let args = arguments(call);
    if callee.kind() == "method_index_expression"
        && text(callee.child_by_field_name("method")?, source) == "request_uri"
    {
        let url = literal(*args.first()?, source).filter(|url| url.starts_with("http"))?;
        let method = table_method(args.get(1), source)?;
        return JoinPoint::http_uses(owner, &method, url, location(call));
    }
    if text(callee, source) != "ngx.location.capture" {
        return None;
    }
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
