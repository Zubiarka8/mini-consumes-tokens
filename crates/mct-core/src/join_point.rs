//! Join points: a literal value one side declares and another side uses
//! (ADR-038). Only a literal counts; a value built at run time is no join point.

use crate::{Location, SymbolId, SymbolRecord};

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

impl JoinPoint {
    /// An HTTP endpoint a route declares for `symbol`, or `None` when the
    /// method or path is not a literal the normalization accepts.
    pub fn http_declares(
        symbol: SymbolId,
        method: &str,
        path: &str,
        location: Location,
    ) -> Option<Self> {
        Some(Self {
            symbol,
            kind: JoinKind::HttpEndpoint,
            value: http_endpoint(method, path)?,
            role: JoinRole::Declares,
            location,
        })
    }

    /// An HTTP endpoint `symbol` calls with a literal path (a query string
    /// or fragment after it is dropped).
    pub fn http_uses(
        symbol: SymbolId,
        method: &str,
        path: &str,
        location: Location,
    ) -> Option<Self> {
        Some(Self {
            symbol,
            kind: JoinKind::HttpEndpoint,
            value: http_use_endpoint(method, path)?,
            role: JoinRole::Uses,
            location,
        })
    }
}

/// A route whose handler is still a name: the parser sees `app.get("/x",
/// handler)` before it knows the handler's symbol. [`resolve_routes`] turns
/// it into a join point once the file's symbols are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedRoute {
    pub method: String,
    pub path: String,
    pub handler: String,
    pub location: Location,
}

/// The `Declares` join points of the routes whose handler is exactly one
/// symbol of this file for which `is_handler` holds. A handler imported from
/// elsewhere, or defined twice, is skipped: a name alone does not prove which
/// function it is.
pub fn resolve_routes<'a>(
    routes: Vec<NamedRoute>,
    symbols: &'a [SymbolRecord],
    is_handler: impl Fn(&SymbolRecord) -> bool + 'a,
) -> impl Iterator<Item = JoinPoint> + 'a {
    routes.into_iter().filter_map(move |route| {
        let mut handlers = symbols
            .iter()
            .filter(|s| s.name == route.handler && is_handler(s));
        let (Some(handler), None) = (handlers.next(), handlers.next()) else {
            return None;
        };
        JoinPoint::http_declares(handler.id, &route.method, &route.path, route.location)
    })
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
/// after the path is dropped first (`/api/orders?limit=5`), and an absolute
/// URL to this machine (`http://localhost:3000/api/orders`) is its path. A URL
/// to any other host names a server this project does not declare: `None`.
pub fn http_use_endpoint(method: &str, path: &str) -> Option<String> {
    let path = own_host_path(path).unwrap_or(path);
    let end = path.find(['?', '#']).unwrap_or(path.len());
    http_endpoint(method, &path[..end])
}

/// The path of an absolute URL whose host is `localhost`, `127.0.0.1` or
/// `[::1]`, with or without a port.
fn own_host_path(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = match authority.strip_prefix('[') {
        Some(ipv6) => ipv6.split_once(']')?.0,
        None => authority.split(':').next()?,
    };
    ["localhost", "127.0.0.1", "::1"]
        .contains(&host)
        .then_some(if path.is_empty() { "/" } else { path })
}

fn segment_form(segment: &str) -> Option<String> {
    if segment.is_empty() {
        return None;
    }
    let parameter = if let Some(name) = segment.strip_prefix(':') {
        Some(name)
    } else if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        // `{id}`, `{id:int}` (ASP.NET, Django-style converters) and `{}`.
        Some(inner.split_once(':').map_or(inner, |(name, _)| name))
            .filter(|name| !name.is_empty() || segment == "{}")
    } else if let Some(inner) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(inner)
    } else {
        // Flask and Django: `<id>` and `<int:id>`.
        segment
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .map(|inner| inner.rsplit_once(':').map_or(inner, |(_, name)| name))
    };
    match parameter {
        Some("") if segment == "{}" => Some("{}".to_string()),
        Some(name) if !name.is_empty() && is_parameter_name(name) => Some("{}".to_string()),
        Some(_) => None,
        None if segment.contains(['{', '}', '[', ']', ':', '<', '>']) => None,
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
    fn a_use_of_this_machine_is_its_path_and_another_host_is_none() {
        for url in [
            "http://localhost:3000/api/orders",
            "https://127.0.0.1/api/orders?x=1",
            "http://[::1]:80/api/orders",
        ] {
            assert_eq!(
                http_use_endpoint("GET", url),
                Some("GET /api/orders".into()),
                "{url}"
            );
        }
        for url in [
            "https://example.com/api/orders",
            "http://localhost.evil.com/api/orders",
            "http://localhost@evil.com/api/orders",
        ] {
            assert_eq!(http_use_endpoint("GET", url), None, "{url}");
        }
        assert_eq!(
            http_use_endpoint("GET", "http://localhost"),
            Some("GET /".into())
        );
    }

    #[test]
    fn a_use_drops_its_query_and_fragment() {
        assert_eq!(
            http_use_endpoint("GET", "/api/orders?limit=5#top"),
            Some("GET /api/orders".into())
        );
    }
}
