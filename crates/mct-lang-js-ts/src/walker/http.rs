//! HTTP join points (ADR-038): `fetch` and `axios` calls with a literal path
//! use an endpoint, and an Express `app.get("/path", handler)` declares one.
//! A path that is built at run time gives nothing.

use mct_core::{http_use_endpoint, JoinKind, JoinPoint, JoinRole, SymbolId, SymbolKind};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

pub(super) use mct_core::NamedRoute as Route;

const METHODS: [(&str, &str); 7] = [
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
];

fn method(name: &str) -> Option<&'static str> {
    METHODS
        .iter()
        .find_map(|(lower, upper)| (*lower == name).then_some(*upper))
}

/// The literal path of a string or template literal. A template literal may
/// end in one `${..}` that is a whole path segment (`/orders/${id}`): that
/// hole is written `{}`. Any other hole, or an escape sequence, gives `None`.
fn literal_path(node: Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let parts: Vec<Node> = node.named_children(&mut cursor).collect();
    match node.kind() {
        "string" => match parts.as_slice() {
            [fragment] if fragment.kind() == "string_fragment" => {
                Some(text(*fragment, source).to_string())
            }
            _ => None,
        },
        "template_string" => {
            let (hole, fragments) = match parts.split_last() {
                Some((last, rest)) if last.kind() == "template_substitution" => (true, rest),
                _ => (false, parts.as_slice()),
            };
            let mut path = String::new();
            for fragment in fragments {
                if fragment.kind() != "string_fragment" {
                    return None;
                }
                path.push_str(text(*fragment, source));
            }
            if hole {
                path.ends_with('/').then(|| path.push_str("{}"))?;
            }
            Some(path)
        }
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

/// The `method` of a `fetch` options object: `GET` without one, `None` when
/// it is not a literal or the object has a spread that could set it.
fn fetch_method(options: Option<&Node>, source: &str) -> Option<String> {
    let Some(options) = options else {
        return Some("GET".to_string());
    };
    if options.kind() != "object" {
        return None;
    }
    let mut cursor = options.walk();
    let mut found = None;
    for pair in options.named_children(&mut cursor) {
        match pair.kind() {
            "pair" => {
                let key = pair.child_by_field_name("key")?;
                let key = match key.kind() {
                    "property_identifier" => text(key, source).to_string(),
                    "string" => literal_path(key, source)?,
                    _ => return None,
                };
                if key == "method" {
                    let value = pair.child_by_field_name("value")?;
                    found = Some(literal_path(value, source)?.to_ascii_uppercase());
                }
            }
            "comment" => {}
            // A spread, shorthand or computed key might set the method.
            _ => return None,
        }
    }
    Some(found.unwrap_or_else(|| "GET".to_string()))
}

/// The endpoint a `fetch(..)` or `axios.<method>(..)` call uses.
pub(super) fn use_of(
    call: Node,
    function: Node,
    owner: SymbolId,
    source: &str,
) -> Option<JoinPoint> {
    let args = arguments(call);
    let (method, path) = match function.kind() {
        "identifier" if text(function, source) == "fetch" => (
            fetch_method(args.get(1), source)?,
            literal_path(*args.first()?, source)?,
        ),
        "member_expression" => {
            let object = function.child_by_field_name("object")?;
            let property = function.child_by_field_name("property")?;
            if object.kind() != "identifier" || text(object, source) != "axios" {
                return None;
            }
            (
                method(text(property, source))?.to_string(),
                literal_path(*args.first()?, source)?,
            )
        }
        _ => return None,
    };
    Some(JoinPoint {
        symbol: owner,
        kind: JoinKind::HttpEndpoint,
        value: http_use_endpoint(&method, &path)?,
        role: JoinRole::Uses,
        location: location(call),
    })
}

/// An Express-style `app.get("/path", ..., handler)`: the last argument is
/// the handler, and only a plain name can be tied to a symbol.
pub(super) fn route_of(call: Node, function: Node, source: &str) -> Option<Route> {
    if function.kind() != "member_expression" {
        return None;
    }
    let method = method(text(function.child_by_field_name("property")?, source))?;
    let args = arguments(call);
    let [path, .., handler] = args.as_slice() else {
        return None;
    };
    (handler.kind() == "identifier").then_some(())?;
    Some(Route {
        method: method.to_string(),
        path: literal_path(*path, source)?,
        handler: text(*handler, source).to_string(),
        location: location(call),
    })
}

/// Join points for the routes whose handler is exactly one function of this
/// file.
pub(super) fn resolve(
    routes: Vec<Route>,
    symbols: &[mct_core::SymbolRecord],
) -> impl Iterator<Item = JoinPoint> + '_ {
    mct_core::resolve_routes(routes, symbols, |s| {
        s.parent.is_none() && s.kind == SymbolKind::Function
    })
}
