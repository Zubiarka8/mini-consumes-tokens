//! Public index APIs plus full HTTP wire parsing/response tests, without sockets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::reviews::{self, Kind, State, Status};
use super::{http, store, Site};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::Duration;

struct Wire {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    timeout: bool,
}
impl Read for Wire {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.timeout {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let size = buffer.len().min(37);
        self.input.read(&mut buffer[..size])
    }
}
impl Write for Wire {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.output.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl http::Connection for Wire {
    fn read_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        assert!(timeout <= Duration::from_secs(2));
        Ok(())
    }
    fn write_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        assert_eq!(timeout, Duration::from_secs(2));
        Ok(())
    }
}
struct Server {
    site: std::cell::RefCell<Site>,
    address: &'static str,
    prefix: &'static str,
}
impl Server {
    fn start(root: &Path, no_obsidian: bool) -> Self {
        let root = root.canonicalize().unwrap();
        Self {
            site: std::cell::RefCell::new(Site::load(&root, no_obsidian).unwrap()),
            address: "127.0.0.1:8787",
            prefix: "/secret/",
        }
    }
    fn raw(&self, request: &str) -> String {
        let mut wire = Wire {
            input: Cursor::new(request.as_bytes().to_vec()),
            output: Vec::new(),
            timeout: false,
        };
        http::serve_connection(&mut wire, 8787, "secret", &mut self.site.borrow_mut()).unwrap();
        String::from_utf8(wire.output).unwrap()
    }
    fn get(&self, path: &str) -> String {
        self.raw(&format!(
            "GET {}{path} HTTP/1.1\r\nHost: {}\r\n\r\n",
            self.prefix, self.address
        ))
    }
    fn form(&self, route: &str, body: &str, headers: &str) -> String {
        self.raw(&format!("POST /secret/{route} HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }
    fn json_post(&self, route: &str, body: &str, headers: &str) -> String {
        self.raw(&format!("POST /secret/{route} HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }
    /// A JSON write with the same-origin header and the CSRF token.
    fn write(&self, route: &str, body: &serde_json::Value) -> String {
        let csrf = self.site.borrow().csrf.clone();
        let mut body = body.clone();
        if let Some(fields) = body.as_object_mut() {
            let site = self.site.borrow();
            let identity = fields
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if matches!(route, "api/review" | "api/review/delete") {
                if let Some(kind) = fields
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .and_then(Kind::parse)
                {
                    fields.entry("expected_revision").or_insert_with(|| {
                        serde_json::json!(site
                            .reviews
                            .decision_revision(kind, &identity)
                            .unwrap_or(0))
                    });
                    if route == "api/review" {
                        fields.entry("fingerprint").or_insert_with(|| {
                            serde_json::json!(site
                                .fingerprint_of(kind, &identity)
                                .unwrap_or_default())
                        });
                    }
                }
            } else if route == "api/file-review" {
                let path = fields
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                fields.entry("expected_revision").or_insert_with(|| {
                    serde_json::json!(site.reviews.file_revision(&identity, &path).unwrap_or(0))
                });
            }
        }
        self.json_post(
            route,
            &body.to_string(),
            &format!("Origin: http://127.0.0.1:8787\r\nX-CSRF-Token: {csrf}\r\n"),
        )
    }
    fn review_body(&self, kind: &str, id: &str, action: &str, extra: &[(&str, &str)]) -> String {
        let csrf = self.site.borrow().csrf.clone();
        let site = self.site.borrow();
        let parsed_kind = Kind::parse(kind);
        let fingerprint = parsed_kind
            .and_then(|k| site.fingerprint_of(k, id))
            .unwrap_or_default();
        let revision = parsed_kind
            .and_then(|k| site.reviews.decision_revision(k, id).ok())
            .unwrap_or(0)
            .to_string();
        let mut pairs = vec![
            ("fingerprint", fingerprint.as_str()),
            ("expected_revision", revision.as_str()),
            ("csrf", csrf.as_str()),
            ("kind", kind),
            ("id", id),
            ("action", action),
        ];
        pairs.extend_from_slice(extra);
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish()
    }
    fn review(&self, kind: &str, id: &str, action: &str, reason: &str) -> String {
        let extra = [("reason", reason)];
        let extra: &[(&str, &str)] = if reason.is_empty() { &[] } else { &extra };
        self.form(
            "review",
            &self.review_body(kind, id, action, extra),
            "Origin: http://127.0.0.1:8787\r\n",
        )
    }
    fn duplicate_id(&self, index: usize) -> String {
        self.site.borrow().duplicate_keys[index].identity.clone()
    }
    fn dead_id(&self, name: &str) -> String {
        let site = self.site.borrow();
        site.dead
            .iter()
            .find(|dead| dead.entry.name == name)
            .unwrap()
            .identity
            .clone()
    }
}
fn index(root: &Path) {
    let root = root.canonicalize().unwrap();
    let mut index = mct_index::Index::open(
        &root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let report = index
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/a.rs"), format!("pub fn alpha{BODY}")).unwrap();
    fs::write(root.path().join("src/b.rs"), format!("pub fn beta{BODY}")).unwrap();
    index(root.path());
    root
}

fn open_store(root: &Path) -> store::Store {
    store::Store::open(&root.canonicalize().unwrap()).unwrap()
}

/// Decisions persisted in the dashboard database, newest first.
fn saved_decisions(root: &Path) -> Vec<reviews::Record> {
    open_store(root).decisions().unwrap()
}

/// Makes every new decision fail to save, as a full disk or a locked file would.
fn block_review_writes(root: &Path) {
    rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER block_reviews BEFORE INSERT ON reviews \
             BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .unwrap();
}

fn unblock_review_writes(root: &Path) {
    rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3"))
        .unwrap()
        .execute_batch("DROP TRIGGER block_reviews;")
        .unwrap();
}

#[test]
fn the_default_page_is_duplicates_and_the_header_has_the_four_sections_in_order() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    let landing = server.get("");
    assert!(
        landing.starts_with("HTTP/1.1 303") && landing.contains("Location: duplicados\r\n"),
        "{landing}"
    );
    let page = server.get("duplicados");
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<html lang=\"es\">") && page.contains("<h1>Código duplicado</h1>"));
    let at = |label: &str| page.find(&format!(">{label}")).unwrap_or(usize::MAX);
    let order = [
        at("Código duplicado"),
        at("Código muerto"),
        at("Obsidian"),
        at("Revisiones"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    assert!(page.contains("aria-current=\"page\">Código duplicado"));
    assert!(page.contains("href=\"estado\"") && page.contains("href=\"ajustes\""));
    assert!(page.contains("Cache-Control: no-store"));
    assert!(page.contains("script-src 'self'") && !page.contains("unsafe-"));
    assert!(server.get("style.css").contains("text/css"));
    assert!(server.get("app.js").contains("text/javascript"));
    assert_eq!(original, fs::read(database).unwrap());
    assert!(!root.path().join(".mct-index/obsidian").exists());
    assert!(saved_decisions(root.path()).is_empty());
}

#[test]
fn legacy_routes_redirect_to_the_new_pages() {
    let root = project();
    let server = Server::start(root.path(), true);
    for (old, new) in [
        ("duplicates", "duplicados"),
        ("group?id=0", "duplicados"),
        ("atlas?dir=src", "explorar?dir=src"),
        ("file?path=src%2Fa.rs", "archivo?path=src%2Fa.rs"),
    ] {
        let response = server.get(old);
        assert!(
            response.starts_with("HTTP/1.1 303")
                && response.contains(&format!("Location: {new}\r\n")),
            "{old}: {response}"
        );
    }
}

#[test]
fn every_page_renders_and_every_link_resolves() {
    let root = project();
    let server = Server::start(root.path(), false);
    let mut pending = vec![String::from("duplicados")];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(target) = pending.pop() {
        if !visited.insert(target.clone()) {
            continue;
        }
        assert!(visited.len() < 200, "unexpected navigation expansion");
        let response = server.get(&target);
        assert!(response.starts_with("HTTP/1.1 200"), "{target}: {response}");
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(length, body.len(), "{target}");
        if !headers.contains("text/html") {
            continue;
        }
        assert!(
            !body.contains("<script>") && !body.contains("javascript:"),
            "{target}"
        );
        for value in body.split("href=\"").skip(1) {
            let href = value.split('"').next().unwrap().replace("&amp;", "&");
            if href.starts_with('#') {
                continue;
            }
            assert!(!href.contains("://"), "no external resources: {href}");
            pending.push(href);
        }
    }
    for needle in [
        "duplicado?k=",
        "muerto",
        "obsidian",
        "explorar",
        "archivo?path=src%2Fa.rs",
        "notas",
        "nota?n=",
        "grafo",
        "revisiones",
        "estado",
        "ajustes",
        "simbolo?k=",
    ] {
        assert!(
            visited.iter().any(|t| t.starts_with(needle)),
            "{needle} in {visited:?}"
        );
    }
}

#[test]
fn http_rejects_cross_site_requests_traversal_and_unmanaged_files() {
    let root = project();
    let server = Server::start(root.path(), true);
    for path in [
        "../index.sqlite3",
        "%2e%2e/index.sqlite3",
        "obsidian/.obsidian/config",
        ".mct-index/index.sqlite3",
        "archivo?path=..%2Fsecret",
        "archivo?path=%2Fetc%2Fpasswd",
        "archivo?path=src%2F..%2Fa.rs",
        "archivo?path=%3Cscript%3E",
        "nota?n=..%2F..%2Fsecret",
        "nota?n=index.md",
        "duplicado?k=missing",
        "simbolo?k=missing",
        "missing",
    ] {
        assert!(server.get(path).starts_with("HTTP/1.1 404"), "{path}");
    }
    for path in [
        "duplicados?page=oops",
        "duplicados?page=1&page=2",
        "duplicados?status=weird",
        "duplicados?verdict=weird",
        "muerto?status=weird",
        "revisiones?kind=weird",
        "explorar?kind=weird",
        "notas?kind=weird",
    ] {
        assert!(server.get(path).starts_with("HTTP/1.1 400"), "{path}");
    }
    let injected = server.get("duplicados?q=%22%3E%3Cscript%3Ealert%281%29%3C%2Fscript%3E");
    assert!(!injected.contains("<script>alert"));
    assert!(injected.contains("&lt;script&gt;"));
    for (headers, status) in [("Host: evil.example", 421), ("Host: 127.0.0.1:1", 421)] {
        assert!(server
            .raw(&format!(
                "GET {} HTTP/1.1\r\n{headers}\r\n\r\n",
                server.prefix
            ))
            .starts_with(&format!("HTTP/1.1 {status}")));
    }
    let host = format!("Host: {}", server.address);
    for header in ["Origin: https://example.com", "Sec-Fetch-Site: cross-site"] {
        assert!(server
            .raw(&format!(
                "GET {} HTTP/1.1\r\n{host}\r\n{header}\r\n\r\n",
                server.prefix
            ))
            .starts_with("HTTP/1.1 403"));
    }
    assert!(server
        .raw(&format!("GET / HTTP/1.1\r\n{host}\r\n\r\n"))
        .starts_with("HTTP/1.1 404"));
    assert!(server
        .raw(&format!(
            "POST {} HTTP/1.1\r\n{host}\r\n\r\n",
            server.prefix
        ))
        .starts_with("HTTP/1.1 405"));
    let head = server.raw(&format!(
        "HEAD {} HTTP/1.1\r\n{host}\r\n\r\n",
        server.prefix
    ));
    assert!(head.starts_with("HTTP/1.1 303") && head.ends_with("\r\n\r\n"));
    let huge = "a".repeat(17_000);
    assert!(server
        .raw(&format!(
            "GET {} HTTP/1.1\r\n{host}\r\nX-Large: {huge}\r\n\r\n",
            server.prefix
        ))
        .starts_with("HTTP/1.1 431"));
}

#[test]
fn incomplete_headers_and_timeouts_are_bounded_without_rendering_content() {
    let root = project();
    let server = Server::start(root.path(), true);
    assert!(server
        .raw("GET /secret/ HTTP/1.1\r\n")
        .starts_with("HTTP/1.1 400"));
    let mut wire = Wire {
        input: Cursor::new(Vec::new()),
        output: Vec::new(),
        timeout: true,
    };
    http::serve_connection(&mut wire, 8787, "secret", &mut server.site.borrow_mut()).unwrap();
    let response = String::from_utf8(wire.output).unwrap();
    assert!(response.starts_with("HTTP/1.1 408"));
    assert!(!response.contains("alpha"));
}

#[test]
fn statistics_match_the_fixture_and_every_figure_links_to_its_list() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("pkg/inner")).unwrap();
    fs::write(root.path().join("pkg/inner/m.py"), "class Box:\n    def open(self):\n        pass\n\n    def close(self):\n        pass\n\ndef make():\n    return Box()\n").unwrap();
    fs::write(
        root.path().join("pkg/util.py"),
        "def helper():\n    pass\n\ndef other():\n    pass\n",
    )
    .unwrap();
    fs::write(root.path().join("top.rs"), "pub struct Thing;\npub enum Mode { A }\npub trait Run { fn run(&self); }\npub fn go() {}\n").unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    {
        let site = server.site.borrow();
        let stats = &site.stats;
        assert_eq!((stats.files, stats.directories), (3, 2));
        let python = &stats.languages["python"];
        assert_eq!(python.files, 2);
        use super::stats::Bucket;
        assert_eq!(python.count(Bucket::Class), 1);
        assert_eq!(python.count(Bucket::Method), 2);
        assert_eq!(python.count(Bucket::Function), 3);
        let rust = &stats.languages["rust"];
        assert_eq!(rust.count(Bucket::Function), 1);
        assert_eq!(rust.count(Bucket::Type), 3);
    }
    let dashboard = server.get("obsidian");
    assert!(dashboard.starts_with("HTTP/1.1 200"), "{dashboard}");
    assert!(
        dashboard.contains("explorar?lang=python&amp;kind=function\">3</a>"),
        "{dashboard}"
    );
    assert!(dashboard.contains("explorar?lang=rust&amp;kind=type\">3</a>"));
    assert!(dashboard.contains("explorar?lang=python\">2</a>"));
    assert!(dashboard.contains("Cómo se cuenta"));
    let functions = server.get("explorar?lang=python&kind=function");
    for name in ["helper", "other", "make"] {
        assert!(
            functions.contains(&format!("<code>{name}</code>")),
            "{functions}"
        );
    }
    assert!(!functions.contains("<code>open</code>"));
    assert!(server
        .get("explorar?kind=class")
        .contains("<code>Box</code>"));
    assert!(server
        .get("explorar?kind=method&q=clo")
        .contains("<code>close</code>"));
    assert!(!server
        .get("explorar?kind=method&q=clo")
        .contains("<code>open</code>"));
    let directory = server.get("explorar?dir=pkg");
    assert!(
        directory.contains("archivo?path=pkg%2Futil.py")
            && directory.contains("explorar?dir=pkg%2Finner")
    );
    assert!(!server.get("explorar?dir=pkg&lang=rust").contains("util.py"));
    let file = server.get("archivo?path=pkg%2Finner%2Fm.py&from=2&to=3");
    assert!(
        file.contains("<code>Box</code>") && file.contains("class=\"ln hl\" id=\"L2\""),
        "{file}"
    );
    assert!(file.contains("def open(self)"));
}

#[test]
fn file_pages_page_through_code_and_symbols_and_escape_names() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let filename = "src/a & ü.rs";
    let source: String = (0..205)
        .map(|id| format!("pub fn symbol_{id}() {{}}\n"))
        .collect();
    fs::write(root.path().join(filename), source).unwrap();
    for id in 0..101 {
        fs::write(
            root.path().join(format!("src/file_{id:03}.rs")),
            "pub fn tiny() {}\n",
        )
        .unwrap();
    }
    index(root.path());
    let server = Server::start(root.path(), true);
    let first = server.get("explorar?dir=src");
    assert!(first.contains("Página 1 de 2") && first.contains("102 elementos"));
    assert!(first.contains("archivo?path=src%2Fa+%26+%C3%BC.rs"));
    assert!(server
        .get("explorar?dir=src&page=1")
        .contains("file_100.rs"));
    let page = server.get("archivo?path=src%2Fa+%26+%C3%BC.rs");
    assert!(page.contains("src/a &amp; ü.rs") && page.contains("<code>symbol_0</code>"));
    assert!(!page.contains("<code>symbol_204</code>") || page.contains("Página 1 de 3"));
    let last = server.get("archivo?path=src%2Fa+%26+%C3%BC.rs&sp=2");
    assert!(last.contains("<code>symbol_204</code>"), "{last}");
    let later = server.get("archivo?path=src%2Fa+%26+%C3%BC.rs&from=150");
    assert!(
        later.contains("id=\"L150\"") && later.contains("Tramo anterior"),
        "{later}"
    );
}

#[test]
fn file_relations_link_in_both_directions_and_the_snapshot_refreshes_only_on_request() {
    let root = project();
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("use crate::b::beta;\n{original}")).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    assert!(server
        .get("archivo?path=src%2Fa.rs")
        .contains("<li><a href=\"archivo?path=src%2Fb.rs\""));
    fs::write(&file, format!("{original}\npub fn newly_indexed() {{}}\n")).unwrap();
    index(root.path());
    assert!(!server
        .get("archivo?path=src%2Fa.rs")
        .contains("<code>newly_indexed</code>"));
    assert!(server
        .get("estado")
        .contains("El índice se modificó después de esta instantánea"));
    let csrf = server.site.borrow().csrf.clone();
    let body = format!("csrf={csrf}");
    assert!(server.form("reload", &body, "").starts_with("HTTP/1.1 403"));
    assert!(server
        .form("reload", "csrf=bad", "Origin: http://127.0.0.1:8787\r\n")
        .starts_with("HTTP/1.1 403"));
    assert!(server
        .form(
            "reload",
            &format!("{body}&x=1"),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 400"));
    let reloaded = server.form("reload", &body, "Origin: http://127.0.0.1:8787\r\n");
    assert!(
        reloaded.starts_with("HTTP/1.1 303") && reloaded.contains("Location: estado\r\n"),
        "{reloaded}"
    );
    assert!(server
        .get("archivo?path=src%2Fa.rs")
        .contains("<code>newly_indexed</code>"));
    assert_eq!(
        server.site.borrow().csrf,
        csrf,
        "the page tokens stay valid after a reload"
    );
}

fn fingerprint_like_adr_015(site: &Site, index: usize) -> String {
    // Independent copy of the ADR-015 key so that saved decisions keep matching.
    let group = &site.duplicates.groups[index];
    let hashes: std::collections::BTreeMap<_, _> = site
        .navigation
        .files
        .iter()
        .map(|f| (f.relative_path.as_str(), f.content_hash.as_str()))
        .collect();
    let mut members: Vec<_> = group
        .members
        .iter()
        .map(|m| {
            (
                &m.path,
                &m.symbol_name,
                &m.symbol_kind,
                m.start_line,
                m.end_line,
                hashes[m.path.as_str()],
            )
        })
        .collect();
    members.sort();
    let mut bytes = Vec::new();
    let mut field = |value: &str| {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    };
    field("duplicate-review-v1");
    field(site.duplicates.options.normalization.as_str());
    field(&group.language);
    for (path, name, kind, start, end, hash) in members {
        for value in [
            path.as_str(),
            name.as_str(),
            kind.as_str(),
            &start.to_string(),
            &end.to_string(),
            hash,
        ] {
            field(value);
        }
    }
    hmac_sha256::Hash::hash(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn duplicate_decisions_persist_restore_and_never_touch_the_index() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let raw = server.get("duplicados.toon");
    assert!(server.get("duplicados").contains("Falso positivo"));
    let response = server.review(
        "duplicate",
        &id,
        "false_positive",
        "es un patrón de plantilla",
    );
    assert!(
        response.starts_with("HTTP/1.1 303") && response.contains("Location: duplicados\r\n"),
        "{response}"
    );
    let pending = server.get("duplicados");
    assert!(
        !pending.contains(&format!("duplicado?k={id}"))
            && pending.contains("Pendientes <span class=\"count\">0</span>")
    );
    let listed = server.get("duplicados?status=false_positive");
    assert!(
        listed.contains(&format!("duplicado?k={id}")) && listed.contains("Restaurar a pendiente")
    );
    assert!(listed.contains("es un patrón de plantilla"));
    let detail = server.get(&format!("duplicado?k={id}"));
    assert!(detail.contains("Historial de decisiones") && detail.contains("Falso positivo"));
    assert_eq!(server.get("duplicados.toon"), raw);
    // The three kinds of "not pending" are different states, never merged.
    let central = server.get("revisiones");
    assert!(central.contains("Falso positivo") && !central.contains("Ignorado a propósito</span>"));

    let restarted = Server::start(root.path(), true);
    assert!(!restarted
        .get("duplicados")
        .contains(&format!("duplicado?k={id}")));
    assert!(restarted
        .get("archivo?path=src%2Fa.rs")
        .contains("0 pendientes"));
    let response = restarted.review("duplicate", &id, "restore", "");
    assert!(response.starts_with("HTTP/1.1 303"));
    assert!(restarted
        .get("duplicados")
        .contains(&format!("duplicado?k={id}")));
    assert!(Server::start(root.path(), true)
        .get("duplicados")
        .contains(&format!("duplicado?k={id}")));
    let history = open_store(root.path());
    assert_eq!(
        history.history(Kind::Duplicate, &id).unwrap().len(),
        2,
        "restore keeps the earlier decision"
    );
    assert_eq!(fs::read(database).unwrap(), original);
}

#[test]
fn a_changed_duplicate_body_asks_for_reevaluation_instead_of_hiding_the_decision() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    assert!(server
        .review("duplicate", &id, "ignored", "api pública")
        .starts_with("HTTP/1.1 303"));
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("total += 8", "total += 9"),
    )
    .unwrap();
    index(root.path());
    let changed = Server::start(root.path(), true);
    assert_eq!(
        changed.duplicate_id(0),
        id,
        "identity survives edits and reindexing"
    );
    assert!(matches!(
        changed.site.borrow().duplicate_state(0),
        State::NeedsReevaluation {
            status: Status::Ignored,
            ..
        }
    ));
    let pending = changed.get("duplicados");
    assert!(
        !pending.contains(&format!("duplicado?k={id}")),
        "not in the pending queue"
    );
    let reevaluate = changed.get("duplicados?status=reevaluate");
    assert!(
        reevaluate.contains(&format!("duplicado?k={id}"))
            && reevaluate.contains("El código cambió desde «Ignorado a propósito»"),
        "{reevaluate}"
    );
    assert!(changed
        .get("revisiones?status=reevaluate")
        .contains(&format!("duplicado?k={id}")));
    assert!(changed.get("estado").contains("Para reevaluar</a>: 1"));
    // Reverting the file makes the old fingerprint match again.
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("total += 9", "total += 8"),
    )
    .unwrap();
    index(root.path());
    assert!(matches!(
        Server::start(root.path(), true)
            .site
            .borrow()
            .duplicate_state(0),
        State::Decided {
            status: Status::Ignored,
            ..
        }
    ));
    // A new decision requires a current snapshot of the changed code.
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("total += 8", "total += 9"),
    )
    .unwrap();
    index(root.path());
    let changed = Server::start(root.path(), true);
    assert!(changed
        .review("duplicate", &id, "confirmed", "")
        .starts_with("HTTP/1.1 303"));
    assert!(matches!(
        changed.site.borrow().duplicate_state(0),
        State::Decided {
            status: Status::Confirmed,
            ..
        }
    ));
    let history = open_store(root.path());
    assert_eq!(history.history(Kind::Duplicate, &id).unwrap().len(), 2);
}

#[test]
fn adr_015_decisions_are_honoured_and_the_old_file_is_never_rewritten() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = fingerprint_like_adr_015(&server.site.borrow(), 0);
    assert_eq!(
        key,
        reviews::legacy_group_keys(
            &server.site.borrow().duplicates.groups,
            &server.site.borrow().navigation,
            server.site.borrow().duplicates.options.normalization
        )
        .unwrap()[0],
        "the legacy fingerprint remains the ADR-015 key"
    );
    let legacy = root.path().join(".mct-index/duplicate-reviews.toon");
    let content = format!("version: 1\nreviews[1]{{key,reason}}:\n  {key},dismissed\n");
    fs::write(&legacy, &content).unwrap();
    let old = Server::start(root.path(), true);
    assert!(matches!(
        old.site.borrow().duplicate_state(0),
        State::Decided {
            status: Status::Ignored,
            at: None,
            ..
        }
    ));
    assert!(old
        .get("duplicados?status=ignored")
        .contains("decisión anterior, sin fecha"));
    let id = old.duplicate_id(0);
    assert!(old
        .review("duplicate", &id, "restore", "")
        .starts_with("HTTP/1.1 303"));
    assert!(old.get("duplicados").contains(&format!("duplicado?k={id}")));
    assert!(old
        .review("duplicate", &id, "false_positive", "")
        .starts_with("HTTP/1.1 303"));
    assert_eq!(fs::read_to_string(&legacy).unwrap(), content);
    assert!(matches!(
        Server::start(root.path(), true)
            .site
            .borrow()
            .duplicate_state(0),
        State::Decided {
            status: Status::FalsePositive,
            ..
        }
    ));
}

#[test]
fn dead_code_candidates_are_listed_honestly_and_reviewed_like_duplicates() {
    let root = project();
    fs::write(
        root.path().join("src/c.rs"),
        "pub fn unused_helper() {}\npub fn used_helper() {}\npub fn caller() { used_helper(); }\n",
    )
    .unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let list = server.get("muerto");
    assert!(
        list.starts_with("HTTP/1.1 200") && list.contains("<h1>Código muerto</h1>"),
        "{list}"
    );
    assert!(
        list.contains("<code>unused_helper</code>") && !list.contains("<code>used_helper</code>")
    );
    assert!(list.contains("no demostrado") || list.contains("no código probadamente inalcanzable"));
    assert!(list.contains("reflexión"), "limitations are listed");
    let id = server.dead_id("unused_helper");
    let detail = server.get(&format!("simbolo?k={id}"));
    assert!(
        detail.contains("pub fn unused_helper") && detail.contains("Antes de borrar, comprueba"),
        "{detail}"
    );
    assert!(detail.contains("0 por nombre"));
    assert!(server
        .review("dead", &id, "false_positive", "framework")
        .contains("Location: muerto\r\n"));
    assert!(!server.get("muerto").contains("<code>unused_helper</code>"));
    assert!(server
        .get("muerto?status=false_positive")
        .contains("<code>unused_helper</code>"));
    assert!(server.get("revisiones?kind=dead").contains("unused_helper"));
    assert!(server
        .get("revisiones?kind=duplicate")
        .contains("Todavía no hay decisiones"));
    // Same identity after the symbol moves down the file; a body edit asks for a new look.
    let file = root.path().join("src/c.rs");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("// header\n\n{text}")).unwrap();
    index(root.path());
    let moved = Server::start(root.path(), true);
    assert_eq!(moved.dead_id("unused_helper"), id);
    assert!(matches!(
        moved
            .site
            .borrow()
            .dead
            .iter()
            .position(|d| d.identity == id)
            .map(|i| moved.site.borrow().dead_state(i)),
        Some(State::Decided { .. })
    ));
    assert!(moved
        .review("dead", &id, "restore", "")
        .starts_with("HTTP/1.1 303"));
    assert!(moved.get("muerto").contains("<code>unused_helper</code>"));
    // A dead-code identity is not a duplicate group: the other kind rejects it.
    assert!(moved
        .review("duplicate", &id, "ignored", "")
        .starts_with("HTTP/1.1 404"));
}

#[test]
fn review_posts_require_origin_csrf_valid_action_and_current_finding() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let body = server.review_body("duplicate", &id, "false_positive", &[]);
    assert!(server.form("review", &body, "").starts_with("HTTP/1.1 403"));
    assert!(server
        .form("review", &body, "Origin: https://evil.example\r\n")
        .starts_with("HTTP/1.1 403"));
    let csrf = server.site.borrow().csrf.clone();
    let origin = "Origin: http://127.0.0.1:8787\r\n";
    assert!(server
        .form("review", &body.replace(&csrf, "bad"), origin)
        .starts_with("HTTP/1.1 403"));
    assert!(server
        .review("duplicate", &"0".repeat(64), "false_positive", "")
        .starts_with("HTTP/1.1 404"));
    assert!(server
        .review("duplicate", "not-a-key", "false_positive", "")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .review("duplicate", &id, "erase-source", "")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .review("duplicate", &id, "pending", "")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .review("other", &id, "ignored", "")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .form("review", &(body.clone() + "&action=restore"), origin)
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .form("review", &(body.clone() + "&surprise=1"), origin)
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .form("elsewhere", &body, origin)
        .starts_with("HTTP/1.1 405"));
    assert!(server
        .get(&format!("review?{body}"))
        .starts_with("HTTP/1.1 404"));
    let huge = "a".repeat(4097);
    assert!(server
        .form("review", &huge, origin)
        .starts_with("HTTP/1.1 413"));
    assert!(
        saved_decisions(root.path()).is_empty(),
        "nothing was written by rejected requests"
    );
    // `next` can only name a known page.
    let bad_next = server.form(
        "review",
        &server.review_body(
            "duplicate",
            &id,
            "ignored",
            &[("next", "https://evil.example/")],
        ),
        origin,
    );
    assert!(bad_next.contains("Location: duplicados\r\n"), "{bad_next}");
    let good_next = server.form(
        "review",
        &server.review_body(
            "duplicate",
            &id,
            "confirmed",
            &[("next", "revisiones?status=confirmed")],
        ),
        origin,
    );
    assert!(
        good_next.contains("Location: revisiones?status=confirmed\r\n"),
        "{good_next}"
    );
    // Reasons are single-line and bounded.
    let long = "x".repeat(2000);
    assert!(server
        .review(
            "duplicate",
            &id,
            "ignored",
            &format!("{long}\r\nSet-Cookie: a=b")
        )
        .starts_with("HTTP/1.1 303"));
    let saved = saved_decisions(root.path());
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].reason, "x".repeat(500));
    assert!(!saved[0].reason.contains('\n'));
}

#[test]
fn a_failed_review_save_is_visible_and_does_not_change_the_page() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    block_review_writes(root.path());
    let response = server.review("duplicate", &id, "false_positive", "");
    assert!(
        response.starts_with("HTTP/1.1 500") && response.contains("No se aplicó ningún cambio"),
        "{response}"
    );
    assert!(server
        .get("duplicados")
        .contains(&format!("duplicado?k={id}")));
    assert!(saved_decisions(root.path()).is_empty());
}

#[test]
fn current_ignore_rules_hide_stale_index_rows_from_every_view_and_the_vault() {
    let root = project();
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        format!(
            "use crate::b::beta;\n{}",
            fs::read_to_string(&file).unwrap()
        ),
    )
    .unwrap();
    index(root.path());
    fs::write(root.path().join(".mctignore"), "src/b.rs\n").unwrap();
    fs::remove_file(root.path().join("src/b.rs")).unwrap();
    let server = Server::start(root.path(), false);
    let site = server.site.borrow();
    assert!(!site
        .navigation
        .files
        .iter()
        .any(|f| f.relative_path == "src/b.rs"));
    assert!(!site
        .navigation
        .symbols
        .iter()
        .any(|s| s.relative_path == "src/b.rs"));
    assert!(site.navigation.imports.is_empty());
    assert!(site.duplicates.groups.is_empty() && site.duplicates.skipped.is_empty());
    assert!(!site
        .dead
        .iter()
        .any(|d| d.entry.relative_path == "src/b.rs"));
    drop(site);
    assert!(!server.get("explorar?dir=src").contains("src/b.rs"));
    assert!(server
        .get("archivo?path=src%2Fb.rs")
        .starts_with("HTTP/1.1 404"));
    assert!(!server.get("muerto").contains("<code>beta</code>"));
    assert!(server.get("ajustes").contains("src/b.rs"));
    let root_path = root.path().canonicalize().unwrap();
    let vault = mct_obsidian::vault_dir(&root_path);
    for note in mct_obsidian::Manifest::load(&vault).unwrap().notes.keys() {
        assert!(!fs::read_to_string(vault.join(note))
            .unwrap()
            .contains("src/b.rs"));
    }
    fs::write(root_path.join(".mctignore"), [0xff, 0xfe]).unwrap();
    assert!(
        Site::load(&root_path, true).is_err(),
        "unreadable rules must fail closed"
    );
}

#[test]
fn web_regenerates_obsidian_and_survives_a_vault_conflict() {
    let root = project();
    {
        let server = Server::start(root.path(), false);
        assert!(server.get("notas").contains("200 OK"));
        assert!(root.path().join(".mct-index/obsidian/index.md").is_file());
    }
    let note = root.path().join(".mct-index/obsidian/index.md");
    fs::write(&note, "Personal edited map").unwrap();
    let server = Server::start(root.path(), false);
    assert!(server
        .get("duplicados")
        .contains("No se pudo regenerar la bóveda de Obsidian"));
    assert!(server.get("obsidian").contains("200 OK"));
    assert_eq!(fs::read_to_string(note).unwrap(), "Personal edited map");
}

#[test]
fn notes_are_listed_searched_rendered_linked_and_graphed_from_the_vault() {
    let root = project();
    let server = Server::start(root.path(), false);
    let (count, links) = {
        let site = server.site.borrow();
        (site.notes.list.len(), site.notes.link_count)
    };
    assert!(count >= 4 && links >= 3, "{count} notes, {links} links");
    let list = server.get("notas");
    assert!(list.contains("nota?n=index.md"), "{list}");
    let found = server.get("notas?q=alpha");
    assert!(found.contains("a.rs") && !found.contains("b.rs"), "{found}");
    let index_note = server.get("nota?n=index.md");
    assert!(
        index_note.starts_with("HTTP/1.1 200") && index_note.contains("<h1>Code atlas</h1>")
            || index_note.contains("Code atlas"),
        "{index_note}"
    );
    assert!(
        index_note.contains("href=\"nota?n=directories%2F"),
        "internal links point at listed notes: {index_note}"
    );
    let file_note = {
        let site = server.site.borrow();
        site.notes
            .list
            .iter()
            .find(|n| n.source.as_deref() == Some("src/a.rs"))
            .unwrap()
            .name
            .clone()
    };
    let encoded = form_urlencoded::byte_serialize(file_note.as_bytes()).collect::<String>();
    let page = server.get(&format!("nota?n={encoded}"));
    assert!(
        page.contains("archivo?path=src%2Fa.rs") && page.contains("Enlazada desde"),
        "{page}"
    );
    assert!(server
        .get("archivo?path=src%2Fa.rs")
        .contains(&format!("nota?n={encoded}")));
    let graph = server.get("grafo");
    assert!(
        graph.contains("data-graph=\"") && graph.contains("<canvas>"),
        "{graph}"
    );
    let data = graph
        .split("data-graph=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    let json: serde_json::Value = serde_json::from_str(&data).unwrap();
    let nodes = json["nodes"].as_array().unwrap().len();
    assert!(
        nodes > 0
            && json["edges"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e[0].as_u64().unwrap() < nodes as u64
                    && e[1].as_u64().unwrap() < nodes as u64)
    );
    assert!(server
        .get(&format!("grafo?n={encoded}"))
        .contains("Vecindad de"));
    assert!(server.get("grafo?n=missing.md").starts_with("HTTP/1.1 404"));
    let without = Server::start(root.path(), true);
    assert!(
        without.site.borrow().notes.list.len() >= 4,
        "an existing vault is still readable with --no-obsidian"
    );
}

#[test]
fn settings_and_status_describe_the_project_without_changing_it() {
    let root = project();
    let server = Server::start(root.path(), true);
    let status = server.get("estado");
    assert!(
        status.contains("Recargar análisis")
            && status.contains("Cobertura por lenguaje")
            && status.contains("rust")
    );
    let settings = server.get("ajustes");
    assert!(
        settings.contains(".mct-index/dashboard.sqlite3")
            && settings.contains("duplicate-reviews.toon")
    );
    assert!(saved_decisions(root.path()).is_empty());
}

impl Server {
    fn get_with(&self, path: &str, headers: &str) -> String {
        self.raw(&format!(
            "GET {}{path} HTTP/1.1\r\nHost: {}\r\n{headers}\r\n",
            self.prefix, self.address
        ))
    }
}

#[test]
fn the_interface_language_follows_the_cookie_then_accept_language_then_spanish() {
    let root = project();
    let server = Server::start(root.path(), true);
    let spanish = server.get("duplicados");
    assert!(spanish.contains("<html lang=\"es\">") && spanish.contains("Código duplicado"));
    let english = server.get_with("duplicados", "Accept-Language: en-US,en;q=0.9\r\n");
    assert!(
        english.contains("<html lang=\"en\">") && english.contains("<h1>Duplicate code</h1>"),
        "{english}"
    );
    assert!(
        english.contains(">Dead code")
            && english.contains(">Reviews")
            && english.contains("Skip to content")
    );
    let cookie = server.get_with(
        "duplicados",
        "Accept-Language: en\r\nCookie: mct_lang=es\r\n",
    );
    assert!(
        cookie.contains("<html lang=\"es\">"),
        "the cookie wins over Accept-Language"
    );
    let junk = server.get_with(
        "duplicados",
        "Cookie: mct_lang=zz; other=1\r\nAccept-Language: fr\r\n",
    );
    assert!(junk.contains("<html lang=\"es\">"));
}

#[test]
fn switching_language_sets_a_scoped_cookie_and_validates_the_request() {
    let root = project();
    let server = Server::start(root.path(), true);
    let csrf = server.site.borrow().csrf.clone();
    let origin = "Origin: http://127.0.0.1:8787\r\n";
    let body = format!("csrf={csrf}&lang=en&next=muerto%3Fstatus%3Dall");
    let response = server.form("lang", &body, origin);
    assert!(
        response.starts_with("HTTP/1.1 303")
            && response.contains("Location: muerto?status=all\r\n"),
        "{response}"
    );
    assert!(response.contains("Set-Cookie: mct_lang=en; Path=/secret/; Max-Age=31536000; SameSite=Strict; HttpOnly\r\n"), "{response}");
    assert!(server.form("lang", &body, "").starts_with("HTTP/1.1 403"));
    assert!(server
        .form("lang", &body.replace(&csrf, "bad"), origin)
        .starts_with("HTTP/1.1 403"));
    assert!(server
        .form("lang", &format!("csrf={csrf}&lang=fr"), origin)
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .form("lang", &format!("csrf={csrf}&lang=en&x=1"), origin)
        .starts_with("HTTP/1.1 400"));
    let bad_next = server.form(
        "lang",
        &format!("csrf={csrf}&lang=es&next=https%3A%2F%2Fevil.example"),
        origin,
    );
    assert!(bad_next.contains("Location: duplicados\r\n") && bad_next.contains("mct_lang=es"));
    // The switch buttons are on every page and carry the page to come back to.
    let page = server.get("muerto?status=all");
    assert!(
        page.contains("name=\"lang\" value=\"en\"") && page.contains("value=\"muerto?status=all\""),
        "{page}"
    );
}

#[test]
fn every_page_is_fully_translated_in_english_and_review_messages_follow() {
    let root = project();
    fs::write(root.path().join("src/c.rs"), "pub fn unused_helper() {}\n").unwrap();
    index(root.path());
    let server = Server::start(root.path(), false);
    let en = "Accept-Language: en\r\n";
    let mut pending = vec![String::from("duplicados")];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(target) = pending.pop() {
        if !visited.insert(target.clone()) {
            continue;
        }
        assert!(visited.len() < 200);
        let response = server.get_with(&target, en);
        assert!(response.starts_with("HTTP/1.1 200"), "{target}: {response}");
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        if !headers.contains("text/html") {
            continue;
        }
        if let Some(found) = body.chars().find(|ch| "áéíóúñ¿¡«»".contains(*ch)) {
            let at = body.find(found).unwrap();
            panic!(
                "Spanish text left on {target}: …{}…",
                &body[at.saturating_sub(80)..(at + 80).min(body.len())]
            );
        }
        for value in body.split("href=\"").skip(1) {
            let href = value.split('"').next().unwrap().replace("&amp;", "&");
            if !href.starts_with('#') {
                pending.push(href);
            }
        }
    }
    for needle in [
        "simbolo?k=",
        "duplicado?k=",
        "grafo",
        "nota?n=",
        "archivo?path=",
        "estado",
        "ajustes",
        "revisiones",
    ] {
        assert!(visited.iter().any(|t| t.starts_with(needle)), "{needle}");
    }
    let id = server.dead_id("unused_helper");
    let body = server.review_body("dead", &id, "bogus", &[]);
    let origin = "Origin: http://127.0.0.1:8787\r\nAccept-Language: en\r\n";
    assert!(server
        .form("review", &body, origin)
        .starts_with("HTTP/1.1 400"));
    block_review_writes(root.path());
    let failed = server.form(
        "review",
        &server.review_body("dead", &id, "ignored", &[]),
        origin,
    );
    assert!(
        failed.starts_with("HTTP/1.1 500") && failed.contains("No change was applied"),
        "{failed}"
    );
    unblock_review_writes(root.path());
    assert!(server
        .form(
            "review",
            &server.review_body("dead", &id, "ignored", &[("reason", "public API")]),
            origin
        )
        .starts_with("HTTP/1.1 303"));
    let ignored = server.get_with("muerto?status=ignored", en);
    assert!(
        ignored.contains("Ignored on purpose")
            && ignored.contains("Restore to pending")
            && ignored.contains("public API"),
        "{ignored}"
    );
    assert!(server.get_with("revisiones", en).contains("Dead code"));
    assert!(server.get_with("estado", en).contains("Reload analysis"));
    // The same decision reads the same in Spanish.
    assert!(server
        .get("muerto?status=ignored")
        .contains("Ignorado a propósito"));
}

fn api_json(response: &str) -> serde_json::Value {
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Content-Type: application/json"));
    assert!(response.contains("Cache-Control: no-store"));
    let (_, body) = response.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(body).unwrap()
}

#[test]
fn the_json_api_reports_the_snapshot_findings_and_their_review_state() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);

    let summary = api_json(&server.get("api/summary"));
    assert_eq!(summary["files"], 2);
    assert!(summary["duplicate_groups"].as_u64().unwrap() >= 1);
    assert_eq!(
        summary["duplicates_pending"], summary["duplicate_groups"],
        "nothing is decided yet"
    );

    let list = api_json(&server.get("api/duplicates?limit=1"));
    assert_eq!(list["limit"], 1);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["identity"], id);
    assert_eq!(list["items"][0]["state"]["status"], "pending");
    assert!(!list["items"][0]["members"].as_array().unwrap().is_empty());

    assert!(server
        .review("duplicate", &id, "confirmed", "same code")
        .starts_with("HTTP/1.1 303"));
    let list = api_json(&server.get("api/duplicates"));
    assert_eq!(list["items"][0]["state"]["status"], "confirmed");
    assert_eq!(list["items"][0]["state"]["reason"], "same code");
    let reviews = api_json(&server.get("api/reviews"));
    assert_eq!(reviews["total"], 1);
    assert_eq!(reviews["items"][0]["current"], true);

    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert_eq!(finding["history"].as_array().unwrap().len(), 1);
    assert!(finding["comments"].as_array().unwrap().is_empty());
}

#[test]
fn the_spa_shell_and_its_assets_are_embedded_and_free_of_inline_code() {
    let root = project();
    let server = Server::start(root.path(), true);
    let csrf = server.site.borrow().csrf.clone();
    let shell = server.get("app");
    assert!(shell.starts_with("HTTP/1.1 200"), "{shell}");
    assert!(shell.contains("text/html"));
    assert!(shell.contains(&format!("name=\"csrf-token\" content=\"{csrf}\"")));
    assert!(shell.contains("<symbol id=\"i-copy\""));
    for placeholder in ["__CSRF__", "__SPRITE__"] {
        assert!(!shell.contains(placeholder), "{placeholder}");
    }
    // The CSP forbids inline script and style, so the shell must not need them.
    let (_, body) = shell.split_once("\r\n\r\n").unwrap();
    assert!(body.contains("src=\"spa.js\"") && body.contains("src=\"alpine.js\""));
    assert!(!body.contains("<script>") && !body.contains("<style"));
    assert!(!body.contains(" style=") && !body.contains("javascript:"));
    assert!(!body.contains("https://") && !body.contains("http://"));

    assert!(server.get("spa.css").contains("text/css"));
    assert!(server.get("spa.js").contains("text/javascript"));
    let alpine = server.get("alpine.js");
    assert!(alpine.contains("text/javascript") && alpine.len() > 10_000);
}

#[test]
fn the_json_api_serves_notes_note_and_graph_only_when_the_vault_exists() {
    let root = project();
    let off = Server::start(root.path(), true);
    let notes = api_json(&off.get("api/notes"));
    assert_eq!(notes["enabled"], false);
    assert_eq!(notes["total"], 0);
    assert!(off.get("api/note?n=index.md").starts_with("HTTP/1.1 404"));
    assert!(off.get("api/notes?kind=bogus").starts_with("HTTP/1.1 400"));

    let on = Server::start(root.path(), false);
    let notes = api_json(&on.get("api/notes?limit=500"));
    assert_eq!(notes["enabled"], true);
    let items = notes["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let name = items[0]["name"].as_str().unwrap().to_string();
    let note = api_json(&on.get(&format!("api/note?n={name}")));
    assert_eq!(note["name"], name);
    assert!(!note["text"].as_str().unwrap().is_empty());
    let graph = api_json(&on.get("api/graph"));
    assert!(!graph["nodes"].as_array().unwrap().is_empty());
    let count = graph["nodes"].as_array().unwrap().len() as u64;
    for edge in graph["edges"].as_array().unwrap() {
        assert!(edge[0].as_u64().unwrap() < count && edge[1].as_u64().unwrap() < count);
    }
    let focused = api_json(&on.get(&format!("api/graph?n={name}")));
    assert!(focused["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["focus"] == true));
    assert!(on.get("api/graph?n=missing.md").starts_with("HTTP/1.1 404"));
    let summary = api_json(&on.get("api/summary"));
    assert!(summary["notes"].as_u64().unwrap() > 0);
}

#[test]
fn the_json_api_rejects_bad_parameters_unknown_findings_and_unknown_routes() {
    let root = project();
    let server = Server::start(root.path(), true);
    for (path, status) in [
        ("api/duplicates?offset=x", "400"),
        ("api/dead?limit=-1", "400"),
        ("api/finding?kind=bogus&id=a", "400"),
        ("api/finding?kind=dead&id=zz", "400"),
        (
            &format!("api/finding?kind=dead&id={}", "a".repeat(64)),
            "404",
        ),
        ("api/nothing", "404"),
    ] {
        assert!(
            server.get(path).starts_with(&format!("HTTP/1.1 {status}")),
            "{path}"
        );
    }
    // Writes are not part of M3: the API accepts no POST.
    let post = server.form("api/summary", "x=1", "Origin: http://127.0.0.1:8787\r\n");
    assert!(post.starts_with("HTTP/1.1 405"), "{post}");
    // A huge limit is clamped, not honoured.
    assert_eq!(api_json(&server.get("api/dead?limit=999999"))["limit"], 200);
}

#[test]
fn the_version_token_moves_on_reviews_and_the_snapshot_follows_a_changed_index() {
    let root = project();
    let server = Server::start(root.path(), true);
    let first = api_json(&server.get("api/version"));
    assert_eq!(first["stale"], false);
    assert!(first["rebuild_ms"].is_u64());
    let id = server.duplicate_id(0);
    assert!(server
        .review("duplicate", &id, "ignored", "")
        .starts_with("HTTP/1.1 303"));
    let after_review = api_json(&server.get("api/version"));
    assert_ne!(first["token"], after_review["token"]);

    // Another indexing run: the index file's time moves forward.
    let database = root.path().join(".mct-index/index.sqlite3");
    fs::OpenOptions::new()
        .write(true)
        .open(&database)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(30))
        .unwrap();
    let stale = api_json(&server.get("api/version"));
    assert_eq!(stale["stale"], true);
    assert_ne!(after_review["token"], stale["token"]);

    // `api/version` never rebuilds; the next data read does, after the interval.
    std::thread::sleep(Duration::from_millis(2200));
    // The rebuild runs on a worker thread: the read that starts it is answered
    // from the old snapshot, and a later read swaps the new one in.
    api_json(&server.get("api/summary"));
    let mut fresh = api_json(&server.get("api/version"));
    for _ in 0..300 {
        if fresh["stale"] == false {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        api_json(&server.get("api/summary"));
        fresh = api_json(&server.get("api/version"));
    }
    assert_eq!(fresh["stale"], false);
    assert_eq!(fresh["rebuilding"], false);
    assert_ne!(stale["token"], fresh["token"]);
}

#[test]
fn json_writes_save_reviews_and_comments_and_show_up_in_the_reads() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let before = api_json(&server.get("api/version"));

    let decided = server.write(
        "api/review",
        &serde_json::json!({"kind": "duplicate", "id": id, "action": "ignored", "reason": "generated"}),
    );
    assert!(decided.starts_with("HTTP/1.1 200"), "{decided}");
    assert_eq!(api_json(&decided)["state"]["status"], "ignored");
    let listed = api_json(&server.get("api/duplicates"));
    let item = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["identity"] == id.as_str())
        .unwrap();
    assert_eq!(item["state"]["reason"], "generated");
    assert_ne!(
        before["token"],
        api_json(&server.get("api/version"))["token"]
    );

    let restored = server.write(
        "api/review",
        &serde_json::json!({"kind": "duplicate", "id": id, "action": "restore"}),
    );
    assert_eq!(api_json(&restored)["state"]["status"], "pending");

    let added = api_json(&server.write(
        "api/comment",
        &serde_json::json!({"kind": "duplicate", "id": id, "body": "first\nsecond"}),
    ));
    let comment_id = added["comment_id"].as_i64().unwrap();
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert_eq!(finding["comments"][0]["body"], "first\nsecond");
    assert!(finding["history"].as_array().unwrap().len() >= 2);

    let edited = server.write(
        "api/comment/update",
        &serde_json::json!({"comment_id": comment_id, "body": "edited"}),
    );
    assert!(edited.starts_with("HTTP/1.1 200"), "{edited}");
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert_eq!(finding["comments"][0]["body"], "edited");

    let removed = server.write(
        "api/comment/delete",
        &serde_json::json!({"comment_id": comment_id}),
    );
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert!(finding["comments"].as_array().unwrap().is_empty());
    // The same comment cannot be deleted or edited twice.
    for (route, body) in [
        (
            "api/comment/delete",
            serde_json::json!({"comment_id": comment_id}),
        ),
        (
            "api/comment/update",
            serde_json::json!({"comment_id": comment_id, "body": "x"}),
        ),
    ] {
        assert!(server.write(route, &body).starts_with("HTTP/1.1 404"));
    }
}

#[test]
fn file_reviews_are_saved_per_member_and_reject_strangers_and_bad_input() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert!(finding["files"].as_array().unwrap().is_empty());
    let listed = api_json(&server.get("api/duplicates"));
    let item = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["identity"] == id.as_str())
        .unwrap();
    let path = item["members"][0]["path"].as_str().unwrap().to_owned();

    let saved = server.write(
        "api/file-review",
        &serde_json::json!({"id": id, "path": path, "status": "in_review", "note": "check\nlater"}),
    );
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert_eq!(api_json(&saved)["status"], "in_review");
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert_eq!(finding["files"][0]["path"], path.as_str());
    assert_eq!(finding["files"][0]["status"], "in_review");
    assert_eq!(finding["files"][0]["note"], "check\nlater");

    let rejected = [
        serde_json::json!({"id": id, "path": path, "status": "bogus"}),
        serde_json::json!({"id": id, "path": path, "status": "accepted", "extra": 1}),
        serde_json::json!({"id": id, "path": path, "status": "accepted", "note": "x".repeat(2001)}),
        serde_json::json!({"id": "nothex", "path": path, "status": "accepted"}),
    ];
    for body in rejected {
        let response = server.write("api/file-review", &body);
        assert!(response.starts_with("HTTP/1.1 400"), "{body}: {response}");
    }
    for stranger_path in ["not/a/member.rs", ""] {
        let stranger = server.write(
            "api/file-review",
            &serde_json::json!({"id": id, "path": stranger_path, "status": "accepted"}),
        );
        assert!(stranger.starts_with("HTTP/1.1 404"), "{stranger}");
    }
    let unknown = server.write(
        "api/file-review",
        &serde_json::json!({"id": "0".repeat(64), "path": path, "status": "accepted"}),
    );
    assert!(unknown.starts_with("HTTP/1.1 404"), "{unknown}");

    // Back to "pending" with no note removes the row.
    let cleared = server.write(
        "api/file-review",
        &serde_json::json!({"id": id, "path": path, "status": "pending", "note": ""}),
    );
    assert!(cleared.starts_with("HTTP/1.1 200"), "{cleared}");
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert!(finding["files"].as_array().unwrap().is_empty());
}

#[test]
fn json_writes_require_origin_csrf_json_size_and_valid_bodies() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let good = serde_json::json!({"kind": "duplicate", "id": id, "action": "ignored"}).to_string();
    let csrf = server.site.borrow().csrf.clone();
    let origin = "Origin: http://127.0.0.1:8787\r\n";
    let token = format!("X-CSRF-Token: {csrf}\r\n");
    let status = |response: String| response.lines().next().unwrap().to_string();

    // Transport rules.
    for (headers, expected) in [
        (token.clone(), "403"),
        (origin.to_string(), "403"),
        (format!("{origin}X-CSRF-Token: wrong\r\n"), "403"),
        (format!("Origin: http://evil.example\r\n{token}"), "403"),
        (
            format!("{origin}Sec-Fetch-Site: cross-site\r\n{token}"),
            "403",
        ),
    ] {
        let response = server.json_post("api/review", &good, &headers);
        assert!(status(response).contains(expected), "{headers}");
    }
    let form_type = server.raw(&format!(
        "POST /secret/api/review HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{origin}{token}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{good}",
        good.len()
    ));
    assert!(status(form_type).contains("400"));
    let big = format!("{{\"body\":\"{}\"}}", "x".repeat(9000));
    assert!(
        status(server.json_post("api/comment", &big, &format!("{origin}{token}"))).contains("413")
    );
    assert!(
        status(server.json_post("api/other", "{}", &format!("{origin}{token}"))).contains("405")
    );
    // The form route still rejects a JSON content type.
    assert!(status(server.json_post("review", &good, origin)).contains("400"));

    // Body rules.
    let post = |route: &str, body: serde_json::Value| status(server.write(route, &body));
    let bad_id = "z".repeat(64);
    let unknown = "a".repeat(64);
    for (route, body, expected) in [
        (
            "api/review",
            serde_json::json!({"kind": "duplicate", "id": id, "action": "pending"}),
            "400",
        ),
        (
            "api/review",
            serde_json::json!({"kind": "duplicate", "id": id, "action": "ignored", "extra": 1}),
            "400",
        ),
        (
            "api/review",
            serde_json::json!({"kind": "duplicate", "id": id, "action": "ignored", "reason": 5}),
            "400",
        ),
        (
            "api/review",
            serde_json::json!({"kind": "nope", "id": id, "action": "ignored"}),
            "400",
        ),
        (
            "api/review",
            serde_json::json!({"kind": "duplicate", "id": bad_id, "action": "ignored"}),
            "400",
        ),
        (
            "api/review",
            serde_json::json!({"kind": "duplicate", "id": unknown, "action": "ignored"}),
            "404",
        ),
        ("api/review", serde_json::json!([1]), "400"),
        (
            "api/comment",
            serde_json::json!({"kind": "duplicate", "id": id, "body": "  "}),
            "400",
        ),
        (
            "api/comment",
            serde_json::json!({"kind": "duplicate", "id": id, "body": "x".repeat(2001)}),
            "400",
        ),
        (
            "api/comment",
            serde_json::json!({"kind": "duplicate", "id": unknown, "body": "hi"}),
            "404",
        ),
        (
            "api/comment/update",
            serde_json::json!({"comment_id": "1", "body": "hi"}),
            "400",
        ),
        (
            "api/comment/update",
            serde_json::json!({"comment_id": 999, "body": "hi"}),
            "404",
        ),
        (
            "api/comment/delete",
            serde_json::json!({"comment_id": 1, "body": "hi"}),
            "400",
        ),
    ] {
        let line = post(route, body.clone());
        assert!(line.contains(expected), "{route} {body}: {line}");
    }
    assert!(
        status(server.json_post("api/review", "not json", &format!("{origin}{token}")))
            .contains("400")
    );
    // Nothing above changed any state.
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")));
    assert!(finding["history"].as_array().unwrap().is_empty());
    assert!(finding["comments"].as_array().unwrap().is_empty());
}

#[test]
fn decisions_survive_unrelated_edits_and_moves_but_symbol_edits_need_review() {
    let root = project();
    let initial = Server::start(root.path(), true);
    let dup = initial.duplicate_id(0);
    let dead = initial.dead_id("alpha");
    initial.review("duplicate", &dup, "ignored", "keep");
    initial.review("dead", &dead, "false_positive", "public API");
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    for text in [
        format!("{original}\n// unrelated edit\n"),
        format!("// moved\n\n{original}"),
    ] {
        fs::write(&file, text).unwrap();
        index(root.path());
        let moved = Server::start(root.path(), true);
        assert_eq!(moved.duplicate_id(0), dup);
        assert_eq!(moved.dead_id("alpha"), dead);
        assert!(matches!(
            moved.site.borrow().duplicate_state(0),
            State::Decided { .. }
        ));
        let site = moved.site.borrow();
        assert!(matches!(
            site.dead_state(site.dead_ids[&dead]),
            State::Decided { .. }
        ));
    }
    fs::write(&file, original.replace("total += 8", "total += 9")).unwrap();
    index(root.path());
    let changed = Server::start(root.path(), true);
    let site = changed.site.borrow();
    assert!(matches!(
        site.duplicate_state(0),
        State::NeedsReevaluation { .. }
    ));
    assert!(matches!(
        site.dead_state(site.dead_ids[&dead]),
        State::NeedsReevaluation { .. }
    ));
}

#[test]
fn unindexed_source_and_missing_files_never_validate_a_saved_decision() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.dead_id("alpha");
    server.review("dead", &id, "ignored", "");
    let file = root.path().join("src/a.rs");
    fs::write(&file, "// different unindexed source\n").unwrap();
    let stale = Server::start(root.path(), true);
    let site = stale.site.borrow();
    assert!(matches!(
        site.dead_state(site.dead_ids[&id]),
        State::NeedsReevaluation { .. }
    ));
    drop(site);
    fs::remove_file(&file).unwrap();
    let missing = Server::start(root.path(), true);
    let site = missing.site.borrow();
    assert!(matches!(
        site.dead_state(site.dead_ids[&id]),
        State::NeedsReevaluation { .. }
    ));
}

#[test]
fn orphan_decisions_are_kept_until_explicit_protected_deletion() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.dead_id("alpha");
    server.review("dead", &id, "ignored", "keep until deleted");
    let body = serde_json::json!({"kind": "dead", "id": id});
    assert!(server
        .write("api/review/delete", &body)
        .starts_with("HTTP/1.1 400"));
    fs::remove_file(root.path().join("src/a.rs")).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    let list = orphan.get("revisiones?status=obsolete");
    assert!(list.contains("review/delete") && list.contains("keep until deleted"));
    assert_eq!(saved_decisions(root.path()).len(), 1);
    assert!(orphan
        .json_post(
            "api/review/delete",
            &body.to_string(),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 403"));
    assert_eq!(saved_decisions(root.path()).len(), 1);
    assert!(orphan
        .write("api/review/delete", &body)
        .starts_with("HTTP/1.1 200"));
    assert!(saved_decisions(root.path()).is_empty());
    assert_eq!(
        open_store(root.path())
            .history(Kind::Dead, &id)
            .unwrap()
            .len(),
        1
    );
    assert!(orphan
        .write("api/review/delete", &body)
        .starts_with("HTTP/1.1 404"));
}

#[test]
fn site_upgrades_proven_toon_fingerprints_and_rechecks_unproven_ones() {
    for edited in [false, true] {
        let root = project();
        let initial = Server::start(root.path(), true);
        let site = initial.site.borrow();
        let dup = site.duplicate_keys[0].identity.clone();
        let dead = initial.dead_id("alpha");
        let entry = site.dead[site.dead_ids[&dead]].entry.clone();
        let old_dead = reviews::legacy_dead_keys(&[entry], &site.navigation)
            .remove(0)
            .1;
        let old_dup = fingerprint_like_adr_015(&site, 0);
        fs::write(root.path().join(".mct-index/reviews.toon"), format!(
            "version: 2\nreviews[2]{{kind,identity,fingerprint,status,at,reason}}:\n  dead,{dead},{old_dead},false_positive,7,API\n  duplicate,{dup},{old_dup},ignored,8,keep\nhistory[0]{{kind,identity,fingerprint,status,at,reason}}:\n"
        )).unwrap();
        drop(site);
        if edited {
            let file = root.path().join("src/a.rs");
            fs::write(
                &file,
                fs::read_to_string(&file)
                    .unwrap()
                    .replace("total += 8", "total += 9"),
            )
            .unwrap();
            index(root.path());
        }
        let loaded = Server::start(root.path(), true);
        let site = loaded.site.borrow();
        for state in [
            site.duplicate_state(site.duplicate_ids[&dup]),
            site.dead_state(site.dead_ids[&dead]),
        ] {
            if edited {
                assert!(matches!(state, State::NeedsReevaluation { .. }));
            } else {
                assert!(matches!(state, State::Decided { .. }));
            }
        }
        assert!(
            site.reviews.history(Kind::Dead, &dead).unwrap().is_empty(),
            "format upgrades add no human decisions"
        );
    }
}

#[test]
fn stale_orphan_views_cannot_delete_a_reappearing_finding() {
    let root = project();
    let initial = Server::start(root.path(), true);
    let id = initial.dead_id("alpha");
    initial.review("dead", &id, "ignored", "keep");
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::remove_file(&file).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    fs::write(&file, original).unwrap();
    index(root.path());
    assert!(orphan
        .write(
            "api/review/delete",
            &serde_json::json!({"kind": "dead", "id": id})
        )
        .starts_with("HTTP/1.1 409"));
    assert_eq!(saved_decisions(root.path()).len(), 1);
}

#[test]
fn server_rendered_orphan_delete_uses_the_existing_csrf_gate() {
    let root = project();
    let initial = Server::start(root.path(), true);
    let id = initial.dead_id("alpha");
    initial.review("dead", &id, "ignored", "keep");
    fs::remove_file(root.path().join("src/a.rs")).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    let csrf = orphan.site.borrow().csrf.clone();
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", csrf.as_str()),
            ("kind", "dead"),
            ("id", id.as_str()),
            ("expected_revision", "1"),
        ])
        .finish();
    assert!(orphan
        .form(
            "review/delete",
            &body.replace(&csrf, "wrong"),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 403"));
    assert_eq!(saved_decisions(root.path()).len(), 1);
    assert!(orphan
        .form("review/delete", &body, "Origin: http://127.0.0.1:8787\r\n")
        .starts_with("HTTP/1.1 303"));
    assert!(saved_decisions(root.path()).is_empty());
}

#[test]
fn stale_form_and_api_decisions_cannot_validate_unseen_code() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.dead_id("alpha");
    let body = server.review_body("dead", &id, "false_positive", &[]);
    let item = api_json(&server.get("api/dead"))["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["identity"] == id)
        .unwrap()
        .clone();
    let payload = serde_json::json!({"kind":"dead","id":id,"action":"false_positive","fingerprint":item["fingerprint"],"expected_revision":item["revision"]});
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("total += 8", "total += 9"),
    )
    .unwrap();
    index(root.path());
    let csrf = server.site.borrow().csrf.clone();
    assert!(server
        .form(
            "reload",
            &format!("csrf={csrf}"),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 303"));
    assert!(server
        .form("review", &body, "Origin: http://127.0.0.1:8787\r\n")
        .starts_with("HTTP/1.1 409"));
    assert!(server
        .write("api/review", &payload)
        .starts_with("HTTP/1.1 409"));
    assert!(saved_decisions(root.path()).is_empty());
    assert!(server
        .review("dead", &id, "false_positive", "")
        .starts_with("HTTP/1.1 303"));
}

#[test]
fn duplicate_editor_submissions_conflict_and_unindexed_edits_block_decisions() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let old = server.review_body("duplicate", &id, "ignored", &[]);
    assert!(server
        .review("duplicate", &id, "confirmed", "")
        .starts_with("HTTP/1.1 303"));
    assert!(server
        .form("review", &old, "Origin: http://127.0.0.1:8787\r\n")
        .starts_with("HTTP/1.1 409"));
    let before = saved_decisions(root.path());
    fs::write(root.path().join("src/a.rs"), "// source not indexed yet\n").unwrap();
    assert!(server
        .review("duplicate", &id, "ignored", "")
        .starts_with("HTTP/1.1 409"));
    assert_eq!(saved_decisions(root.path()), before);
}

#[test]
fn finding_lists_are_bounded_filtered_and_pinned_to_a_review_generation() {
    let root = tempfile::tempdir().unwrap();
    let text: String = (0..330)
        .map(|n| format!("pub fn item_{n:03}() {{}}\n"))
        .collect();
    fs::write(root.path().join("bulk.rs"), text).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let first = api_json(&server.get("api/dead?limit=50"));
    assert_eq!(first["total"], 330);
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    let token = first["snapshot"].as_str().unwrap();
    let query = api_json(&server.get(&format!("api/dead?limit=50&q=item_2&snapshot={token}")));
    assert_eq!(query["total"], 100);
    let id = first["items"][0]["identity"].as_str().unwrap();
    assert!(server
        .review("dead", id, "false_positive", "")
        .starts_with("HTTP/1.1 303"));
    assert!(server
        .get(&format!("api/dead?offset=50&snapshot={token}"))
        .starts_with("HTTP/1.1 409"));
    let filtered = api_json(&server.get("api/dead?status=false_positive&limit=50"));
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["counts"]["all"], 330);
    assert_eq!(filtered["items"][0]["identity"], id);
    assert!(server
        .get("api/dead?status=not_a_status")
        .starts_with("HTTP/1.1 400"));
}

#[test]
fn retained_pending_orphan_annotations_are_visible_and_archivable() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.dead_id("alpha");
    server.review("dead", &id, "ignored", "");
    server.review("dead", &id, "restore", "");
    assert!(server
        .write(
            "api/comment",
            &serde_json::json!({"kind":"dead","id":id,"body":"retained note"})
        )
        .starts_with("HTTP/1.1 200"));
    fs::remove_file(root.path().join("src/a.rs")).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    let listing = api_json(&orphan.get("api/reviews?status=orphan"));
    assert_eq!(listing["total"], 1);
    let detail = api_json(&orphan.get(&format!("api/finding?kind=dead&id={id}")));
    assert_eq!(detail["current"], false);
    assert_eq!(detail["comments"][0]["body"], "retained note");
    assert!(orphan
        .write(
            "api/review/delete",
            &serde_json::json!({"kind":"dead","id":id})
        )
        .starts_with("HTTP/1.1 200"));
    assert_eq!(
        api_json(&orphan.get("api/reviews?status=orphan"))["total"],
        0
    );
    assert_eq!(
        open_store(root.path())
            .comments(Kind::Dead, &id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn full_duplicate_source_is_bounded_explicitly_and_can_be_paged_to_the_end() {
    let root = tempfile::tempdir().unwrap();
    let body: String = (0..505).map(|n| format!("    total += {n};\n")).collect();
    for name in ["alpha", "beta"] {
        let formatted = if name == "alpha" {
            body.clone()
        } else {
            (0..505)
                .map(|n| format!("total += {n};{}", if n % 50 == 49 { "\n" } else { " " }))
                .collect()
        };
        fs::write(
            root.path().join(format!("{name}.rs")),
            format!("pub fn {name}(mut total:i64)->i64 {{\n{formatted}total\n}}\n"),
        )
        .unwrap();
    }
    index(root.path());
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let first = api_json(&server.get(&format!("api/duplicates?id={id}&full=1")));
    let member = &first["items"][0]["members"][0];
    assert_eq!(member["excerpt"].as_array().unwrap().len(), 400);
    assert_eq!(member["excerpt_truncated"], true);
    assert_eq!(member["source_next_offset"], 400);
    let token = first["snapshot"].as_str().unwrap();
    let last = api_json(&server.get(&format!(
        "api/duplicates?id={id}&full=1&source_offset=400&snapshot={token}"
    )));
    let tail = &last["items"][0]["members"][0];
    assert!(tail["source_next_offset"].is_null());
    assert_eq!(tail["excerpt_first"], 401);
    let shorter = &last["items"][0]["members"][1];
    assert_eq!(
        shorter["excerpt_available"], true,
        "a finished shorter member is not an unreadable file"
    );
    assert!(shorter["excerpt"].as_array().unwrap().is_empty());
    assert!(tail["excerpt"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line.as_str().unwrap().contains("total += 504")));
}

#[test]
fn a_dead_candidate_in_full_carries_its_verified_source_window_in_pages() {
    let root = project();
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        format!("// a\n// b\n// c\n// d\npub fn alpha{BODY}// e\n"),
    )
    .unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let id = server.dead_id("alpha");
    let list = api_json(&server.get("api/dead"));
    let listed = &list["items"].as_array().unwrap()[0];
    assert!(listed.get("excerpt").is_none(), "the list stays light");
    assert_eq!(listed["line_count"], 9);
    assert!(listed["cautions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "unseen"));
    let one = api_json(&server.get(&format!("api/dead?id={id}&full=1")));
    assert_eq!(one["total"], 1);
    let item = &one["items"][0];
    assert_eq!(
        (item["window_first"].clone(), item["window_last"].clone()),
        (2.into(), 14.into())
    );
    assert_eq!(item["excerpt_first"], 2);
    assert_eq!(item["excerpt"].as_array().unwrap().len(), 13);
    assert_eq!(item["excerpt"][3], "pub fn alpha(value: i64) -> i64 {");
    assert!(item["source_next_offset"].is_null());
    let token = one["snapshot"].as_str().unwrap();
    let paged = api_json(&server.get(&format!(
        "api/dead?id={id}&full=1&source_offset=0&source_limit=5&snapshot={token}"
    )));
    assert_eq!(paged["items"][0]["source_next_offset"], 5);
    assert_eq!(paged["items"][0]["excerpt_truncated"], true);
    fs::write(&file, "pub fn alpha() {}\n").unwrap();
    let stale = api_json(&server.get(&format!("api/dead?id={id}&full=1")));
    assert_eq!(
        stale["items"][0]["excerpt_available"], false,
        "never a stale range on new content"
    );
    assert!(stale["items"][0]["excerpt"].as_array().unwrap().is_empty());
    assert!(server
        .get(&format!("api/dead?id={id}&full=1&source_limit=many"))
        .starts_with("HTTP/1.1 400"));
}

#[test]
fn saved_dead_decisions_survive_sibling_removal_and_are_shared_with_ai_queries() {
    for with_provenance in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("twins.py");
        fs::write(
            &file,
            "def twin():\n    return 1\n\ndef twin():\n    return 2\n",
        )
        .unwrap();
        index(root.path());
        let server = Server::start(root.path(), true);
        let (id, fp) = {
            let site = server.site.borrow();
            let twin = site
                .dead
                .iter()
                .filter(|d| d.entry.name == "twin")
                .nth(1)
                .unwrap();
            (twin.identity.clone(), twin.fingerprint.clone())
        };
        if with_provenance {
            assert!(server
                .review("dead", &id, "false_positive", "last definition")
                .starts_with("HTTP/1.1 303"));
        } else {
            open_store(root.path())
                .set(
                    (Kind::Dead, &id, &fp),
                    Status::FalsePositive,
                    "last definition",
                    1,
                )
                .unwrap();
        }
        fs::write(&file, "def twin():\n    return 2\n").unwrap();
        index(root.path());
        let moved = Server::start(root.path(), true);
        assert_eq!(moved.dead_id("twin"), id);
        assert!(matches!(
            moved.site.borrow().dead_state(0),
            State::Decided {
                status: Status::FalsePositive,
                ..
            }
        ));
        let path = root.path().canonicalize().unwrap();
        let idx = mct_index::Index::open_existing_read_only(
            &path,
            &path.join(".mct-index/index.sqlite3"),
            mct_index::ExcludeSet::default(),
        )
        .unwrap();
        let candidates = mct_index::find_dead_code_candidates(&idx, None, None).unwrap();
        let navigation = idx.navigation_snapshot().unwrap();
        let saved = mct_index::review_state::ReviewSnapshot::load(&path).unwrap();
        assert!(saved.filter_dead(&path, &navigation, candidates).is_empty());
        drop(idx);
        fs::write(&file, "def twin():\n    return 3\n").unwrap();
        index(root.path());
        let changed = Server::start(root.path(), true);
        assert_eq!(changed.dead_id("twin"), id);
        assert!(matches!(
            changed.site.borrow().dead_state(0),
            State::NeedsReevaluation { .. }
        ));
    }
}

#[test]
fn blank_snapshot_explicitly_restarts_a_bounded_read() {
    let root = project();
    let server = Server::start(root.path(), true);
    assert!(server
        .get("api/dead?snapshot=&offset=0&limit=1")
        .starts_with("HTTP/1.1 200"));
}

/// Decision-write audit regressions; a child module so they reuse this harness.
#[path = "tests_decisions.rs"]
mod decisions;
