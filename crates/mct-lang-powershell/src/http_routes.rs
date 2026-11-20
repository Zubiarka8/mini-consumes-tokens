//! HTTP join points (ADR-038): `Invoke-RestMethod -Uri http://localhost:5000/api/x
//! -Method Post` uses an endpoint. Only a literal URL to this machine counts.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

const SWITCHES: [&str; 4] = [
    "usebasicparsing",
    "skipcertificatecheck",
    "allowunencryptedauthentication",
    "passthru",
];

/// A word or quoted literal with no expansion in it.
fn plain(node: Node, source: &str) -> Option<String> {
    let raw = text(node, source).trim();
    let value = if let Some(inner) = raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        inner
    } else if let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        inner
    } else {
        raw
    };
    (!value.contains(['$', '`', '(']) && !value.is_empty()).then(|| value.to_string())
}

/// The endpoint an `Invoke-RestMethod` or `Invoke-WebRequest` call uses.
pub(super) fn request_use(command: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    let elements = command.child_by_field_name("command_elements")?;
    let mut cursor = elements.walk();
    let mut pending: Option<String> = None;
    let (mut uri, mut method, mut has_body) = (None, None, false);
    for element in elements.named_children(&mut cursor) {
        match element.kind() {
            "command_argument_sep" => {}
            "command_parameter" => {
                let raw = text(element, source).trim_start_matches('-');
                let (name, inline) = raw.split_once(':').unwrap_or((raw, ""));
                let name = name.to_ascii_lowercase();
                has_body |= name == "body";
                if !inline.is_empty() {
                    match name.as_str() {
                        "uri" => uri = Some(inline.trim_matches(['"', '\'']).to_string()),
                        "method" => method = Some(inline.to_string()),
                        _ => {}
                    }
                } else if !SWITCHES.contains(&name.as_str()) {
                    pending = Some(name);
                }
            }
            _ => {
                let value = plain(element, source);
                match pending.take().as_deref() {
                    Some("uri") => uri = Some(value?),
                    Some("method") => method = Some(value?),
                    Some(_) => {}
                    None if uri.is_none() => uri = value,
                    None => {}
                }
            }
        }
    }
    let method = match method {
        Some(method) => method,
        None if has_body => return None,
        None => "GET".to_string(),
    };
    JoinPoint::http_uses(owner, &method, &uri?, location(command))
}
