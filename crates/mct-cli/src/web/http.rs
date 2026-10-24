//! Bounded HTTP/1.1: GET/HEAD views, same-origin form POSTs (review, reload,
//! language) and same-origin JSON POSTs under `api/` (CSRF token in a header).
//! No request path is translated to a filesystem path.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::i18n::Lang;
use super::render::Response;
use super::{render, Site};

const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 4096;
const MAX_JSON_BODY: usize = 8 * 1024;
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
        303 => "See Other",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    };
    write!(stream, "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nCross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'\r\nAllow: GET, HEAD, POST\r\n", response.status, response.content_type, response.body.len())?;
    if let Some(location) = &response.location {
        write!(stream, "Location: {location}\r\n")?;
    }
    if let Some(lang) = response.set_lang {
        write!(stream, "Set-Cookie: mct_lang={}; Path=/{token}/; Max-Age=31536000; SameSite=Strict; HttpOnly\r\n", lang.code())?;
    }
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
    lang: Lang,
    /// Value of `X-CSRF-Token`, required for JSON writes.
    csrf: Option<String>,
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
                        Ok(body) if request.target.starts_with("api/") => {
                            if request.csrf.as_deref() == Some(site.csrf.as_str()) {
                                super::api::write(site, &request.target, body)
                            } else {
                                Response::error(403)
                            }
                        }
                        Ok(body) => site.post(&request.target, body, request.lang),
                        Err(_) => Response::error(400),
                    }
                } else {
                    let route = request.target.split('?').next().unwrap_or_default();
                    if super::api::wants_fresh_snapshot(route) {
                        site.refresh_if_stale();
                    }
                    render::route(site, &request.target, request.lang)
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
    let json = target.starts_with("api/");
    let length = if post {
        if !(json && super::api::WRITE_ROUTES.contains(&target)
            || matches!(target, "review" | "review/delete" | "reload" | "lang"))
        {
            return Err(405);
        }
        if fields.get("origin").copied() != Some(format!("http://{host}").as_str()) {
            return Err(403);
        }
        let expected = if json {
            "application/json"
        } else {
            "application/x-www-form-urlencoded"
        };
        if fields.get("content-type").copied() != Some(expected) {
            return Err(400);
        }
        let length: usize = fields
            .get("content-length")
            .ok_or(400_u16)?
            .parse()
            .map_err(|_| 400_u16)?;
        if length > if json { MAX_JSON_BODY } else { MAX_BODY } {
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
    let lang = Lang::from_headers(
        fields.get("cookie").copied(),
        fields.get("accept-language").copied(),
    );
    Ok(Request {
        target: target.to_string(),
        lang,
        csrf: fields.get("x-csrf-token").map(|token| (*token).to_string()),
        post,
        head: method == "HEAD",
        length,
    })
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
        let valid = "GET /secret/atlas?dir=src HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n\r\n";
        assert_eq!(
            validated_target(valid, 1234, "secret"),
            Ok("atlas?dir=src".to_string())
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
