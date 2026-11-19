//! Explicit Actix Web route macros attach directly to their handler.

use mct_core::{JoinKind, JoinPoint, JoinRole, SymbolId};
use tree_sitter::Node;

use super::text;

pub(super) fn declaration(item: Node, source: &str, symbol: SymbolId) -> Option<JoinPoint> {
    let attribute = item.named_child(0)?;
    let name = text(attribute.named_child(0)?, source);
    let method = match name.trim_start_matches("::").strip_prefix("actix_web::")? {
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
    };
    let args = attribute.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut tokens = args.children(&mut cursor).filter(|child| !child.is_extra());
    tokens.next()?; // Opening parenthesis.
    let literal = tokens.next()?;
    if !matches!(tokens.next()?.kind(), "," | ")") {
        return None;
    }
    let raw = text(literal, source);
    let path = match literal.kind() {
        "string_literal" => raw.strip_prefix('"')?.strip_suffix('"')?,
        "raw_string_literal" => {
            let (prefix, contents) = raw.split_once('"')?;
            contents
                .strip_suffix(prefix.strip_prefix('r')?)?
                .strip_suffix('"')?
        }
        _ => return None,
    };
    if path.len() > 512
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['\\', '?', '#'])
        || path.chars().any(char::is_whitespace)
    {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    Some(JoinPoint {
        symbol,
        kind: JoinKind::HttpEndpoint,
        value: format!("{method} {path}"),
        role: JoinRole::Declares,
    })
}
