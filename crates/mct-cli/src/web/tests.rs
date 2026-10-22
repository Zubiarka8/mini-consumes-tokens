//! Public index APIs plus full HTTP wire parsing/response tests, without sockets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let body = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
    fs::write(root.path().join("src/a.rs"), format!("pub fn alpha{body}")).unwrap();
    fs::write(root.path().join("src/b.rs"), format!("pub fn beta{body}")).unwrap();
    index(root.path());
    root
}

#[test]
fn local_web_links_both_views_and_keeps_the_index_read_only() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    assert_eq!(server.address, "127.0.0.1:8787");
    let overview = server.get("");
    assert!(overview.starts_with("HTTP/1.1 200"), "{overview}");
    assert!(overview.contains("href=\"atlas\"") && overview.contains("href=\"duplicates\""));
    assert!(
        overview.contains("default-src 'none'") && overview.contains("Cache-Control: no-store")
    );
    assert!(server.get("atlas").contains("atlas?dir=src&amp;q="));
    let atlas = server.get("atlas?dir=src");
    assert!(atlas.contains("file?path=src%2Fa.rs"), "{atlas}");
    assert!(atlas.contains("file?path=src%2Fb.rs"));
    let file = server.get("file?path=src%2Fa.rs");
    assert!(file.contains("alpha") && file.contains("duplicates?path=src%2Fa.rs"));
    assert!(file.contains("src/a.rs:1"));
    assert!(
        !file.contains("total +="),
        "atlas must contain metadata only"
    );
    let duplicates = server.get("duplicates?path=src%2Fa.rs");
    assert!(duplicates.contains("Group 1"), "{duplicates}");
    assert!(duplicates.contains("file?path=src%2Fb.rs"));
    let group = server.get("group?id=0");
    assert!(group.contains("Open in atlas") && group.contains("Full alignment"));
    let toon = server.get("duplicates.toon");
    assert!(toon.contains("text/plain") && toon.contains("members[2]"));
    assert!(server.get("style.css").contains("text/css"));
    assert_eq!(original, fs::read(database).unwrap());
    assert!(!root.path().join(".mct-index/obsidian").exists());
}

#[test]
fn web_regenerates_obsidian_and_survives_a_vault_conflict() {
    let root = project();
    {
        let server = Server::start(root.path(), false);
        assert!(server.get("").contains("200 OK"));
        assert!(root.path().join(".mct-index/obsidian/index.md").is_file());
    }
    let note = root.path().join(".mct-index/obsidian/index.md");
    fs::write(&note, "Personal edited map").unwrap();
    let server = Server::start(root.path(), false);
    assert!(server
        .get("")
        .contains("Cannot regenerate the Obsidian vault"));
    assert!(server.get("atlas").contains("200 OK"));
    assert_eq!(fs::read_to_string(note).unwrap(), "Personal edited map");
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
        "file?path=..%2Fsecret",
        "missing",
    ] {
        assert!(server.get(path).starts_with("HTTP/1.1 404"), "{path}");
    }
    assert!(server.get("atlas?page=oops").starts_with("HTTP/1.1 400"));
    assert!(server
        .get("atlas?page=1&page=2")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .get("file?path=%3Cscript%3E")
        .starts_with("HTTP/1.1 404"));
    let injected = server.get("atlas?q=%22%3E%3Cscript%3Ealert%281%29%3C%2Fscript%3E");
    assert!(!injected.contains("<script>"));
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
    assert!(head.starts_with("HTTP/1.1 200") && head.ends_with("\r\n\r\n"));
    let huge = "a".repeat(17_000);
    assert!(server
        .raw(&format!(
            "GET {} HTTP/1.1\r\n{host}\r\nX-Large: {huge}\r\n\r\n",
            server.prefix
        ))
        .starts_with("HTTP/1.1 431"));
}

#[test]
fn atlas_pagination_and_encoded_paths_keep_every_entry_accessible() {
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
    fs::create_dir(root.path().join("src2")).unwrap();
    fs::write(root.path().join("src2/outside.rs"), "pub fn outside() {}\n").unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let first = server.get("atlas?dir=src");
    assert!(first.contains("Page 1 of 2") && first.contains("102 items"));
    assert!(first.contains("file?path=src%2Fa+%26+%C3%BC.rs"));
    assert!(!first.contains("outside.rs"));
    let second = server.get("atlas?dir=src&page=1");
    assert!(second.contains("file_100.rs") && second.contains("Page 2 of 2"));
    let first = server.get("file?path=src%2Fa+%26+%C3%BC.rs");
    assert!(first.contains("symbol_0") && first.contains("src/a &amp; ü.rs"));
    assert!(!first.contains("<code>symbol_204</code>"));
    let last = server.get("file?path=src%2Fa+%26+%C3%BC.rs&sp=2");
    assert!(last.contains("<code>symbol_204</code>"), "{last}");
    assert!(last.contains("Page 3 of 3"));
    let searched = server.get("atlas?q=symbol_204");
    assert!(searched.contains("1 matching files") && searched.contains("matches symbol_204"));
    assert!(searched.contains("file?path=src%2Fa+%26+%C3%BC.rs"));
    let searched = server.get("atlas?dir=src&q=symbol_204");
    assert!(searched.contains("file?path=src%2Fa+%26+%C3%BC.rs"));
    assert!(!searched.contains("file_100.rs"));
}

#[test]
fn generated_navigation_has_no_broken_links_and_wire_lengths_are_exact() {
    let root = project();
    let server = Server::start(root.path(), true);
    let mut pending = vec![String::new()];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(target) = pending.pop() {
        if !visited.insert(target.clone()) {
            continue;
        }
        assert!(visited.len() < 100, "unexpected navigation expansion");
        let response = server.get(&target);
        assert!(response.starts_with("HTTP/1.1 200"), "{target}: {response}");
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(length, body.len());
        if !headers.contains("text/html") {
            continue;
        }
        for value in body.split("href=\"").skip(1) {
            let href = value.split('"').next().unwrap().replace("&amp;", "&");
            if href.starts_with('#') {
                continue;
            }
            let target = if href == "./" { "".to_string() } else { href };
            assert!(!target.contains("://"), "no external resources");
            pending.push(target);
        }
    }
    assert!(visited.iter().any(|target| target.starts_with("group?")));
    assert!(visited.contains("file?path=src%2Fb.rs"));
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
fn file_relations_link_in_both_directions_and_snapshot_refresh_is_explicit() {
    let root = project();
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("use crate::b::beta;\n{original}")).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    assert!(server
        .get("file?path=src%2Fa.rs")
        .contains("<h2>Resolved imports</h2><ul><li><a href=\"file?path=src%2Fb.rs\""));
    assert!(server
        .get("file?path=src%2Fb.rs")
        .contains("<h2>Imported by</h2><ul><li><a href=\"file?path=src%2Fa.rs\""));
    fs::write(&file, format!("{original}\npub fn newly_indexed() {{}}\n")).unwrap();
    index(root.path());
    assert!(!server.get("file?path=src%2Fa.rs").contains("newly_indexed"));
    let refreshed = Server::start(root.path(), true);
    assert!(refreshed
        .get("file?path=src%2Fa.rs")
        .contains("newly_indexed"));
}

impl Server {
    fn post(&self, key: &str, action: &str) -> String {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("csrf", self.site.borrow().csrf.as_str()),
                ("key", key),
                ("action", action),
            ])
            .finish();
        self.form(&body, "Origin: http://127.0.0.1:8787\r\n")
    }
    fn form(&self, body: &str, headers: &str) -> String {
        self.raw(&format!("POST /secret/review HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }
}

#[test]
fn dismiss_and_restore_persist_without_mutating_the_analysis_or_index() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    let raw = server.get("duplicates.toon");
    let key = server.site.borrow().review_keys[0].clone();
    assert!(server.get("duplicates").contains("Discard group"));
    let response = server.post(&key, "false-positive");
    assert!(response.starts_with("HTTP/1.1 303") && response.contains("Location: duplicates\r\n"));
    let active = server.get("duplicates");
    assert!(
        !active.contains("group?id=0")
            && active.contains("To review (0)")
            && active.contains("Discarded (1)")
    );
    let discarded = server.get("duplicates?view=discarded");
    assert!(
        discarded.contains("group?id=0")
            && discarded.contains("False positive")
            && discarded.contains("Restore to review")
    );
    assert_eq!(server.get("duplicates.toon"), raw);
    let restarted = Server::start(root.path(), true);
    assert!(!restarted.get("duplicates").contains("group?id=0"));
    assert!(restarted
        .get("file?path=src%2Fa.rs")
        .contains("Duplicate groups containing this file</a> (0)"));
    assert!(restarted.post(&key, "restore").starts_with("HTTP/1.1 303"));
    assert!(restarted.get("duplicates").contains("group?id=0"));
    assert!(!Server::start(root.path(), true)
        .get("duplicates?view=discarded")
        .contains("group?id=0"));
    assert_eq!(fs::read(database).unwrap(), original);
}

#[test]
fn review_identity_ignores_rank_but_changes_with_code_or_group_members() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    assert!(server.post(&key, "dismissed").starts_with("HTTP/1.1 303"));
    {
        let site = server.site.borrow();
        let mut group = site.duplicates.groups[0].clone();
        group.members.reverse();
        assert_eq!(
            super::reviews::group_keys(
                &[group.clone()],
                &site.navigation,
                site.duplicates.options.normalization
            )
            .unwrap()[0],
            key
        );
        group.members.pop();
        assert_ne!(
            super::reviews::group_keys(
                &[group],
                &site.navigation,
                site.duplicates.options.normalization
            )
            .unwrap()[0],
            key
        );
    }
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
    assert_ne!(changed.site.borrow().review_keys[0], key);
    assert!(changed.get("duplicates").contains("group?id=0"));
}

#[test]
fn review_posts_require_origin_csrf_valid_action_and_current_group_identity() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    let csrf = server.site.borrow().csrf.clone();
    let body = format!("csrf={csrf}&key={key}&action=false-positive");
    assert!(server.form(&body, "").starts_with("HTTP/1.1 403"));
    assert!(server
        .form(&body, "Origin: https://evil.example\r\n")
        .starts_with("HTTP/1.1 403"));
    assert!(server
        .form(
            &body.replace(&csrf, "bad"),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 403"));
    assert!(server
        .post(&"0".repeat(64), "false-positive")
        .starts_with("HTTP/1.1 404"));
    assert!(server
        .post(&key, "erase-source")
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .form(
            &(body.clone() + "&action=restore"),
            "Origin: http://127.0.0.1:8787\r\n"
        )
        .starts_with("HTTP/1.1 400"));
    assert!(server
        .get(&format!("review?{body}"))
        .starts_with("HTTP/1.1 404"));
    assert!(server
        .get("duplicates?view=unknown")
        .starts_with("HTTP/1.1 400"));
    let huge = "a".repeat(4097);
    assert!(server
        .form(&huge, "Origin: http://127.0.0.1:8787\r\n")
        .starts_with("HTTP/1.1 413"));
    assert!(!root
        .path()
        .join(".mct-index/duplicate-reviews.toon")
        .exists());
    assert!(server.get("duplicates").contains("group?id=0"));
}

#[test]
fn a_failed_review_save_is_visible_and_does_not_hide_the_group() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    fs::write(
        root.path().join(".mct-index/duplicate-reviews.lock"),
        "another writer",
    )
    .unwrap();
    let response = server.post(&key, "false-positive");
    assert!(response.starts_with("HTTP/1.1 500") && response.contains("No changes were applied"));
    assert!(server.get("duplicates").contains("group?id=0"));
    assert!(!root
        .path()
        .join(".mct-index/duplicate-reviews.toon")
        .exists());
}

#[test]
fn current_ignore_rules_hide_stale_index_rows_from_both_web_views_and_the_vault() {
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
        .any(|file| file.relative_path == "src/b.rs"));
    assert!(!site
        .navigation
        .symbols
        .iter()
        .any(|symbol| symbol.relative_path == "src/b.rs"));
    assert!(site.navigation.imports.is_empty());
    assert!(site.duplicates.groups.is_empty() && site.duplicates.skipped.is_empty());
    drop(site);
    assert!(!server.get("atlas?dir=src").contains("src/b.rs"));
    assert!(server
        .get("file?path=src%2Fb.rs")
        .starts_with("HTTP/1.1 404"));
    let root_path = root.path().canonicalize().unwrap();
    let database = root_path.join(".mct-index/index.sqlite3");
    let raw = mct_index::Index::open_existing_read_only(
        &root_path,
        &database,
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    assert!(raw
        .indexed_files()
        .unwrap()
        .iter()
        .any(|file| file.relative_path == "src/b.rs"));
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
fn review_state_is_applied_before_group_pagination_in_both_views() {
    let root = tempfile::tempdir().unwrap();
    for side in ["left", "right"] {
        let mut source = String::new();
        for count in 8..33 {
            source.push_str(&format!(
                "pub fn {side}_{count}(mut value: i64) -> i64 {{\n"
            ));
            source.push_str(&"value += 1;\n".repeat(count));
            source.push_str("value\n}\n");
        }
        fs::write(root.path().join(format!("{side}.rs")), source).unwrap();
    }
    index(root.path());
    let server = Server::start(root.path(), true);
    let keys = server.site.borrow().review_keys.clone();
    assert_eq!(keys.len(), 25);
    assert!(server.get("duplicates").contains("Page 1 of 2"));
    for key in keys.iter().take(6) {
        assert!(server.post(key, "dismissed").starts_with("HTTP/1.1 303"));
    }
    let active = server.get("duplicates");
    assert!(active.contains("To review (19)") && active.contains("Page 1 of 1"));
    assert!(!active.contains("group?id=0"));
    let discarded = server.get("duplicates?view=discarded");
    assert!(discarded.contains("Discarded (6)") && discarded.contains("group?id=0"));
    assert!(discarded.contains("Restore to review"));
}

#[test]
fn filters_are_explained_and_survive_review_actions() {
    let root = project();
    let server = Server::start(root.path(), true);
    let found = server.get("atlas?q=BETA");
    assert!(found.contains("1 matching files") && found.contains("Clear search"));
    assert!(found.contains("file?path=src%2Fb.rs") && !found.contains("file?path=src%2Fa.rs"));
    assert!(server
        .get("atlas?q=nothing-here")
        .contains("No matching entries."));
    let key = server.site.borrow().review_keys[0].clone();
    let filtered = server.get("duplicates?q=alpha");
    assert!(filtered.contains("Clear all filters") && filtered.contains("aria-current=\"page\""));
    assert!(filtered.contains("name=\"back\" value=\"q=alpha&amp;view=active\""));
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", server.site.borrow().csrf.clone()),
            ("key", key),
            ("action", "dismissed".to_string()),
            ("back", "q=alpha&view=active&evil=1".to_string()),
        ])
        .finish();
    let response = server.form(&body, "Origin: http://127.0.0.1:8787\r\n");
    assert!(
        response.contains("Location: duplicates?q=alpha&view=active\r\n"),
        "{response}"
    );
}
