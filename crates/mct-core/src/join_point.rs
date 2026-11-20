//! Join points: a literal value one side declares and another side uses
//! (ADR-038). Only a literal counts; a value built at run time is no join point.

use crate::{Location, SymbolId};

/// A typed, normalized value tied to one symbol of the parsed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinPoint {
    pub symbol: SymbolId,
    pub kind: JoinKind,
    /// Normalized per kind, for example `GET /api/orders/{}`.
    pub value: String,
    pub role: JoinRole,
    /// Where the declaration or use is written.
    pub location: Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    HttpEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinRole {
    Declares,
    Uses,
}

const MAX_PATH_BYTES: usize = 512;
const METHODS: [&str; 9] = [
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT",
];

/// The normalized value of an HTTP endpoint: `"<METHOD> <path>"`.
///
/// `method` is matched case-insensitively. `path` must be an absolute literal
/// path: it starts with one `/`, has no query, fragment, backslash,
/// whitespace, wildcard or empty inner segment. A trailing `/` is dropped, and
/// a whole-segment parameter (`:id`, `{id}`, `[id]`) becomes `{}` so that
/// every framework's spelling gives the same value. Anything else gives `None`:
/// there is no guess.
pub fn http_endpoint(method: &str, path: &str) -> Option<String> {
    let method = METHODS
        .iter()
        .find(|known| known.eq_ignore_ascii_case(method))?;
    if path.len() > MAX_PATH_BYTES
        || !path.starts_with('/')
        || path.contains(['\\', '?', '#', '*'])
        || path.chars().any(char::is_whitespace)
    {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return Some(format!("{method} /"));
    }
    let mut value = format!("{method} ");
    for segment in trimmed.split('/').skip(1) {
        value.push('/');
        value.push_str(&segment_form(segment)?);
    }
    Some(value)
}

/// Like [`http_endpoint`] for a client's literal: a query string or fragment
/// after the path is dropped first (`/api/orders?limit=5`).
pub fn http_use_endpoint(method: &str, path: &str) -> Option<String> {
    let end = path.find(['?', '#']).unwrap_or(path.len());
    http_endpoint(method, &path[..end])
}

fn segment_form(segment: &str) -> Option<String> {
    if segment.is_empty() {
        return None;
    }
    let parameter = segment
        .strip_prefix(':')
        .or_else(|| segment.strip_prefix('{')?.strip_suffix('}'))
        .or_else(|| segment.strip_prefix('[')?.strip_suffix(']'));
    match parameter {
        Some("") if segment == "{}" => Some("{}".to_string()),
        Some(name) if is_parameter_name(name) && !name.is_empty() => Some("{}".to_string()),
        Some(_) => None,
        None if segment.contains(['{', '}', '[', ']', ':']) => None,
        None => Some(segment.to_string()),
    }
}

fn is_parameter_name(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{http_endpoint, http_use_endpoint};

    #[test]
    fn parameter_spellings_give_one_value() {
        let expected = Some("GET /api/orders/{}".to_string());
        for path in [
            "/api/orders/:id",
            "/api/orders/{id}",
            "/api/orders/[id]",
            "/api/orders/{}",
        ] {
            assert_eq!(http_endpoint("get", path), expected, "{path}");
        }
    }

    #[test]
    fn trailing_slash_and_root_are_normalized() {
        assert_eq!(
            http_endpoint("POST", "/api/notes/"),
            Some("POST /api/notes".into())
        );
        assert_eq!(http_endpoint("GET", "/"), Some("GET /".into()));
    }

    #[test]
    fn rejects_what_is_not_a_literal_absolute_path() {
        for path in [
            "api/orders",
            "//host/x",
            "/a//b",
            "/a?b=1",
            "/a#b",
            "/a b",
            "/a/*",
            "/a/x{id}",
            "/a/{id:\\d+}",
            "/a/:",
            "https://example.com/a",
        ] {
            assert_eq!(http_endpoint("GET", path), None, "{path}");
        }
        assert_eq!(http_endpoint("FETCH", "/a"), None);
    }

    #[test]
    fn a_use_drops_its_query_and_fragment() {
        assert_eq!(
            http_use_endpoint("GET", "/api/orders?limit=5#top"),
            Some("GET /api/orders".into())
        );
    }
}
