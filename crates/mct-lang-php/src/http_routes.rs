//! HTTP join points (ADR-038): a Symfony `#[Route('/x', methods: ['GET'])]`
//! attribute declares an endpoint for its method (with the class's `#[Route]`
//! prefix), and a Laravel test request (`$this->getJson('/x')`) or a Guzzle
//! `$client->get('/x')` uses one. Laravel's `Route::get(..)` lives in a routes
//! file apart from its controller, so it declares nothing here.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::{named_children, text};

/// The file names Symfony routing or an HTTP client: the evidence the calls are web calls.
pub(super) fn mentions_web(source: &str) -> bool {
    ["Symfony", "Illuminate", "Guzzle", "TestCase"]
        .iter()
        .any(|name| source.contains(name))
}

fn literal<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    let argument = if node.kind() == "argument" {
        node.named_child(node.named_child_count().checked_sub(1)?)?
    } else {
        node
    };
    match argument.kind() {
        "string" | "encapsed_string" => {
            let raw = text(argument, source);
            let inner = raw.get(1..raw.len().checked_sub(1)?)?;
            (!inner.contains('$') || raw.starts_with('\'')).then_some(inner)
        }
        _ => None,
    }
}

/// The `#[Route(..)]` attributes of a declaration, as their argument nodes.
fn route_attributes<'a>(declaration: Node<'a>, source: &str) -> Vec<Vec<Node<'a>>> {
    let mut found = Vec::new();
    for list in named_children(declaration)
        .into_iter()
        .filter(|n| n.kind() == "attribute_list")
    {
        for group in named_children(list) {
            for attribute in named_children(group)
                .into_iter()
                .filter(|n| n.kind() == "attribute")
            {
                let name = attribute
                    .named_child(0)
                    .map_or("", |name| text(name, source));
                if name.trim_start_matches('\\').rsplit('\\').next() == Some("Route") {
                    let args = named_children(attribute)
                        .into_iter()
                        .find(|n| n.kind() == "arguments")
                        .map(named_children)
                        .unwrap_or_default();
                    found.push(args);
                }
            }
        }
    }
    found
}

/// A named argument's name (`methods: [..]`), when it has one.
fn argument_name<'a>(argument: Node, source: &'a str) -> Option<&'a str> {
    argument
        .child_by_field_name("name")
        .map(|name| text(name, source))
}

/// The path of a `#[Route]`: its first positional (or `path:`) literal.
fn route_path<'a>(args: &[Node], source: &'a str) -> Result<&'a str, ()> {
    let argument = args
        .iter()
        .find(|a| matches!(argument_name(**a, source), None | Some("path")))
        .ok_or(())?;
    literal(*argument, source).ok_or(())
}

/// The methods a `#[Route(methods: ['GET', 'POST'])]` lists; none when it has
/// no `methods` (it answers every method) or is not a list of literals.
fn route_methods(args: &[Node], source: &str) -> Vec<String> {
    let Some(argument) = args
        .iter()
        .find(|a| argument_name(**a, source) == Some("methods"))
    else {
        return Vec::new();
    };
    let Some(list) = argument
        .named_child(argument.named_child_count().saturating_sub(1))
        .filter(|n| n.kind() == "array_creation_expression")
    else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter_map(|element| literal(element.named_child(0)?, source).map(str::to_ascii_uppercase))
        .collect()
}

/// The endpoints a method's `#[Route]` attributes declare.
pub(super) fn declarations(method: Node, source: &str, symbol: SymbolId) -> Vec<JoinPoint> {
    let mut class = method.parent();
    while class.is_some_and(|node| node.kind() != "class_declaration") {
        class = class.and_then(|node| node.parent());
    }
    let prefix = match class.map(|class| route_attributes(class, source)) {
        None => "",
        Some(routes) if routes.is_empty() => "",
        Some(routes) => match routes.as_slice() {
            [args] => match route_path(args, source) {
                Ok(prefix) => prefix,
                Err(()) => return Vec::new(),
            },
            _ => return Vec::new(),
        },
    };
    let mut points = Vec::new();
    for args in route_attributes(method, source) {
        let Ok(path) = route_path(&args, source) else {
            continue;
        };
        let full = format!("{}{}", prefix.trim_end_matches('/'), path);
        for http_method in route_methods(&args, source) {
            points.extend(JoinPoint::http_declares(
                symbol,
                &http_method,
                &full,
                location(method),
            ));
        }
    }
    points
}

/// `$this->getJson('/x')` and `$client->post('/x')`: a request with a literal path.
pub(super) fn use_of(call: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    if call.kind() != "member_call_expression" {
        return None;
    }
    let name = text(call.child_by_field_name("name")?, source);
    let method = match name.strip_suffix("Json").unwrap_or(name) {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        _ => return None,
    };
    let args = named_children(call.child_by_field_name("arguments")?);
    JoinPoint::http_uses(
        owner,
        method,
        literal(*args.first()?, source)?,
        location(call),
    )
}
