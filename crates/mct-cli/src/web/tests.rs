//! Dashboard API, static assets and full HTTP wire parsing/response tests, without sockets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{http, Site};
use std::collections::BTreeMap;
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
            prefix: "/",
        }
    }
    fn raw(&self, request: &str) -> String {
        let mut wire = Wire {
            input: Cursor::new(request.as_bytes().to_vec()),
            output: Vec::new(),
            timeout: false,
        };
        http::serve_connection(&mut wire, 8787, &mut self.site.borrow_mut()).unwrap();
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

impl Server {
    fn body(response: &str) -> &str {
        response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
    }
    fn json(&self, path: &str) -> serde_json::Value {
        let response = self.get(path);
        assert!(response.starts_with("HTTP/1.1 200"), "{path}: {response}");
        serde_json::from_str(Self::body(&response)).unwrap()
    }
    fn post_review(&self, body: &str, headers: &str) -> String {
        self.raw(&format!("POST /api/review HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }
    /// Posts a decision based on the revision the API shows right now.
    fn decide(&self, key: &str, state: Option<&str>, note: &str) -> String {
        let csrf = self.site.borrow().csrf.clone();
        let revision = self.json(&format!("api/finding?key={key}"))["revision"].clone();
        let body = serde_json::json!({ "csrf": csrf, "key": key, "state": state, "note": note, "revision": revision })
            .to_string();
        self.post_review(&body, "Origin: http://127.0.0.1:8787\r\n")
    }
}

#[test]
fn dashboard_serves_static_app_and_a_read_only_json_api() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    let page = server.get("");
    assert!(
        page.starts_with("HTTP/1.1 200") && page.contains("text/html"),
        "{page}"
    );
    assert!(page.contains("type=\"module\"") && page.contains("lang=\"es\""));
    for header in [
        "script-src 'self'",
        "default-src 'none'",
        "Cache-Control: no-store",
        "frame-ancestors 'none'",
    ] {
        assert!(page.contains(header), "{header}");
    }
    assert!(!page.contains("unsafe-inline") && !page.contains("unsafe-eval"));
    assert!(server.get("app.css").contains("text/css"));
    for module in [
        "main",
        "core",
        "i18n",
        "components",
        "charts",
        "code",
        "graph",
        "markdown",
        "palette",
        "page-dashboard",
        "ai",
        "i18n-en",
        "issues",
        "page-symbols",
        "page-findings",
        "page-search",
        "page-deps",
        "page-stats",
        "page-notes",
        "page-workspace",
        "page-settings",
    ] {
        assert!(
            server
                .get(&format!("js/{module}.js"))
                .contains("text/javascript"),
            "{module}"
        );
    }

    let meta = server.json("api/meta");
    assert_eq!(meta["capabilities"]["search"]["semantic"], false);
    assert_eq!(meta["capabilities"]["history"], false);
    assert_eq!(meta["indexing"]["state"], "ready");
    let summary = server.json("api/summary");
    assert_eq!(summary["files"], 2);
    assert_eq!(summary["duplicates"]["groups"], 1);
    assert_eq!(summary["deadCode"]["candidates"], 2);
    assert!(summary["lastIndexDurationMs"].is_null() && summary["history"].is_null());

    let symbols = server.json("api/symbols?q=alpha");
    assert_eq!(symbols["total"], 1);
    let item = &symbols["items"][0];
    assert_eq!(item["path"], "src/a.rs");
    assert_eq!(item["duplicateGroups"], 1);
    assert_eq!(item["deadCandidate"], true);
    let detail = server.json(&format!("api/symbol?id={}", item["id"]));
    assert!(detail["code"]["lines"][0]
        .as_str()
        .unwrap()
        .contains("pub fn alpha"));
    assert_eq!(detail["duplicates"].as_array().unwrap().len(), 1);
    assert!(detail["dead"].is_object());
    assert_eq!(
        server.json("api/files")["files"].as_array().unwrap().len(),
        2
    );

    let findings = server.json("api/findings");
    assert_eq!(findings["total"], 3);
    let duplicates = server.json("api/findings?type=duplicate");
    assert_eq!(duplicates["total"], 1);
    let group = &duplicates["items"][0];
    assert_eq!(group["state"], "unreviewed");
    assert_eq!(group["verdict"], "real");
    assert_eq!(group["scope"], "cross-file");
    assert!(
        group["similarity"].is_null(),
        "no invented similarity score"
    );
    let key = group["key"].as_str().unwrap();
    let finding = server.json(&format!("api/finding?key={key}"));
    assert_eq!(finding["members"].as_array().unwrap().len(), 2);
    assert!(finding["analysis"].as_str().unwrap().contains("alpha"));
    let dead = server.json("api/findings?type=dead-code&q=beta");
    assert!(["high", "medium", "low"].contains(&dead["items"][0]["confidence"].as_str().unwrap()));
    assert_eq!(dead["items"][0]["zone"], "source");
    assert!(dead["items"][0]["severity"].is_null());
    let dead_detail = server.json(&format!(
        "api/finding?key={}",
        dead["items"][0]["key"].as_str().unwrap()
    ));
    assert!(dead_detail["detail"]["code"]["lines"][0]
        .as_str()
        .unwrap()
        .contains("beta"));

    let search = server.json("api/search?q=alpha");
    assert_eq!(search["semantic"], false);
    assert_eq!(search["groups"]["symbols"]["items"][0]["path"], "src/a.rs");
    assert!(search["groups"]["duplicates"]["total"].as_u64().unwrap() >= 1);
    // With no query the search lists everything instead of nothing; literal text needs a query.
    let everything = server.json("api/search?q=");
    assert!(everything["groups"]["symbols"]["total"].as_u64().unwrap() >= 1);
    assert!(everything["groups"]["files"]["total"].as_u64().unwrap() >= 1);
    assert!(everything["groups"].get("code").is_none());
    assert_eq!(
        server.json("api/dependencies?level=file&all=1")["truncated"],
        false
    );
    let deps = server.json("api/dependencies?level=file");
    assert_eq!(deps["edgeKinds"][0], "imports");
    let stats = server.json("api/stats");
    assert!(stats["measurements"]["tokens"].is_null());
    assert!(stats["measurements"]["apiLatency"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["route"] == "symbols"));
    assert_eq!(original, fs::read(database).unwrap());
    assert!(!root.path().join(".mct-index/obsidian").exists());
}

#[test]
fn a_truncated_dependency_graph_still_shows_every_file_in_a_cycle() {
    let root = tempfile::tempdir().unwrap();
    // 160 files that import three hubs outrank a two-file cycle by degree, so a
    // graph cut at 150 nodes would drop the cycle if it only ranked by degree.
    for hub in 1..=3 {
        fs::write(root.path().join(format!("hub{hub}.py")), "X = 1\n").unwrap();
    }
    for id in 0..160 {
        fs::write(
            root.path().join(format!("m{id:03}.py")),
            "import hub1\nimport hub2\nimport hub3\n",
        )
        .unwrap();
    }
    fs::write(root.path().join("z_cycle_a.py"), "import z_cycle_b\n").unwrap();
    fs::write(root.path().join("z_cycle_b.py"), "import z_cycle_a\n").unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let deps = server.json("api/dependencies?level=file&limit=150");
    assert_eq!(deps["truncated"], true);
    assert_eq!(deps["nodes"].as_array().unwrap().len(), 150);
    let all = server.json("api/dependencies?level=file&all=1");
    assert_eq!(all["truncated"], false);
    assert_eq!(all["nodes"].as_array().unwrap().len(), 165);
    assert_eq!(deps["cycles"].as_array().unwrap().len(), 1);
    let in_cycle: Vec<&str> = deps["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["cycle"] == true)
        .map(|node| node["id"].as_str().unwrap())
        .collect();
    assert_eq!(in_cycle, ["z_cycle_a.py", "z_cycle_b.py"]);
}

#[test]
fn source_previews_only_read_indexed_files_inside_the_project() {
    let root = project();
    fs::write(root.path().join("secret.txt"), "TOP SECRET\n").unwrap();
    let server = Server::start(root.path(), true);
    let source = server.json("api/source?path=src%2Fa.rs&start=1&end=2");
    assert_eq!(source["lines"].as_array().unwrap().len(), 2);
    for path in [
        "secret.txt",
        "..%2Fsecret.txt",
        "%2Fetc%2Fpasswd",
        ".mct-index%2Findex.sqlite3",
        "src%2Fmissing.rs",
        "src",
    ] {
        let response = server.get(&format!("api/source?path={path}"));
        assert!(response.starts_with("HTTP/1.1 404"), "{path}: {response}");
        assert!(!response.contains("TOP SECRET"));
    }
    let huge = server.json("api/source?path=src%2Fa.rs&start=1&end=99999");
    assert!(huge["lines"].as_array().unwrap().len() <= 400);
}

#[test]
fn source_previews_refuse_a_file_changed_since_it_was_indexed() {
    let root = project();
    let server = Server::start(root.path(), true);
    let before = server.json("api/source?path=src%2Fa.rs&start=1&end=2");
    assert_eq!(before["lines"].as_array().unwrap().len(), 2);
    fs::write(
        root.path().join("src/a.rs"),
        "// one line added\npub fn alpha() {}\n",
    )
    .unwrap();
    let response = server.get("api/source?path=src%2Fa.rs&start=1&end=2");
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(
        response.contains("changed since it was indexed"),
        "{response}"
    );
    assert!(!response.contains("one line added"));
    let untouched = server.json("api/source?path=src%2Fb.rs&start=1&end=2");
    assert_eq!(untouched["lines"].as_array().unwrap().len(), 2);
}

#[test]
fn api_lists_are_bounded_paged_and_filterable() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let source: String = (0..120)
        .map(|id| format!("pub fn symbol_{id}() {{}}\n"))
        .collect();
    fs::write(root.path().join("src/many.rs"), source).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let first = server.json("api/symbols?size=50&kind=function");
    assert_eq!(first["total"], 120);
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    let last = server.json("api/symbols?size=50&page=99&kind=function");
    assert_eq!(last["page"], 2, "out-of-range pages clamp to the last page");
    assert_eq!(last["items"].as_array().unwrap().len(), 20);
    assert_eq!(
        server.json("api/symbols?size=9999")["items"]
            .as_array()
            .unwrap()
            .len(),
        121
    );
    assert_eq!(
        server.json("api/symbols?q=SYMBOL_11&kind=function")["total"],
        11
    );
    assert_eq!(server.json("api/symbols?kind=class")["total"], 0);
    assert_eq!(
        server.json("api/symbols?language=rust&dir=src&kind=function")["total"],
        120
    );
    assert_eq!(
        server.json("api/symbols?dir=sr")["total"],
        0,
        "directory filters match whole path segments"
    );
    assert_eq!(
        server.json("api/symbols?finding=dead-code&sort=refs&order=desc")["total"],
        120
    );
}

#[test]
fn symbol_facets_count_what_the_other_filters_leave() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let source: String = (0..20)
        .map(|id| format!("pub fn symbol_{id}() {{}}\n"))
        .collect();
    fs::write(root.path().join("src/many.rs"), source).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let count = |facets: &serde_json::Value, name: &str| {
        facets
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == name)
            .map_or(0, |row| row[1].as_u64().unwrap())
    };
    let all = server.json("api/symbols?size=1");
    assert_eq!(count(&all["facets"]["kind"], "function"), 20);
    // Dead-code candidates: the counts follow the filter, not just the total.
    let dead = server.json("api/symbols?size=1&finding=dead-code");
    assert_eq!(count(&dead["facets"]["kind"], "function"), 20);
    assert_eq!(dead["total"], count(&dead["facets"]["language"], "rust"));
    // No duplicates in this project: every facet is empty, not left at 20.
    let none = server.json("api/symbols?size=1&finding=duplicate");
    assert_eq!(none["total"], 0);
    for facet in ["kind", "language", "ext"] {
        assert!(
            none["facets"][facet].as_array().unwrap().is_empty(),
            "{facet}"
        );
    }
    // A facet ignores its own filter, so the other choices stay listed.
    let only_functions = server.json("api/symbols?size=1&kind=function");
    assert_eq!(
        count(&only_functions["facets"]["kind"], "function"),
        count(&all["facets"]["kind"], "function")
    );
    assert_eq!(
        only_functions["facets"]["kind"].as_array().unwrap().len(),
        all["facets"]["kind"].as_array().unwrap().len()
    );
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
        "api/unknown",
        "api",
        "js/../app.css",
        "js/missing.js",
        "missing",
    ] {
        assert!(server.get(path).starts_with("HTTP/1.1 404"), "{path}");
    }
    for path in ["api/symbols?page=1&page=2", "api/symbols?q=a&q=b"] {
        assert!(server.get(path).starts_with("HTTP/1.1 400"), "{path}");
    }
    let hostile = server.get("api/symbols?q=%22%3E%3Cscript%3Ealert%281%29%3C%2Fscript%3E");
    assert!(hostile.contains("application/json") && hostile.starts_with("HTTP/1.1 200"));
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
        .raw(&format!("GET api/meta HTTP/1.1\r\n{host}\r\n\r\n"))
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
fn generated_responses_have_exact_lengths() {
    let root = project();
    let server = Server::start(root.path(), true);
    for path in [
        "",
        "app.css",
        "js/main.js",
        "api/meta",
        "api/summary",
        "api/findings",
        "api/dependencies",
    ] {
        let response = server.get(path);
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(length, body.len(), "{path}");
    }
}

#[test]
fn incomplete_headers_and_timeouts_are_bounded_without_rendering_content() {
    let root = project();
    let server = Server::start(root.path(), true);
    assert!(server.raw("GET / HTTP/1.1\r\n").starts_with("HTTP/1.1 400"));
    let mut wire = Wire {
        input: Cursor::new(Vec::new()),
        output: Vec::new(),
        timeout: true,
    };
    http::serve_connection(&mut wire, 8787, &mut server.site.borrow_mut()).unwrap();
    let response = String::from_utf8(wire.output).unwrap();
    assert!(response.starts_with("HTTP/1.1 408"));
    assert!(!response.contains("alpha"));
}

#[test]
fn review_decisions_persist_reopen_and_keep_an_audit_trail() {
    let root = project();
    let database = root.path().join(".mct-index/index.sqlite3");
    let original = fs::read(&database).unwrap();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    let before = server.get("api/findings?type=duplicate");
    let response = server.decide(&key, Some("false-positive"), "generated twin");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let saved: serde_json::Value = serde_json::from_str(Server::body(&response)).unwrap();
    assert_eq!(saved["state"], "false-positive");
    assert_eq!(saved["history"].as_array().unwrap().len(), 1);
    let listed = server.json("api/findings?type=duplicate&state=false-positive");
    assert_eq!(
        listed["total"], 1,
        "false positives stay listed and recoverable"
    );
    assert_eq!(listed["items"][0]["note"], "generated twin");
    assert_eq!(
        server.json("api/summary")["findings"]["states"]["false-positive"],
        1
    );
    assert_eq!(
        server.json("api/summary")["recent"][0]["to"],
        "false-positive"
    );
    let reopened = server.decide(&key, None, "code changed");
    assert!(reopened.starts_with("HTTP/1.1 200"));
    let after = server.json(&format!("api/finding?key={key}"));
    assert_eq!(after["state"], "unreviewed");
    assert_eq!(
        after["history"].as_array().unwrap().len(),
        2,
        "reopening never erases history"
    );
    assert!(before
        .split_once("\r\n\r\n")
        .unwrap()
        .1
        .contains("unreviewed"));
    // A new process sees the same decisions and the same dead-code identity.
    let dead_key = server.site.borrow().dead[0].key.clone();
    assert!(server
        .decide(&dead_key, Some("in-review"), "")
        .starts_with("HTTP/1.1 200"));
    let restarted = Server::start(root.path(), true);
    assert_eq!(
        restarted.json("api/findings?type=dead-code&state=in-review")["total"],
        1
    );
    assert_eq!(
        restarted.json(&format!("api/finding?key={key}"))["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        original,
        fs::read(database).unwrap(),
        "reviews never touch the index"
    );
}

#[test]
fn review_posts_require_origin_csrf_valid_state_and_current_identity() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    let csrf = server.site.borrow().csrf.clone();
    let origin = "Origin: http://127.0.0.1:8787\r\n";
    let body = |value: serde_json::Value| value.to_string();
    let revision = server.json(&format!("api/finding?key={key}"))["revision"].clone();
    let good =
        serde_json::json!({ "csrf": csrf, "key": key, "state": "confirmed", "revision": revision });
    assert!(
        server
            .post_review(&body(good.clone()), "")
            .starts_with("HTTP/1.1 403"),
        "origin is mandatory"
    );
    assert!(server
        .post_review(&body(good.clone()), "Origin: https://evil.example\r\n")
        .starts_with("HTTP/1.1 403"));
    let mut wrong = good.clone();
    wrong["csrf"] = "nope".into();
    assert!(server
        .post_review(&body(wrong), origin)
        .starts_with("HTTP/1.1 403"));
    for (patch, status) in [
        (serde_json::json!({ "key": "f".repeat(64) }), 404),
        (serde_json::json!({ "key": "../../etc/passwd" }), 404),
        (serde_json::json!({ "state": "deleted" }), 400),
        (serde_json::json!({ "state": 7 }), 400),
        (serde_json::json!({ "extra": true }), 400),
        (serde_json::json!({ "revision": null }), 400),
        (serde_json::json!({ "note": "x".repeat(501) }), 400),
    ] {
        let mut value = good.clone();
        for (field, content) in patch.as_object().unwrap() {
            value[field] = content.clone();
        }
        let response = server.post_review(&body(value), origin);
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "{patch}: {response}"
        );
    }
    assert!(server
        .post_review("{not json", origin)
        .starts_with("HTTP/1.1 400"));
    let form = server.raw(&format!("POST /api/review HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{origin}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: 1\r\n\r\nx"));
    assert!(form.starts_with("HTTP/1.1 400"));
    assert!(server
        .post_review(&body(good), origin)
        .starts_with("HTTP/1.1 200"));
    let wrong_path = server.raw(&format!("POST /review HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{origin}Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"));
    assert!(wrong_path.starts_with("HTTP/1.1 405"));
}

#[test]
fn a_failed_review_save_is_reported_and_changes_nothing() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    // Another writer is alive and holds the OS lock until the end of the test.
    let other_writer =
        fs::File::create(root.path().join(".mct-index/review-decisions.lock")).unwrap();
    other_writer.try_lock().unwrap();
    let response = server.decide(&key, Some("dismissed"), "reason");
    assert!(
        response.starts_with("HTTP/1.1 503")
            && response.contains("No changes were applied")
            && response.contains("review-decisions.lock"),
        "{response}"
    );
    assert_eq!(
        server.json("api/findings?type=duplicate")["items"][0]["state"],
        "unreviewed"
    );
    for name in ["review-decisions.toon", "duplicate-reviews.toon"] {
        assert!(!root.path().join(".mct-index").join(name).exists());
    }
    drop(other_writer);
    let response = server.decide(&key, Some("dismissed"), "reason");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the save succeeds once the other writer is gone: {response}"
    );
}

#[test]
fn review_identity_follows_group_members_and_file_content() {
    let root = project();
    let before = Server::start(root.path(), true);
    let key = before.site.borrow().review_keys[0].clone();
    assert!(before
        .decide(&key, Some("false-positive"), "")
        .starts_with("HTTP/1.1 200"));
    let dead = before
        .site
        .borrow()
        .dead
        .iter()
        .map(|d| d.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(dead.len(), 2);
    assert!(dead.iter().all(|k| *k != key));
    assert_eq!(
        Server::start(root.path(), true).site.borrow().review_keys[0],
        key,
        "keys are stable between runs"
    );
    let file = root.path().join("src/b.rs");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("total += 8;", "total += 9;")).unwrap();
    index(root.path());
    let changed = Server::start(root.path(), true);
    // Same members, changed code: the same finding, shown again until
    // someone decides about the new code.
    assert_eq!(
        changed.site.borrow().review_keys,
        std::slice::from_ref(&key)
    );
    let row = changed.json(&format!("api/finding?key={key}"));
    assert_eq!(row["state"], "needs-reevaluation");
    assert_eq!(row["previousState"], "false-positive");
}

#[test]
fn review_workspace_bundle_is_served_from_the_binary() {
    let root = project();
    let server = Server::start(root.path(), true);
    let script = server.get("js/review.js");
    assert!(script.starts_with("HTTP/1.1 200"), "{script}");
    assert!(script.contains("Content-Type: text/javascript"));
    assert!(script.contains("mountReview"));
    let style = server.get("review.css");
    assert!(style.starts_with("HTTP/1.1 200") && style.contains("Content-Type: text/css"));
    // Nothing that needs the network, and the toasts' rules ship in the
    // stylesheet: the build removes sonner's injected <style>, which
    // `style-src 'self'` would block.
    let (script, style) = (Server::body(&script), Server::body(&style));
    for remote in [
        "import(\"http",
        "fetch(\"http",
        "import(`http",
        "fetch(`http",
    ] {
        assert!(!script.contains(remote), "{remote}");
    }
    assert!(!style.contains("url(http") && !style.contains("@import"));
    let toaster = "[data-sonner-toaster][dir=ltr]";
    assert!(style.contains(toaster) && !script.contains(toaster));
}

#[test]
fn duplicate_detail_lists_the_detector_differences() {
    let root = project();
    let file = root.path().join("src/b.rs");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("total += 8;", "total += 9;")).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    let detail = server.json(&format!("api/finding?key={key}"));
    let evidence = &detail["evidence"];
    // Values and lines follow the member order (src/a.rs, then src/b.rs).
    let literal = evidence["differences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|difference| difference["kind"] == "literal-value")
        .expect("the changed literal is a listed difference");
    assert_eq!(literal["values"], serde_json::json!(["8", "9"]));
    assert_eq!(literal["lines"], serde_json::json!([7, 7]));
    assert!(evidence["distinct"].as_u64().unwrap() >= 1);
    assert!(evidence["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning == "literal-values-differ"));
}

/// `key → number` of every listed finding of `kind`.
fn numbers_of(list: &serde_json::Value, kind: &str) -> BTreeMap<String, u64> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == kind)
        .map(|item| {
            (
                item["key"].as_str().unwrap().to_string(),
                item["number"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn finding_numbers_are_readable_stable_and_never_reused() {
    let root = project();
    let first = Server::start(root.path(), true);
    let all = first.json("api/findings?state=&size=100");
    let duplicates = numbers_of(&all, "duplicate");
    let dead = numbers_of(&all, "dead-code");
    assert_eq!(duplicates.values().copied().collect::<Vec<_>>(), [1]);
    let mut dead_numbers: Vec<u64> = dead.values().copied().collect();
    dead_numbers.sort_unstable();
    assert_eq!(dead_numbers, [1, 2], "each kind counts on its own");
    assert!(root.path().join(".mct-index/finding-numbers.tsv").is_file());

    // Sorting, filtering and the detail view never renumber.
    let sorted = first.json("api/findings?state=&size=100&sort=title&order=desc&q=beta");
    assert!(!sorted["items"].as_array().unwrap().is_empty());
    for item in sorted["items"].as_array().unwrap() {
        let key = item["key"].as_str().unwrap();
        let expected = duplicates.get(key).or_else(|| dead.get(key)).unwrap();
        assert_eq!(item["number"].as_u64(), Some(*expected));
    }
    let (group, number) = duplicates.iter().next().unwrap();
    assert_eq!(
        first.json(&format!("api/finding?key={group}"))["number"].as_u64(),
        Some(*number)
    );
    // The page URL can name a finding by its number instead of its key.
    assert_eq!(
        first.json("api/finding?type=duplicate&number=1")["key"],
        group.as_str()
    );
    assert!(first
        .get("api/finding?type=duplicate&number=9")
        .starts_with("HTTP/1.1 404"));
    assert!(first
        .decide(group, Some("false-positive"), "kept")
        .starts_with("HTTP/1.1 200"));

    // Reopening the dashboard keeps every number.
    let reopened = Server::start(root.path(), true).json("api/findings?state=&size=100");
    assert_eq!(numbers_of(&reopened, "duplicate"), duplicates);
    assert_eq!(numbers_of(&reopened, "dead-code"), dead);

    // A third copy makes a different group: it takes the next number, and
    // the decision of #1 does not move to it.
    let body = fs::read_to_string(root.path().join("src/a.rs")).unwrap();
    fs::write(root.path().join("src/c.rs"), body.replace("alpha", "gamma")).unwrap();
    index(root.path());
    let grown_server = Server::start(root.path(), true);
    assert!(
        grown_server
            .get("api/finding?type=duplicate&number=1")
            .starts_with("HTTP/1.1 404"),
        "a finding that is gone keeps its number but has no page"
    );
    let grown = grown_server.json("api/findings?state=&size=100");
    let grown_duplicates = numbers_of(&grown, "duplicate");
    assert_eq!(grown_duplicates.values().copied().collect::<Vec<_>>(), [2]);
    let grown_group = grown_duplicates.keys().next().unwrap();
    let grown_row = grown["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == grown_group.as_str())
        .unwrap();
    assert_eq!(grown_row["state"], "unreviewed");
    let grown_dead = numbers_of(&grown, "dead-code");
    for (key, number) in &dead {
        assert_eq!(grown_dead.get(key), Some(number));
    }
    assert!(grown_dead.values().any(|number| *number == 3));

    // When the first group is detected again it gets #1 and its decision back.
    fs::remove_file(root.path().join("src/c.rs")).unwrap();
    index(root.path());
    let back = Server::start(root.path(), true);
    let back_list = back.json("api/findings?state=&size=100");
    assert_eq!(numbers_of(&back_list, "duplicate"), duplicates);
    let row = back.json(&format!("api/finding?key={group}"));
    assert_eq!(row["number"].as_u64(), Some(1));
    assert_eq!(row["state"], "false-positive");
    assert_eq!(row["note"], "kept");
}

#[test]
fn a_damaged_number_file_is_never_rewritten() {
    let root = project();
    let path = root.path().join(".mct-index/finding-numbers.tsv");
    fs::write(&path, "duplicate\tnot-a-number\tabc\n").unwrap();
    let list = Server::start(root.path(), true).json("api/findings?state=&size=100");
    assert_eq!(
        numbers_of(&list, "duplicate")
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [1],
        "numbers still work for this run"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "duplicate\tnot-a-number\tabc\n"
    );
}

#[test]
fn obsidian_notes_are_served_read_only_with_verified_links() {
    let root = project();
    {
        let server = Server::start(root.path(), false);
        let notes = server.json("api/notes");
        assert!(notes["total"].as_u64().unwrap() >= 3, "{notes}");
        let file_note = notes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["codePath"] == "src/a.rs")
            .unwrap();
        let path = file_note["path"].as_str().unwrap().replace('/', "%2F");
        let detail = server.json(&format!("api/note?path={path}"));
        assert!(detail["body"].as_str().unwrap().contains("# `src/a.rs`"));
        assert_eq!(detail["codePath"], "src/a.rs");
        assert!(detail["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "alpha"));
        assert!(!detail["backlinkList"].as_array().unwrap().is_empty());
        let graph = server.json("api/notes-graph");
        assert!(!graph["edges"].as_array().unwrap().is_empty());
        assert_eq!(graph["edgeKinds"][0], "markdown-link");
        for path in ["..%2F..%2Fsecret.txt", "%2Fetc%2Fpasswd", "missing.md"] {
            assert!(
                server
                    .get(&format!("api/note?path={path}"))
                    .starts_with("HTTP/1.1 404"),
                "{path}"
            );
        }
    }
    let note = root.path().join(".mct-index/obsidian/index.md");
    fs::write(&note, "Personal edited map").unwrap();
    let server = Server::start(root.path(), false);
    assert!(server.json("api/meta")["warning"]
        .as_str()
        .unwrap()
        .contains("Cannot regenerate the Obsidian vault"));
    assert_eq!(fs::read_to_string(note).unwrap(), "Personal edited map");
}

#[test]
fn the_dashboard_follows_a_reindex_without_a_restart() {
    let root = project();
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("use crate::b::beta;\n{original}")).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let deps = server.json("api/dependencies?level=file");
    assert!(
        deps["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["from"] == "src/a.rs" && e["to"] == "src/b.rs"),
        "{deps}"
    );
    let csrf = server.site.borrow().csrf.clone();
    let symbol = server.json("api/symbols?q=alpha")["items"][0]["id"].clone();
    let detail = server.json(&format!("api/symbol?id={symbol}"));
    assert_eq!(detail["imports"][0], "src/b.rs");
    fs::write(&file, format!("{original}\npub fn newly_indexed() {{}}\n")).unwrap();
    index(root.path());
    assert_eq!(server.json("api/symbols?q=newly_indexed")["total"], 1);
    let page = server.get("");
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_eq!(
        server.site.borrow().csrf,
        csrf,
        "a reload keeps the page's CSRF token valid"
    );
}

#[test]
fn current_ignore_rules_hide_stale_index_rows_from_the_api_and_the_vault() {
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
    {
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
    }
    assert!(!server.get("api/files").contains("src/b.rs"));
    assert!(server
        .get("api/source?path=src%2Fb.rs")
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

/// Keys that the `origin/main` dashboard (`e68aec9`, `web/reviews.rs`
/// `group_keys` and `dead_code_key`) stored for `project()` in
/// `duplicate-reviews.toon`; recorded from that code in commit `ac76568`,
/// where a test compared both implementations on this fixture.
const A2_DUPLICATE_KEY: &str = "34e41d67068c03d6043f89c14aedf32dacf3a9c4e2ef3026d50eb80fe0ed203c";
const A2_DEAD_KEYS: [&str; 2] = [
    "eea049a05a956f2b447d711fc7228f1ebbb0c659c9a784c857afea59c14161d0",
    "b1b7bafd8ab725fa534b16291e0bdf0ff404344890f18e1ee5df6d019e4f546e",
];

#[test]
fn legacy_decisions_apply_and_their_file_is_never_rewritten() {
    let root = project();
    let legacy = format!(
        "version: 2\nreviews[2]{{key,state,updated,note}}:\n  {A2_DUPLICATE_KEY},dismissed,100,generated%2C+kept\n  {},false-positive,100,\nhistory[2]{{key,from,to,at,note}}:\n  {A2_DUPLICATE_KEY},none,dismissed,100,generated%2C+kept\n  {},none,false-positive,100,\n",
        A2_DEAD_KEYS[0], A2_DEAD_KEYS[0]
    );
    let path = root.path().join(".mct-index/duplicate-reviews.toon");
    fs::write(&path, &legacy).unwrap();
    let server = Server::start(root.path(), true);
    let group = server.json("api/findings?type=duplicate")["items"][0].clone();
    assert_eq!(group["state"], "dismissed");
    assert_eq!(group["note"], "generated, kept");
    assert_eq!(group["source"], "a2");
    let states: Vec<String> = server.json("api/findings?type=dead-code")["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["state"].as_str().unwrap().to_string())
        .collect();
    assert!(states.contains(&"false-positive".to_string()), "{states:?}");
    let key = group["key"].as_str().unwrap().to_string();
    assert!(server
        .decide(&key, None, "back to the queue")
        .starts_with("HTTP/1.1 200"));
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    assert!(root
        .path()
        .join(".mct-index/review-decisions.toon")
        .exists());
    let restarted = Server::start(root.path(), true);
    let group = restarted.json(&format!("api/finding?key={key}"));
    assert_eq!(group["state"], "unreviewed");
    let sources: Vec<&str> = group["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["a2", "c3"], "history keeps both files' events");
}

#[test]
fn review_rejects_changed_source_for_dead_code_and_duplicates() {
    let root = project();
    let server = Server::start(root.path(), true);
    let duplicate = server.site.borrow().review_keys[0].clone();
    let dead = server
        .site
        .borrow()
        .dead
        .iter()
        .find(|candidate| {
            let site = server.site.borrow();
            site.navigation.symbols[candidate.symbol].name == "alpha"
        })
        .unwrap()
        .key
        .clone();
    let requests: Vec<_> = [&duplicate, &dead].into_iter().map(|key| {
        let revision = server.json(&format!("api/finding?key={key}"))["revision"].clone();
        serde_json::json!({ "csrf": server.site.borrow().csrf, "key": key, "state": "false-positive", "revision": revision }).to_string()
    }).collect();
    fs::write(root.path().join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
    for body in requests {
        let response = server.post_review(&body, "Origin: http://127.0.0.1:8787\r\n");
        assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    }
    assert!(!root
        .path()
        .join(".mct-index/review-decisions.toon")
        .exists());
}

#[test]
fn review_rejects_an_old_finding_after_reindex() {
    let root = project();
    let server = Server::start(root.path(), true);
    let keys = {
        let site = server.site.borrow();
        vec![
            site.review_keys[0].clone(),
            site.dead
                .iter()
                .find(|candidate| site.navigation.symbols[candidate.symbol].name == "alpha")
                .unwrap()
                .key
                .clone(),
        ]
    };
    let bodies: Vec<_> = keys.iter().map(|key| {
        let revision = server.json(&format!("api/finding?key={key}"))["revision"].clone();
        serde_json::json!({ "csrf": server.site.borrow().csrf, "key": key, "state": "false-positive", "revision": revision }).to_string()
    }).collect();
    let path = root.path().join("src/a.rs");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("\n{original}")).unwrap();
    index(root.path());
    for body in bodies {
        let response = server.post_review(&body, "Origin: http://127.0.0.1:8787\r\n");
        assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    }
    assert!(!root
        .path()
        .join(".mct-index/review-decisions.toon")
        .exists());
}

#[test]
fn a_stale_view_is_rejected_with_the_current_review() {
    let root = project();
    let first = Server::start(root.path(), true);
    let second = Server::start(root.path(), true);
    let key = first.site.borrow().review_keys[0].clone();
    let shown = second.json(&format!("api/finding?key={key}"))["revision"].clone();
    assert!(first
        .decide(&key, Some("dismissed"), "")
        .starts_with("HTTP/1.1 200"));
    let csrf = second.site.borrow().csrf.clone();
    let body =
        serde_json::json!({ "csrf": csrf, "key": key, "state": "confirmed", "revision": shown })
            .to_string();
    let response = second.post_review(&body, "Origin: http://127.0.0.1:8787\r\n");
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    let current: serde_json::Value = serde_json::from_str(Server::body(&response)).unwrap();
    assert_eq!(current["state"], "dismissed");
    assert_eq!(
        Server::start(root.path(), true).json(&format!("api/finding?key={key}"))["state"],
        "dismissed"
    );
}

#[test]
fn an_unreadable_review_file_makes_reviews_read_only_and_hides_nothing() {
    let root = project();
    let key = Server::start(root.path(), true).site.borrow().review_keys[0].clone();
    let server = Server::start(root.path(), true);
    assert!(server
        .decide(&key, Some("dismissed"), "")
        .starts_with("HTTP/1.1 200"));
    let draft = root.path().join(".mct-index/reviews.toon");
    fs::write(&draft, "version: 2\ntruncated").unwrap();
    let degraded = Server::start(root.path(), true);
    let meta = degraded.json("api/meta");
    assert!(meta["reviewsReadOnly"]
        .as_str()
        .unwrap()
        .contains("reviews.toon"));
    let row = degraded.json(&format!("api/finding?key={key}"));
    // The valid decision stays visible, flagged as held; nothing is hidden.
    assert_eq!(row["state"], "needs-reevaluation");
    assert_eq!(row["previousState"], "dismissed");
    assert_eq!(row["held"], true);
    assert_eq!(row["readOnly"], true);
    let response = degraded.decide(&key, Some("confirmed"), "");
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert_eq!(fs::read_to_string(&draft).unwrap(), "version: 2\ntruncated");
}

#[test]
fn a_held_decision_stays_in_the_default_actionable_lists() {
    let root = project();
    let key = Server::start(root.path(), true).site.borrow().review_keys[0].clone();
    let server = Server::start(root.path(), true);
    assert!(server
        .decide(&key, Some("dismissed"), "")
        .starts_with("HTTP/1.1 200"));
    // The pages ask for these states by default (`ACTIONABLE` in page-findings.js).
    let actionable =
        "state=unreviewed,needs-reevaluation,in-review,confirmed,resolved-but-detected";
    let keys = |server: &Server| -> Vec<String> {
        server.json(&format!("api/findings?{actionable}"))["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["key"].as_str().map(str::to_owned))
            .collect()
    };
    assert!(
        !keys(&server).contains(&key),
        "a healthy dismissal hides it"
    );
    fs::write(
        root.path().join(".mct-index/reviews.toon"),
        "version: 2\ntruncated",
    )
    .unwrap();
    let degraded = Server::start(root.path(), true);
    assert!(
        keys(&degraded).contains(&key),
        "a held dismissal must stay in the actionable list"
    );
}

#[test]
fn stats_count_a_held_decision_as_the_state_the_list_reports() {
    let root = project();
    let key = Server::start(root.path(), true).site.borrow().review_keys[0].clone();
    let server = Server::start(root.path(), true);
    assert!(server
        .decide(&key, Some("dismissed"), "")
        .starts_with("HTTP/1.1 200"));
    let states = |server: &Server| server.json("api/stats")["findings"]["states"].clone();
    assert_eq!(states(&server)["dismissed"], 1, "a healthy dismissal");
    fs::write(
        root.path().join(".mct-index/reviews.toon"),
        "version: 2\ntruncated",
    )
    .unwrap();
    let degraded = states(&Server::start(root.path(), true));
    // The list reports the held row as `needs-reevaluation`; the counters
    // must agree with it instead of still counting it as dismissed.
    assert_eq!(degraded["needs-reevaluation"], 1, "{degraded}");
    assert!(degraded["dismissed"].is_null(), "{degraded}");
}

#[test]
fn duplicate_filters_match_every_member_without_splitting_groups() {
    let root = project();
    let source = fs::read_to_string(root.path().join("src/a.rs")).unwrap();
    for id in 0..8 {
        let dir = root.path().join(format!("module_{id}"));
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("member.rs"),
            source.replace("alpha", &format!("member_{id}")),
        )
        .unwrap();
    }
    index(root.path());
    let server = Server::start(root.path(), true);
    let all = server.json("api/findings?type=duplicate&size=1");
    let row = &all["items"][0];
    let key = row["key"].as_str().unwrap();
    assert_eq!(row["members"].as_array().unwrap().len(), 6);
    let detail = server.json(&format!("api/finding?key={key}"));
    let members = detail["members"].as_array().unwrap();
    assert_eq!(members.len(), 10);
    for member in members {
        let path = member["path"].as_str().unwrap();
        for filter in [path, path.split('/').next().unwrap()] {
            let filtered = server.json(&format!("api/findings?type=duplicate&dir={filter}&size=1"));
            assert_eq!(filtered["total"], 1, "{filter}");
            assert_eq!(
                filtered["items"][0], *row,
                "group identity and counts: {filter}"
            );
        }
        let name = member["name"].as_str().unwrap();
        assert_eq!(
            server.json(&format!("api/findings?type=duplicate&q={name}"))["total"],
            1
        );
    }
    assert_eq!(
        server.json("api/findings?type=duplicate&dir=module")["total"],
        0
    );
    assert_eq!(
        server.json("api/findings?type=duplicate&dir=missing.rs")["total"],
        0
    );
}

#[test]
fn recent_activity_retains_finding_identity_and_meta_names_the_canonical_review_file() {
    let root = project();
    let server = Server::start(root.path(), true);
    assert_eq!(
        server.json("api/meta")["reviewsFile"],
        ".mct-index/review-decisions.toon"
    );
    for kind in ["duplicate", "dead-code"] {
        let list = server.json(&format!("api/findings?type={kind}"));
        let key = list["items"][0]["key"].as_str().unwrap();
        assert!(server
            .decide(key, Some("confirmed"), "dashboard regression")
            .starts_with("HTTP/1.1 200"));
        let summary = server.json("api/summary");
        let event = summary["recent"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["finding"]["key"] == key)
            .unwrap();
        assert_eq!(event["finding"]["type"], kind);
        assert_eq!(server.json(&format!("api/finding?key={key}"))["key"], key);
    }
}

fn post_ignore(server: &Server, body: &serde_json::Value, headers: &str) -> String {
    let body = body.to_string();
    server.raw(&format!("POST /api/ignore HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
}

#[test]
fn mctignore_is_read_and_saved_with_origin_csrf_conflict_and_limit_checks() {
    let root = project();
    let server = Server::start(root.path(), true);
    let file = root.path().join(".mctignore");
    let csrf = server.site.borrow().csrf.clone();
    let origin = "Origin: http://127.0.0.1:8787\r\n";

    let state = server.json("api/ignore");
    assert_eq!(state["exists"], false);
    assert_eq!(state["importsGitignore"], false);
    assert_eq!(state["directive"], "@import-gitignore");
    assert!(state["template"]
        .as_str()
        .unwrap()
        .contains("@import-gitignore"));

    let save = |content: &str, base: &str| {
        post_ignore(
            &server,
            &serde_json::json!({ "csrf": csrf, "content": content, "base": base }),
            origin,
        )
    };
    let denied = post_ignore(
        &server,
        &serde_json::json!({ "csrf": "nope", "content": "x", "base": "" }),
        origin,
    );
    assert!(denied.starts_with("HTTP/1.1 403"), "{denied}");
    let no_origin = post_ignore(
        &server,
        &serde_json::json!({ "csrf": csrf, "content": "x", "base": "" }),
        "",
    );
    assert!(no_origin.starts_with("HTTP/1.1 403"), "{no_origin}");
    assert!(!file.exists());

    let saved = save("docs/\r\n@import-gitignore", "");
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "docs/\n@import-gitignore\n"
    );
    let state = server.json("api/ignore");
    assert_eq!(state["exists"], true);
    assert_eq!(state["importsGitignore"], true);

    let commented = save("# @import-gitignore\n", "docs/\n@import-gitignore\n");
    assert!(commented.starts_with("HTTP/1.1 200"), "{commented}");
    assert_eq!(server.json("api/ignore")["importsGitignore"], false);

    let stale = save("*.md\n", "something older");
    assert!(stale.starts_with("HTTP/1.1 409"), "{stale}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "# @import-gitignore\n");

    for (body, status) in [
        (
            serde_json::json!({ "csrf": csrf, "content": "a\u{0}b", "base": "# @import-gitignore\n" }),
            "400",
        ),
        (
            serde_json::json!({ "csrf": csrf, "content": "x".repeat(70_000), "base": "# @import-gitignore\n" }),
            "413",
        ),
        (
            serde_json::json!({ "csrf": csrf, "content": "x", "base": "# @import-gitignore\n", "path": "../x" }),
            "400",
        ),
        (
            serde_json::json!({ "csrf": csrf, "content": 7, "base": "" }),
            "400",
        ),
    ] {
        let response = post_ignore(&server, &body, origin);
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "{status}: {response}"
        );
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), "# @import-gitignore\n");
    let leftovers: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn dashboard_snapshot_never_mixes_navigation_and_references_across_reindex() {
    let root = project();
    let mut site = Site::load_snapshot(root.path(), true, || {
        fs::write(
            root.path().join("src/caller.rs"),
            "pub fn caller() { alpha(1); }\n",
        )
        .unwrap();
        index(root.path());
    })
    .unwrap();
    // Navigation was read before the external publication: every other cached
    // query must describe that same generation, where alpha was unreferenced.
    assert!(!site.navigation.symbols.iter().any(|s| s.name == "caller"));
    assert!(!site.references.contains_key("alpha"));
    assert!(site
        .dead
        .iter()
        .any(|candidate| site.navigation.symbols[candidate.symbol].name == "alpha"));
    // The next request follows the new generation after the read guard drops.
    site.follow_reindex();
    assert!(site.navigation.symbols.iter().any(|s| s.name == "caller"));
    assert!(site.references.contains_key("alpha"));
    assert!(!site
        .dead
        .iter()
        .any(|candidate| site.navigation.symbols[candidate.symbol].name == "alpha"));
}

#[test]
fn the_dashboard_url_carries_no_key_and_the_root_needs_none() {
    assert_eq!(super::dashboard_url(8787), "http://localhost:8787/");
    let root = project();
    let server = Server::start(root.path(), true);
    let page = server.get("");
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(Server::body(&page).contains("<html"));
    assert!(server.json("api/meta").is_object());
}

#[test]
fn mutations_are_refused_for_foreign_hosts_origins_and_missing_csrf() {
    let root = project();
    let server = Server::start(root.path(), true);
    let csrf = server.site.borrow().csrf.clone();
    let body = serde_json::json!({ "csrf": csrf, "key": "k", "state": "confirmed", "revision": 0 })
        .to_string();
    let post = |host: &str, extra: &str, body: &str| {
        server.raw(&format!("POST /api/review HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    };
    // DNS rebinding: the attacker's name resolves to 127.0.0.1 but the Host header says so.
    assert!(post(
        "rebind.evil.example:8787",
        "Origin: http://rebind.evil.example:8787\r\n",
        &body
    )
    .starts_with("HTTP/1.1 421"));
    // Another site posting from the user's browser.
    assert!(
        post("localhost:8787", "Origin: https://evil.example\r\n", &body)
            .starts_with("HTTP/1.1 403")
    );
    assert!(post(
        "localhost:8787",
        "Origin: http://localhost:8787\r\nSec-Fetch-Site: cross-site\r\n",
        &body
    )
    .starts_with("HTTP/1.1 403"));
    // No Origin at all (a non-browser client that is not replaying a page).
    assert!(post("localhost:8787", "", &body).starts_with("HTTP/1.1 403"));
    // Right origin, but without the page's CSRF token.
    let no_csrf =
        serde_json::json!({ "key": "k", "state": "confirmed", "revision": 0 }).to_string();
    let response = post(
        "localhost:8787",
        "Origin: http://localhost:8787\r\n",
        &no_csrf,
    );
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    // The same rules guard the .mctignore editor.
    let ignore = serde_json::json!({ "csrf": csrf, "content": "x", "base": "" }).to_string();
    let response = server.raw(&format!("POST /api/ignore HTTP/1.1\r\nHost: evil.example:8787\r\nOrigin: http://evil.example:8787\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{ignore}", ignore.len()));
    assert!(response.starts_with("HTTP/1.1 421"), "{response}");
}
