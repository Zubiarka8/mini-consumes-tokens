//! HTTP join points (ADR-038): Flask/Quart `@app.route`, FastAPI/Starlette
//! `@app.get` decorators declare an endpoint, and a `requests`, `httpx` or
//! test-client call with a literal path uses one.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

const FRAMEWORKS: [&str; 8] = [
    "flask",
    "Flask",
    "fastapi",
    "FastAPI",
    "quart",
    "Quart",
    "starlette",
    "Starlette",
];
const CLIENTS: [&str; 4] = ["requests", "httpx", "client", "session"];

/// The file names a web framework: the evidence a `.get(..)` decorator is a
/// route.
pub(super) fn mentions_framework(source: &str) -> bool {
    FRAMEWORKS.iter().any(|name| source.contains(name))
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

fn method(name: &str) -> Option<&'static str> {
    Some(match name {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "head" => "HEAD",
        "options" => "OPTIONS",
        _ => return None,
    })
}

/// The contents of a plain (or raw) string literal; an f-string, a bytes
/// literal or a concatenation gives `None`.
fn plain_string<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string" {
        return None;
    }
    let mut cursor = node.walk();
    let parts: Vec<Node> = node.named_children(&mut cursor).collect();
    let [start, content, end] = parts.as_slice() else {
        return None;
    };
    let prefix = text(*start, source);
    let plain = !prefix.contains(['f', 'F', 'b', 'B']);
    (start.kind() == "string_start"
        && content.kind() == "string_content"
        && end.kind() == "string_end"
        && plain)
        .then(|| text(*content, source))
}

fn arguments(call: Node) -> Vec<Node> {
    call.child_by_field_name("arguments")
        .map(|args| {
            let mut cursor = args.walk();
            args.named_children(&mut cursor).collect()
        })
        .unwrap_or_default()
}

/// The methods a Flask `methods=[..]` argument lists: `GET` when absent,
/// none when it is not a list of string literals.
fn route_methods(args: &[Node], source: &str) -> Vec<String> {
    let keyword = args.iter().find(|arg| {
        arg.kind() == "keyword_argument"
            && arg
                .child_by_field_name("name")
                .is_some_and(|name| text(name, source) == "methods")
    });
    let Some(keyword) = keyword else {
        return vec!["GET".to_string()];
    };
    let Some(list) = keyword
        .child_by_field_name("value")
        .filter(|v| matches!(v.kind(), "list" | "tuple" | "set"))
    else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    let methods: Option<Vec<String>> = list
        .named_children(&mut cursor)
        .map(|item| plain_string(item, source).map(str::to_ascii_uppercase))
        .collect();
    methods.unwrap_or_default()
}

/// The endpoints a decorator declares for the function it decorates.
pub(super) fn declarations(decorator: Node, source: &str, symbol: SymbolId) -> Vec<JoinPoint> {
    let Some(call) = decorator.named_child(0).filter(|n| n.kind() == "call") else {
        return Vec::new();
    };
    let Some(function) = call
        .child_by_field_name("function")
        .filter(|f| f.kind() == "attribute")
    else {
        return Vec::new();
    };
    let Some(name) = function.child_by_field_name("attribute") else {
        return Vec::new();
    };
    let args = arguments(call);
    let Some(path) = args.first().and_then(|arg| plain_string(*arg, source)) else {
        return Vec::new();
    };
    let methods = match text(name, source) {
        "route" => route_methods(&args, source),
        other => method(other).map(str::to_string).into_iter().collect(),
    };
    methods
        .iter()
        .filter_map(|m| JoinPoint::http_declares(symbol, m, path, location(decorator)))
        .collect()
}

/// `requests.get("/x")`, `self.client.post("/x")`: a client call with a literal
/// path, or `urlopen("http://localhost/x")`.
pub(super) fn use_of(
    call: Node,
    function: Node,
    owner: SymbolId,
    source: &str,
) -> Option<JoinPoint> {
    if function.kind() == "identifier" && text(function, source) == "urlopen" {
        let url = plain_string(*arguments(call).first()?, source)?;
        return JoinPoint::http_uses(
            owner,
            "GET",
            url.starts_with("http").then_some(url)?,
            location(call),
        );
    }
    if function.kind() != "attribute" {
        return None;
    }
    let object = function.child_by_field_name("object")?;
    // `requests.get(..)`, or `self.client.get(..)` of a test case.
    let client = match object.kind() {
        "identifier" => text(object, source),
        "attribute" if text(object.child_by_field_name("object")?, source) == "self" => {
            text(object.child_by_field_name("attribute")?, source)
        }
        _ => return None,
    };
    if !CLIENTS.contains(&client) {
        return None;
    }
    let method = method(text(function.child_by_field_name("attribute")?, source))?;
    let path = plain_string(*arguments(call).first()?, source)?;
    JoinPoint::http_uses(owner, method, path, location(call))
}
