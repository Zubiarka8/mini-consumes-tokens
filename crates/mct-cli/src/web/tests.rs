//! Public index APIs plus full HTTP wire parsing/response tests, without sockets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::reviews::{self, Kind, State, Status};
use super::{http, Site};
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
    fn review_body(&self, kind: &str, id: &str, action: &str, extra: &[(&str, &str)]) -> String {
        let csrf = self.site.borrow().csrf.clone();
        let mut pairs = vec![
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

fn reviews_file(root: &Path) -> std::path::PathBuf {
    root.join(".mct-index/reviews.toon")
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
    assert!(!reviews_file(root.path()).exists());
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
    let history = reviews::Reviews::load(&root.path().canonicalize().unwrap()).unwrap();
    assert_eq!(
        history.history(Kind::Duplicate, &id).len(),
        2,
        "restore keeps the earlier decision"
    );
    assert_eq!(fs::read(database).unwrap(), original);
}

#[test]
fn a_changed_file_asks_for_reevaluation_instead_of_hiding_or_forgetting_the_decision() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    assert!(server
        .review("duplicate", &id, "ignored", "api pública")
        .starts_with("HTTP/1.1 303"));
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        format!(
            "{}\n// changed source\n",
            fs::read_to_string(&file).unwrap()
        ),
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
            .replace("\n// changed source\n", ""),
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
    // A new decision on the changed code replaces the stale one and keeps both in the history.
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
    let history = reviews::Reviews::load(&root.path().canonicalize().unwrap()).unwrap();
    assert_eq!(history.history(Kind::Duplicate, &id).len(), 2);
}

#[test]
fn adr_015_decisions_are_honoured_and_the_old_file_is_never_rewritten() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = fingerprint_like_adr_015(&server.site.borrow(), 0);
    assert_eq!(
        key,
        server.site.borrow().duplicate_keys[0].fingerprint,
        "the fingerprint is the ADR-015 key"
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
        Some(State::NeedsReevaluation { .. })
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
        !reviews_file(root.path()).exists(),
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
    let saved = fs::read_to_string(reviews_file(root.path())).unwrap();
    assert!(saved.lines().all(|line| !line.starts_with("Set-Cookie")));
    assert!(saved.contains(&"x".repeat(500)) && !saved.contains(&"x".repeat(501)));
}

#[test]
fn a_failed_review_save_is_visible_and_does_not_change_the_page() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    fs::write(
        root.path().join(".mct-index/reviews.lock"),
        "another writer",
    )
    .unwrap();
    let response = server.review("duplicate", &id, "false_positive", "");
    assert!(
        response.starts_with("HTTP/1.1 500") && response.contains("No se aplicó ningún cambio"),
        "{response}"
    );
    assert!(server
        .get("duplicados")
        .contains(&format!("duplicado?k={id}")));
    assert!(!reviews_file(root.path()).exists());
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
        settings.contains(".mct-index/reviews.toon") && settings.contains("duplicate-reviews.toon")
    );
    assert!(!reviews_file(root.path()).exists());
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
    let lock = root.path().join(".mct-index/reviews.lock");
    fs::write(&lock, "x").unwrap();
    let failed = server.form(
        "review",
        &server.review_body("dead", &id, "ignored", &[]),
        origin,
    );
    assert!(
        failed.starts_with("HTTP/1.1 500") && failed.contains("No change was applied"),
        "{failed}"
    );
    fs::remove_file(lock).unwrap();
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
