//! Dashboard conflicts are recorded in `<root>/.mct-index/errors/` while the
//! page receives the status and body it always did.
use super::*;

fn errors_dir(root: &Path) -> std::path::PathBuf {
    root.join(".mct-index").join("errors")
}

fn records(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(errors_dir(root)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The status line and body of a response, without what varies between two
/// runs: headers, and the revision, which carries a millisecond timestamp.
fn status_and_body(response: &str) -> (String, serde_json::Value) {
    let mut body: serde_json::Value = serde_json::from_str(Server::body(response)).unwrap();
    body.as_object_mut().unwrap().remove("revision");
    (
        response.lines().next().unwrap_or_default().to_string(),
        body,
    )
}

/// A second tab saved a decision first; this one then posts from a stale view.
fn stale_review(root: &Path) -> String {
    let first = Server::start(root, true);
    let second = Server::start(root, true);
    let key = first.site.borrow().review_keys[0].clone();
    let shown = second.json(&format!("api/finding?key={key}"))["revision"].clone();
    assert!(first
        .decide(&key, Some("dismissed"), "")
        .starts_with("HTTP/1.1 200"));
    let csrf = second.site.borrow().csrf.clone();
    let body = serde_json::json!({
        "csrf": csrf, "key": key, "state": "confirmed",
        "note": "private-note-text", "revision": shown,
    })
    .to_string();
    second.post_review(&body, "Origin: http://127.0.0.1:8787\r\n")
}

#[test]
fn a_review_conflict_is_recorded_and_the_page_gets_the_same_409() {
    let usable = project();
    let blocked = project();
    fs::write(errors_dir(blocked.path()), "occupied").unwrap();

    let recorded = stale_review(usable.path());
    let silent = stale_review(blocked.path());

    assert!(recorded.starts_with("HTTP/1.1 409"), "{recorded}");
    assert_eq!(status_and_body(&recorded), status_and_body(&silent));
    assert_eq!(
        fs::read_to_string(errors_dir(blocked.path())).unwrap(),
        "occupied"
    );

    let names = records(usable.path());
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--dashboard-review--review-conflict--"),
        "{}",
        names[0]
    );
    let body = fs::read_to_string(errors_dir(usable.path()).join(&names[0])).unwrap();
    assert!(body.contains("finding "), "{body}");
    assert!(
        !body.contains("private-note-text"),
        "the note is never recorded: {body}"
    );
}

#[test]
fn a_changed_source_is_recorded_as_review_source_stale() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    let revision = server.json(&format!("api/finding?key={key}"))["revision"].clone();
    let csrf = server.site.borrow().csrf.clone();
    let body = serde_json::json!({
        "csrf": csrf, "key": key, "state": "false-positive", "revision": revision,
    })
    .to_string();
    fs::write(root.path().join("src/a.rs"), "pub fn alpha() {}\n").unwrap();

    let response = server.post_review(&body, "Origin: http://127.0.0.1:8787\r\n");
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(
        response.contains("The source changed or is unavailable"),
        "{response}"
    );
    let names = records(root.path());
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--dashboard-review--review-source-stale--"),
        "{}",
        names[0]
    );
}

#[test]
fn an_ignore_file_conflict_is_recorded() {
    let root = project();
    let server = Server::start(root.path(), true);
    let saved = save_ignore(&server, "docs/\n", "");
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(
        records(root.path()).is_empty(),
        "a successful save records nothing"
    );
    let saved_content = fs::read_to_string(root.path().join(".mctignore")).unwrap();

    let stale = save_ignore(&server, "*.md\n", "something older");
    assert!(stale.starts_with("HTTP/1.1 409"), "{stale}");
    assert_eq!(
        fs::read_to_string(root.path().join(".mctignore")).unwrap(),
        saved_content,
        "the stale save wrote nothing"
    );
    let names = records(root.path());
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--dashboard-ignore--ignore-conflict--"),
        "{}",
        names[0]
    );
    let body = fs::read_to_string(errors_dir(root.path()).join(&names[0])).unwrap();
    assert!(
        body.contains(".mctignore changed on disk meanwhile"),
        "{body}"
    );
}

#[test]
fn an_accepted_decision_records_nothing() {
    let root = project();
    let server = Server::start(root.path(), true);
    let key = server.site.borrow().review_keys[0].clone();
    assert!(server
        .decide(&key, Some("dismissed"), "kept")
        .starts_with("HTTP/1.1 200"));
    assert!(records(root.path()).is_empty());
    assert!(!errors_dir(root.path()).exists());
}
