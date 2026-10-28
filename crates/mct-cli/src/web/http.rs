//! Bounded HTTP/1.1: GET/HEAD views and same-origin POST review decisions and `.mctignore` edits.
//! No request path is translated to a filesystem path.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::response::Response;
use super::{api, assets, ignore_file, Site};

const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 4096;
/// A `.mctignore` of `MAX_IGNORE_BYTES`, even if every byte is JSON-escaped.
const MAX_IGNORE_BODY: usize = 6 * ignore_file::MAX_IGNORE_BYTES + 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) trait Connection: Read + Write {
    fn read_timeout(&self, timeout: Duration) -> std::io::Result<()>;
    fn write_timeout(&self, timeout: Duration) -> std::io::Result<()>;
}

impl Connection for TcpStream {
    fn read_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
    fn write_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        self.set_write_timeout(Some(timeout))
    }
}

pub(super) fn serve_connection(
    stream: &mut impl Connection,
    port: u16,
    token: &str,
    site: &mut Site,
) -> std::io::Result<()> {
    stream.write_timeout(REQUEST_TIMEOUT)?;
    let (response, head) = receive(stream, port, token, site)?;
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        408 => "Request Timeout",
        413 => "Content Too Large",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    };
    write!(stream, "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nCross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; font-src 'self'; form-action 'none'; frame-ancestors 'none'; base-uri 'none'\r\nAllow: GET, HEAD, POST\r\n", response.status, response.content_type, response.body.len())?;
    stream.write_all(b"\r\n")?;
    if !head {
        stream.write_all(response.body.as_bytes())?;
    }
    stream.flush()
}

struct Request {
    target: String,
    post: bool,
    head: bool,
    length: usize,
}

fn receive(
    stream: &mut impl Connection,
    port: u16,
    token: &str,
    site: &mut Site,
) -> std::io::Result<(Response, bool)> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let mut bytes = Vec::new();
    let mut parsed: Option<(Request, usize)> = None;
    loop {
        let head = bytes.starts_with(b"HEAD ");
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok((Response::error(408), head));
        }
        stream.read_timeout(remaining)?;
        let mut buffer = [0; 1024];
        match stream.read(&mut buffer) {
            Ok(0) => return Ok((Response::error(400), head)),
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok((Response::error(408), head))
            }
            Err(error) => return Err(error),
        }
        let head = bytes.starts_with(b"HEAD ");
        if parsed.is_none() {
            if let Some(end) = bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|end| end + 4)
            {
                if end > MAX_HEADERS {
                    return Ok((Response::error(431), head));
                }
                let headers = match std::str::from_utf8(&bytes[..end]) {
                    Ok(value) => value,
                    Err(_) => return Ok((Response::error(400), head)),
                };
                let request = match validate(headers, port, token) {
                    Ok(request) => request,
                    Err(status) => return Ok((Response::error(status), head)),
                };
                parsed = Some((request, end));
            } else if bytes.len() > MAX_HEADERS {
                return Ok((Response::error(431), head));
            }
        }
        if let Some((request, end)) = &parsed {
            let expected = end + request.length;
            if bytes.len() > expected {
                return Ok((Response::error(400), request.head));
            }
            if bytes.len() == expected {
                let response = if request.post {
                    match std::str::from_utf8(&bytes[*end..]) {
                        Ok(body) if request.target == "api/ignore" => site.ignore(body),
                        Ok(body) => site.review(body),
                        Err(_) => Response::error(400),
                    }
                } else {
                    route(site, &request.target)
                };
                return Ok((response, request.head));
            }
        }
    }
}

fn validate(headers: &str, port: u16, token: &str) -> Result<Request, u16> {
    let (headers, body) = headers.split_once("\r\n\r\n").ok_or(400_u16)?;
    if !body.is_empty() {
        return Err(400);
    }
    let mut lines = headers.split("\r\n");
    let mut first = lines.next().ok_or(400_u16)?.split(' ');
    let method = first.next().ok_or(400_u16)?;
    let target = first.next().ok_or(400_u16)?;
    let version = first.next().ok_or(400_u16)?;
    if first.next().is_some()
        || version != "HTTP/1.1"
        || target.bytes().any(|byte| byte <= 32 || byte == 127)
    {
        return Err(400);
    }
    if !matches!(method, "GET" | "HEAD" | "POST") {
        return Err(405);
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(400_u16)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value
                .bytes()
                .any(|byte| byte < 32 && byte != b'\t' || byte == 127)
            || fields
                .insert(name.to_ascii_lowercase(), value.trim())
                .is_some()
        {
            return Err(400);
        }
    }
    let host = fields.get("host").ok_or(421_u16)?;
    if *host != format!("127.0.0.1:{port}") && *host != format!("localhost:{port}") {
        return Err(421);
    }
    if fields
        .get("origin")
        .is_some_and(|origin| *origin != format!("http://{host}"))
        || fields
            .get("sec-fetch-site")
            .is_some_and(|site| !matches!(*site, "same-origin" | "none"))
    {
        return Err(403);
    }
    if fields.contains_key("transfer-encoding") {
        return Err(400);
    }
    let target = target.strip_prefix(&format!("/{token}/")).ok_or(404_u16)?;
    let post = method == "POST";
    let length = if post {
        if !matches!(target, "api/review" | "api/ignore") {
            return Err(405);
        }
        if fields.get("origin").copied() != Some(format!("http://{host}").as_str()) {
            return Err(403);
        }
        if fields.get("content-type").copied() != Some("application/json") {
            return Err(400);
        }
        let length: usize = fields
            .get("content-length")
            .ok_or(400_u16)?
            .parse()
            .map_err(|_| 400_u16)?;
        let limit = if target == "api/ignore" {
            MAX_IGNORE_BODY
        } else {
            MAX_BODY
        };
        if length > limit {
            return Err(413);
        }
        length
    } else {
        if fields
            .get("content-length")
            .is_some_and(|length| *length != "0")
        {
            return Err(400);
        }
        0
    };
    Ok(Request {
        target: target.to_string(),
        post,
        head: method == "HEAD",
        length,
    })
}

/// Static dashboard files and the JSON API; nothing maps to a filesystem path.
fn route(site: &Site, target: &str) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut params = api::Params::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Response::error(400);
        }
    }
    if let Some(response) = assets::serve(path) {
        return response;
    }
    api::route(site, path, &params).unwrap_or_else(|| Response::error(404))
}

#[cfg(test)]
fn validated_target(request: &str, port: u16, token: &str) -> Result<String, u16> {
    validate(request, port, token).map(|request| request.target)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn capability_host_origin_and_body_rules_are_enforced() {
        let valid = "GET /secret/api/files?dir=src HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n\r\n";
        assert_eq!(
            validated_target(valid, 1234, "secret"),
            Ok("api/files?dir=src".to_string())
        );
        for (request, status) in [
            (valid.replace("/secret/", "/wrong/"), 404),
            (valid.replace("127.0.0.1:1234", "evil.example:1234"), 421),
            (
                valid.replace("\r\n\r\n", "\r\nOrigin: https://evil.example\r\n\r\n"),
                403,
            ),
            (
                valid.replace("\r\n\r\n", "\r\nSec-Fetch-Site: cross-site\r\n\r\n"),
                403,
            ),
            (valid.replace("GET", "POST"), 405),
            (
                valid.replace("\r\n\r\n", "\r\nHost: localhost:1234\r\n\r\n"),
                400,
            ),
            (
                valid.replace("\r\n\r\n", "\r\nContent-Length: 1\r\n\r\nx"),
                400,
            ),
            (
                valid.replace("\r\n\r\n", "\r\nTransfer-Encoding: chunked\r\n\r\n"),
                400,
            ),
            (valid.replace("Host:", " Host:"), 400),
            (valid.replace("HTTP/1.1", "HTTP/1.0"), 400),
        ] {
            assert_eq!(
                validated_target(&request, 1234, "secret"),
                Err(status),
                "{request}"
            );
        }
        assert!(validated_target(&valid.replace("GET", "HEAD"), 1234, "secret").is_ok());
    }
}
