//! HTTP join points (ADR-038): `curl` or `wget http://localhost:3000/api/x` uses an
//! endpoint. Only a literal URL to this machine counts, so a script that
//! calls another host links to nothing.

use mct_core::{JoinPoint, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::text;

/// A word or quoted literal with no expansion in it.
fn plain<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    let raw = text(node, source);
    let value = match node.kind() {
        "word" => raw,
        "raw_string" => raw.strip_prefix('\'')?.strip_suffix('\'')?,
        "string" => raw.strip_prefix('"')?.strip_suffix('"')?,
        _ => return None,
    };
    (!value.contains(['$', '`'])).then_some(value)
}

/// The arguments of a command: `None` for one with an expansion in it.
fn literal_args<'a>(command: Node, source: &'a str) -> Vec<Option<&'a str>> {
    let mut cursor = command.walk();
    command
        .named_children(&mut cursor)
        .filter(|n| n.kind() != "command_name" && n.kind() != "file_redirect")
        .map(|n| plain(n, source))
        .collect()
}

/// The endpoint a `curl` command calls: its first literal URL, with the
/// method from `-X`/`--request`, `POST` when it sends data, else `GET`.
pub(super) fn curl_use(command: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    let args = literal_args(command, source);
    let mut method = None;
    let mut sends_data = false;
    let mut url = None;
    let mut tokens = args.iter().copied();
    while let Some(token) = tokens.next() {
        let Some(token) = token else {
            // An expanded word could be anything, including the URL or method.
            url?;
            continue;
        };
        match token {
            "-X" | "--request" => method = Some(tokens.next()??.to_string()),
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-urlencode" | "-F"
            | "--form" | "-T" | "--upload-file" => {
                sends_data = true;
                tokens.next();
            }
            _ if token.starts_with("-X") => method = Some(token[2..].to_string()),
            _ if token.starts_with("http://") || token.starts_with("https://") => {
                url.get_or_insert(token);
            }
            _ => {}
        }
    }
    let method = method.unwrap_or_else(|| if sends_data { "POST" } else { "GET" }.to_string());
    JoinPoint::http_uses(owner, &method, url?, location(command))
}

/// The endpoint a `wget` command calls: its first literal URL, with the
/// method from `--method=`, `POST` for `--post-data`/`--post-file`, else `GET`.
pub(super) fn wget_use(command: Node, owner: SymbolId, source: &str) -> Option<JoinPoint> {
    let mut method = None;
    let mut url = None;
    for token in literal_args(command, source) {
        let Some(token) = token else {
            // An expanded word could be anything, including the URL or method.
            url?;
            continue;
        };
        if let Some(name) = token.strip_prefix("--method=") {
            method = Some(name);
        } else if token.starts_with("--post-data") || token.starts_with("--post-file") {
            method.get_or_insert("POST");
        } else if token.starts_with("http://") || token.starts_with("https://") {
            url.get_or_insert(token);
        }
    }
    JoinPoint::http_uses(owner, method.unwrap_or("GET"), url?, location(command))
}
