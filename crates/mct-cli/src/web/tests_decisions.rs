//! Decision-write contracts from the dashboard adversarial audit (AUD-01,
//! AUD-04, AUD-05, AUD-11, AUD-12): every write path is exercised through the
//! real HTTP handlers on a disposable project, for both the JSON API and the
//! server-rendered forms.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use mct_core::{LanguageParser, ParseError, ParsedFile, SourceFile};
use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{FindDeadCodeArgs, FindDuplicateCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use super::super::reviews::{Kind, State, Status};
use super::{
    api_json, fingerprint_like_adr_015, index, open_store, project, saved_decisions, Server,
};

const ORIGIN: &str = "Origin: http://127.0.0.1:8787\r\n";

struct PausingParser {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl LanguageParser for PausingParser {
    fn language_id(&self) -> &'static str {
        "phase3_fixture"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["reindex"]
    }

    fn parse(&self, _file: &SourceFile) -> Result<ParsedFile, ParseError> {
        self.entered.send(()).expect("test receiver remains open");
        self.release
            .lock()
            .expect("release lock")
            .recv_timeout(Duration::from_secs(10))
            .expect("test releases parser");
        Ok(ParsedFile::default())
    }
}

struct MutatingParser {
    root: std::path::PathBuf,
}

impl LanguageParser for MutatingParser {
    fn language_id(&self) -> &'static str {
        "phase3_mutating_fixture"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["reindex"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        fs::write(
            self.root.join(&file.relative_path),
            format!("{}\nchanged during parse", file.contents),
        )
        .expect("mutate isolated test source");
        Ok(ParsedFile::default())
    }
}

struct PassingParser;

impl LanguageParser for PassingParser {
    fn language_id(&self) -> &'static str {
        "phase3_recovery_fixture"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["reindex"]
    }

    fn parse(&self, _file: &SourceFile) -> Result<ParsedFile, ParseError> {
        Ok(ParsedFile::default())
    }
}

fn duplicate_fixture() -> tempfile::TempDir {
    const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    for (group, repeated) in [("a", 0), ("b", 1), ("c", 2)] {
        let body = BODY.replace(
            "total += 8;",
            &format!("{}total += 8;", "total *= 3;\n".repeat(repeated)),
        );
        for copy in 0..3 {
            fs::write(
                root.path().join(format!("src/{group}{copy}.rs")),
                format!("pub fn {group}{copy}{body}"),
            )
            .unwrap();
        }
    }
    reindex_project(root.path(), false);
    root
}

fn status_line(response: &str) -> &str {
    response.lines().next().unwrap_or_default()
}

fn assert_status(response: &str, expected: &str, context: &str) {
    assert!(
        status_line(response).starts_with(&format!("HTTP/1.1 {expected}")),
        "{context}: {response}"
    );
}

fn reindex_project(root: &Path, force: bool) -> mct_index::ReindexReport {
    let mut index = Index::open(
        root,
        &root.join(".mct-index/index.sqlite3"),
        ExcludeSet::default(),
    )
    .unwrap();
    let report = index
        .reindex(&mct_languages::build_registry(), force)
        .unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    report
}

fn mcp_server(root: &Path) -> MctServer {
    let index = Index::open_existing_read_only(
        root,
        &root.join(".mct-index/index.sqlite3"),
        ExcludeSet::default(),
    )
    .unwrap();
    MctServer::new(index, mct_languages::build_registry())
}

async fn mcp_text(result: Result<rmcp::model::CallToolResult, rmcp::ErrorData>) -> String {
    let result = result.unwrap();
    result.content[0].as_text().unwrap().text.clone()
}

fn run_end_to_end_queries(root: &Path) -> (String, String) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let server = mcp_server(root);
            let dead = mcp_text(
                server
                    .find_dead_code(Parameters(FindDeadCodeArgs {
                        path: None,
                        language: None,
                        limit: None,
                        offset: None,
                        format: Some("text".into()),
                    }))
                    .await,
            )
            .await;
            let duplicates = mcp_text(
                server
                    .find_duplicate_code(Parameters(FindDuplicateCodeArgs {
                        ..FindDuplicateCodeArgs::default()
                    }))
                    .await,
            )
            .await;
            (dead, duplicates)
        })
}

/// Writes decisions through the dashboard API, regenerates only the derived
/// index, then proves the MCP readers still apply the independent SQLite state.
#[test]
fn sqlite_review_decisions_survive_full_incremental_reindex_reopen_and_mcp_queries() {
    const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    for (group, repeated) in [("a", 0), ("b", 1), ("c", 2)] {
        let body = BODY.replace(
            "total += 8;",
            &format!("{}total += 8;", "total *= 3;\n".repeat(repeated)),
        );
        for copy in 0..3 {
            fs::write(
                root.path().join(format!("src/{group}{copy}.rs")),
                format!("pub fn {group}{copy}{body}"),
            )
            .unwrap();
        }
    }
    let initial_index = reindex_project(root.path(), false);
    assert_eq!(initial_index.files_parsed, 9);

    let dashboard = Server::start(root.path(), true);
    let dead_false_positive = dashboard.dead_id("a0");
    let dead_ignored = dashboard.dead_id("a1");
    let group_id = |prefix: &str| {
        let site = dashboard.site.borrow();
        let group = site
            .duplicates
            .groups
            .iter()
            .position(|group| {
                group
                    .members
                    .iter()
                    .any(|member| member.path.ends_with(&format!("/{prefix}0.rs")))
            })
            .unwrap();
        site.duplicate_keys[group].identity.clone()
    };
    let duplicate_false_positive = group_id("a");
    let duplicate_ignored = group_id("b");
    let duplicate_pending = group_id("c");

    for (kind, identity, action) in [
        ("dead", dead_false_positive.as_str(), "false_positive"),
        ("dead", dead_ignored.as_str(), "ignored"),
        (
            "duplicate",
            duplicate_false_positive.as_str(),
            "false_positive",
        ),
        ("duplicate", duplicate_ignored.as_str(), "ignored"),
    ] {
        assert_status(
            &dashboard.review(kind, identity, action, "phase 1 SQLite persistence"),
            "303",
            "dashboard review write",
        );
    }
    let records = saved_decisions(root.path());
    assert_eq!(records.len(), 4);
    assert!(records.iter().any(|r| r.kind == Kind::Dead
        && r.identity == dead_false_positive
        && r.status == Status::FalsePositive));
    assert!(records.iter().any(|r| r.kind == Kind::Dead
        && r.identity == dead_ignored
        && r.status == Status::Ignored));
    assert!(records.iter().any(|r| r.kind == Kind::Duplicate
        && r.identity == duplicate_false_positive
        && r.status == Status::FalsePositive));
    assert!(records.iter().any(|r| r.kind == Kind::Duplicate
        && r.identity == duplicate_ignored
        && r.status == Status::Ignored));
    assert!(!root.path().join(".mct-index/reviews.toon").exists());

    drop(dashboard);
    let forced = reindex_project(root.path(), true);
    assert_eq!(forced.files_parsed, 9);
    let after_forced = saved_decisions(root.path());
    assert_eq!(after_forced.len(), 4);
    for saved in &records {
        assert!(after_forced.iter().any(|current| {
            current.kind == saved.kind
                && current.identity == saved.identity
                && current.fingerprint == saved.fingerprint
                && current.status == saved.status
        }));
    }
    let incremental = reindex_project(root.path(), false);
    assert_eq!(incremental.files_parsed, 0);
    assert_eq!(incremental.files_unchanged, 9);
    assert_eq!(saved_decisions(root.path()).len(), 4);

    // Reopening Store and checking the exact persisted rows precedes the final
    // MCP request; both MCP tools read the same dashboard SQLite database.
    let reopened = open_store(root.path());
    let reopened_records = reopened.decisions().unwrap();
    assert_eq!(reopened_records.len(), 4);
    assert!(reopened_records
        .iter()
        .any(|record| record.identity == duplicate_false_positive));
    drop(reopened);

    let (dead, duplicates) = run_end_to_end_queries(root.path());
    assert!(!dead.contains("a0") && !dead.contains("a1"), "{dead}");
    assert!(
        dead.contains("c0"),
        "unreviewed dead code must remain visible: {dead}"
    );
    assert!(
        dead.contains("suppressed by valid saved review decisions"),
        "{dead}"
    );
    assert!(
        !duplicates.contains(&duplicate_false_positive),
        "{duplicates}"
    );
    assert!(!duplicates.contains(&duplicate_ignored), "{duplicates}");
    assert!(
        duplicates.contains(&duplicate_pending),
        "unreviewed duplicate group must remain visible: {duplicates}"
    );
    assert!(duplicates.contains("actionable: 1"), "{duplicates}");
    assert_eq!(saved_decisions(root.path()).len(), 4);

    // Material edits invalidate only their matching reviewed fingerprints.
    let dead_file = root.path().join("src/a0.rs");
    fs::write(
        &dead_file,
        format!("pub fn a0{}", BODY.replace("total += 1", "total += 101")),
    )
    .unwrap();
    let duplicate_file = root.path().join("src/a1.rs");
    fs::write(
        &duplicate_file,
        format!("pub fn a1{}", BODY.replace("total += 1", "total += 101")),
    )
    .unwrap();
    reindex_project(root.path(), false);
    let (changed_dead, changed_duplicates) = run_end_to_end_queries(root.path());
    assert!(
        changed_dead.contains("a0"),
        "changed dead finding must be actionable: {changed_dead}"
    );
    assert!(
        changed_duplicates.contains("actionable: 2"),
        "changed duplicate group and untouched group must remain actionable: {changed_duplicates}"
    );
    assert_eq!(saved_decisions(root.path()).len(), 4);
    assert!(!root.path().join(".mct-index/reviews.toon").exists());
}

fn first_member_paths(server: &Server) -> Vec<String> {
    server.site.borrow().duplicates.groups[0]
        .members
        .iter()
        .map(|member| member.path.clone())
        .collect()
}

fn duplicate_item(server: &Server, id: &str) -> serde_json::Value {
    api_json(&server.get(&format!("api/duplicates?id={id}")))["items"][0].clone()
}

/// AUD-01: once the index changed under the served snapshot, no write of any
/// kind is applied, even when the content fingerprints happen to be unchanged.
#[test]
fn a_stale_index_stamp_rejects_every_decision_write_before_saving() {
    let root = project();
    let server = Server::start(root.path(), true);
    let dup = server.duplicate_id(0);
    let dead = server.dead_id("alpha");
    let comment = api_json(&server.write(
        "api/comment",
        &json!({"kind": "duplicate", "id": dup, "body": "before"}),
    ))["comment_id"]
        .as_i64()
        .unwrap();
    let path = first_member_paths(&server).remove(0);
    let item = duplicate_item(&server, &dup);
    let review = json!({
        "kind": "duplicate", "id": dup, "action": "ignored",
        "fingerprint": item["fingerprint"], "expected_revision": item["revision"],
    });
    let form = server.review_body("dead", &dead, "false_positive", &[]);

    // An unrelated edit keeps every fingerprint but moves the index stamp.
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        format!("{}\n// unrelated\n", fs::read_to_string(&file).unwrap()),
    )
    .unwrap();
    index(root.path());

    for (route, body) in [
        ("api/review", review.clone()),
        (
            "api/file-review",
            json!({"id": dup, "path": path, "status": "rejected", "note": "x", "expected_revision": 0}),
        ),
        (
            "api/review/delete",
            json!({"kind": "dead", "id": "a".repeat(64), "expected_revision": 0}),
        ),
        (
            "api/comment",
            json!({"kind": "duplicate", "id": dup, "body": "after"}),
        ),
        (
            "api/comment/update",
            json!({"comment_id": comment, "body": "edited"}),
        ),
        ("api/comment/delete", json!({"comment_id": comment})),
    ] {
        assert_status(&server.write(route, &body), "409", route);
    }
    assert_status(&server.form("review", &form, ORIGIN), "409", "form review");

    assert!(saved_decisions(root.path()).is_empty());
    let store = open_store(root.path());
    assert!(store.file_reviews(&dup).unwrap().is_empty());
    let comments = store.comments(Kind::Duplicate, &dup).unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].body, "before");
}

/// AUD-01: both contracts must name the reviewed content and revision; a
/// decision for code the editor did not see is a conflict, never a save.
#[test]
fn both_contracts_require_the_reviewed_fingerprint_and_reject_unseen_code() {
    let root = project();
    let server = Server::start(root.path(), true);
    let dup = server.duplicate_id(0);
    let dead = server.dead_id("alpha");
    let csrf = server.site.borrow().csrf.clone();
    let item = duplicate_item(&server, &dup);
    let other = server
        .site
        .borrow()
        .fingerprint_of(Kind::Dead, &dead)
        .unwrap();

    let missing = |omit: &str| {
        let mut body = json!({
            "kind": "duplicate", "id": dup, "action": "ignored",
            "fingerprint": item["fingerprint"], "expected_revision": item["revision"],
        });
        body.as_object_mut().unwrap().remove(omit);
        body
    };
    // `Server::write` fills absent fields, so these go through `json_post`.
    let headers = format!("{ORIGIN}X-CSRF-Token: {csrf}\r\n");
    for omit in ["fingerprint", "expected_revision"] {
        let response = server.json_post("api/review", &missing(omit).to_string(), &headers);
        assert_status(&response, "400", omit);
        let form = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(
                [
                    ("csrf", csrf.as_str()),
                    ("kind", "dead"),
                    ("id", dead.as_str()),
                    ("action", "ignored"),
                    ("fingerprint", other.as_str()),
                    ("expected_revision", "0"),
                ]
                .into_iter()
                .filter(|(key, _)| *key != omit),
            )
            .finish();
        assert_status(&server.form("review", &form, ORIGIN), "400", omit);
    }
    let mut wrong = missing("");
    wrong["fingerprint"] = json!(other);
    assert_status(
        &server.write("api/review", &wrong),
        "409",
        "foreign fingerprint",
    );
    assert!(saved_decisions(root.path()).is_empty());

    // The body changes and the dashboard loads the new snapshot.
    let stale_form = server.review_body("duplicate", &dup, "false_positive", &[]);
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("total += 8", "total += 9"),
    )
    .unwrap();
    index(root.path());
    let reload = server.form("reload", &format!("csrf={csrf}"), ORIGIN);
    assert_status(&reload, "303", "reload");
    assert_ne!(
        duplicate_item(&server, &dup)["fingerprint"],
        item["fingerprint"]
    );
    assert_status(
        &server.write("api/review", &missing("")),
        "409",
        "old api fingerprint",
    );
    assert_status(
        &server.form("review", &stale_form, ORIGIN),
        "409",
        "old form fingerprint",
    );
    assert!(saved_decisions(root.path()).is_empty());

    // Reviewing the new content is allowed.
    let fresh = duplicate_item(&server, &dup);
    let saved = server.write(
        "api/review",
        &json!({"kind": "duplicate", "id": dup, "action": "ignored",
                "fingerprint": fresh["fingerprint"], "expected_revision": fresh["revision"]}),
    );
    assert_status(&saved, "200", "fresh review");
    assert_eq!(
        saved_decisions(root.path())[0].fingerprint,
        fresh["fingerprint"]
    );
}

/// AUD-04: a per-file label never changes the group verdict, the API says so
/// explicitly, and a real group-level dismissal is a separate decision.
#[test]
fn file_labels_are_not_group_verdicts_and_group_dismissal_is_explicit() {
    let root = project();
    let server = Server::start(root.path(), true);
    let dup = server.duplicate_id(0);
    let item = duplicate_item(&server, &dup);
    for path in first_member_paths(&server) {
        let saved = server.write(
            "api/file-review",
            &json!({"id": dup, "path": path, "status": "rejected", "note": "remove this copy"}),
        );
        assert_status(&saved, "200", &path);
        let saved = api_json(&saved);
        assert_eq!(saved["scope"], "file_label");
        assert_eq!(saved["finding_state"]["status"], "pending");
    }
    assert_eq!(duplicate_item(&server, &dup)["state"]["status"], "pending");
    assert_eq!(
        api_json(&server.get("api/summary"))["duplicates_pending"],
        1
    );
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={dup}")));
    assert_eq!(finding["state"]["status"], "pending");
    assert_eq!(finding["files"][0]["status"], "rejected");
    assert_eq!(finding["files"][0]["scope"], "file_label");

    // A label may name the content its editor saw; another content conflicts.
    let path = first_member_paths(&server).remove(0);
    let foreign = server
        .site
        .borrow()
        .fingerprint_of(Kind::Dead, &server.dead_id("alpha"));
    let conflict = server.write(
        "api/file-review",
        &json!({"id": dup, "path": path, "status": "accepted", "fingerprint": foreign}),
    );
    assert_status(&conflict, "409", "foreign label fingerprint");
    let matched = server.write(
        "api/file-review",
        &json!({"id": dup, "path": path, "status": "accepted", "fingerprint": item["fingerprint"]}),
    );
    assert_status(&matched, "200", "label fingerprint");

    // The group verdict is the finding decision.
    let current = duplicate_item(&server, &dup);
    let dismissed = server.write(
        "api/review",
        &json!({"kind": "duplicate", "id": dup, "action": "false_positive", "reason": "template",
                "fingerprint": current["fingerprint"], "expected_revision": current["revision"]}),
    );
    assert_status(&dismissed, "200", "group dismissal");
    assert_eq!(api_json(&dismissed)["state"]["status"], "false_positive");
    assert_eq!(
        duplicate_item(&server, &dup)["state"]["status"],
        "false_positive"
    );
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={dup}")));
    assert_eq!(finding["files"].as_array().unwrap().len(), 2, "labels kept");
}

/// AUD-05: two editors of one identity; the slower one gets a conflict for
/// decisions and file labels instead of silently replacing the newer save.
#[test]
fn two_editors_conflict_instead_of_overwriting_decisions_and_file_labels() {
    let root = project();
    let server = Server::start(root.path(), true);
    let dup = server.duplicate_id(0);
    let seen = duplicate_item(&server, &dup);
    let decision = |action: &str, reason: &str, revision: &serde_json::Value| {
        json!({"kind": "duplicate", "id": dup, "action": action, "reason": reason,
               "fingerprint": seen["fingerprint"], "expected_revision": revision})
    };
    let newer = server.write("api/review", &decision("confirmed", "B", &seen["revision"]));
    assert_status(&newer, "200", "editor B");
    let older = server.write("api/review", &decision("ignored", "A", &seen["revision"]));
    assert_status(&older, "409", "editor A");
    let state = duplicate_item(&server, &dup)["state"].clone();
    assert_eq!(
        (state["status"].as_str(), state["reason"].as_str()),
        (Some("confirmed"), Some("B"))
    );
    assert_eq!(
        open_store(root.path())
            .history(Kind::Duplicate, &dup)
            .unwrap()
            .len(),
        1
    );

    let path = first_member_paths(&server).remove(0);
    let label = |note: &str, revision: u64| json!({"id": dup, "path": path, "status": "in_review", "note": note, "expected_revision": revision});
    assert_status(
        &server.write("api/file-review", &label("B", 0)),
        "200",
        "label B",
    );
    assert_status(
        &server.write("api/file-review", &label("A", 0)),
        "409",
        "label A",
    );
    let finding = api_json(&server.get(&format!("api/finding?kind=duplicate&id={dup}")));
    assert_eq!(finding["files"][0]["note"], "B");
    let latest = finding["files"][0]["revision"].as_u64().unwrap();
    assert_status(
        &server.write("api/file-review", &label("A", latest)),
        "200",
        "rebased A",
    );

    // The server-rendered form follows the same revision contract.
    let dead = server.dead_id("alpha");
    let stale = server.review_body("dead", &dead, "ignored", &[]);
    assert_status(
        &server.review("dead", &dead, "confirmed", ""),
        "303",
        "form B",
    );
    assert_status(&server.form("review", &stale, ORIGIN), "409", "form A");
    assert!(matches!(
        open_store(root.path())
            .decisions()
            .unwrap()
            .iter()
            .find(|r| r.identity == dead)
            .map(|r| r.status),
        Some(Status::Confirmed)
    ));
}

/// AUD-11: deleting an orphan leaves a tombstone, so the ADR-015 verdict it
/// replaced does not come back when the original source returns.
#[test]
fn deleting_an_orphan_never_resurrects_a_legacy_dismissal() {
    let root = project();
    let first = Server::start(root.path(), true);
    let dup = first.duplicate_id(0);
    let legacy_key = fingerprint_like_adr_015(&first.site.borrow(), 0);
    drop(first);
    let legacy = root.path().join(".mct-index/duplicate-reviews.toon");
    let content = format!("version: 1\nreviews[1]{{key,reason}}:\n  {legacy_key},false-positive\n");
    fs::write(&legacy, &content).unwrap();

    let old = Server::start(root.path(), true);
    assert!(matches!(
        old.site.borrow().duplicate_state(0),
        State::Decided {
            status: Status::FalsePositive,
            at: None,
            ..
        }
    ));
    let item = duplicate_item(&old, &dup);
    let decided = old.write(
        "api/review",
        &json!({"kind": "duplicate", "id": dup, "action": "confirmed", "reason": "new look",
                "fingerprint": item["fingerprint"], "expected_revision": item["revision"]}),
    );
    assert_status(&decided, "200", "new decision");

    let copy = root.path().join("src/b.rs");
    let source = fs::read_to_string(&copy).unwrap();
    fs::remove_file(&copy).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    assert!(!orphan.site.borrow().duplicate_ids.contains_key(&dup));
    let listed = api_json(&orphan.get("api/reviews?status=orphan&kind=duplicate"));
    assert_eq!(listed["total"], 1);
    let deleted = orphan.write(
        "api/review/delete",
        &json!({"kind": "duplicate", "id": dup, "expected_revision": listed["items"][0]["revision"]}),
    );
    assert_status(&deleted, "200", "delete orphan");

    fs::write(&copy, source).unwrap();
    index(root.path());
    for restarted in [
        Server::start(root.path(), true),
        Server::start(root.path(), true),
    ] {
        assert_eq!(restarted.duplicate_id(0), dup);
        assert_eq!(restarted.site.borrow().duplicate_state(0), State::Pending);
        assert_eq!(
            duplicate_item(&restarted, &dup)["state"]["status"],
            "pending"
        );
    }
    assert_eq!(
        fs::read_to_string(&legacy).unwrap(),
        content,
        "legacy file untouched"
    );
    let path = root.path().canonicalize().unwrap();
    let ai = mct_index::review_state::ReviewSnapshot::load(&path).unwrap();
    let fingerprint = Server::start(root.path(), true)
        .site
        .borrow()
        .duplicate_keys[0]
        .fingerprint
        .clone();
    assert!(!ai.suppresses("duplicate", &dup, &fingerprint, &legacy_key));
}

/// AUD-12: restored (Pending) and label-only orphans stay listable and
/// openable, never count as active decisions, and can be archived.
#[test]
fn restored_pending_orphans_are_listed_openable_and_not_active_decisions() {
    let root = project();
    let server = Server::start(root.path(), true);
    let dead = server.dead_id("alpha");
    let dup = server.duplicate_id(0);
    assert_status(
        &server.review("dead", &dead, "ignored", "first"),
        "303",
        "decide",
    );
    assert_status(
        &server.review("dead", &dead, "restore", ""),
        "303",
        "restore",
    );
    let comment = server.write(
        "api/comment",
        &json!({"kind": "dead", "id": dead, "body": "retained note"}),
    );
    assert_status(&comment, "200", "comment");
    let path = first_member_paths(&server).remove(0);
    let label = server.write(
        "api/file-review",
        &json!({"id": dup, "path": path, "status": "in_review", "note": "label only"}),
    );
    assert_status(&label, "200", "label");

    fs::remove_file(root.path().join("src/a.rs")).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    let listing = api_json(&orphan.get("api/reviews?status=orphan"));
    assert_eq!(listing["total"], 2, "{listing}");
    for item in listing["items"].as_array().unwrap() {
        assert_eq!(item["current"], false);
        assert_eq!(item["state"]["status"], "pending");
        assert!(item["revision"].as_u64().unwrap() > 0);
    }
    assert_eq!(api_json(&orphan.get("api/summary"))["orphan_findings"], 2);
    let detail = api_json(&orphan.get(&format!("api/finding?kind=dead&id={dead}")));
    assert_eq!(detail["current"], false);
    assert_eq!(detail["state"]["status"], "pending");
    assert_eq!(detail["comments"][0]["body"], "retained note");
    assert_eq!(detail["history"].as_array().unwrap().len(), 2);
    let detail = api_json(&orphan.get(&format!("api/finding?kind=duplicate&id={dup}")));
    assert_eq!(detail["files"][0]["note"], "label only");
    let page = orphan.get("revisiones?status=obsolete");
    assert!(
        page.contains(&dead[..12]) && page.contains(&dup[..12]),
        "{page}"
    );

    // Restored rows are retained state, not active decisions.
    assert!(open_store(root.path()).decisions().unwrap().is_empty());

    for item in listing["items"].as_array().unwrap() {
        let deleted = orphan.write(
            "api/review/delete",
            &json!({"kind": item["kind"], "id": item["identity"], "expected_revision": item["revision"]}),
        );
        assert_status(&deleted, "200", "archive");
    }
    assert_eq!(
        api_json(&orphan.get("api/reviews?status=orphan"))["total"],
        0
    );
    assert_status(
        &orphan.get(&format!("api/finding?kind=dead&id={dead}")),
        "404",
        "archived finding",
    );
    assert_eq!(
        open_store(root.path())
            .comments(Kind::Dead, &dead)
            .unwrap()
            .len(),
        1
    );
}

/// Saved state and applicability are separate, including when no current code
/// can justify a saved dismissal. Reads must not create decision history.
#[test]
fn finding_detail_explains_saved_changed_unavailable_reset_and_orphan_decisions() {
    let root = project();
    let server = Server::start(root.path(), true);
    let identity = server.dead_id("alpha");
    let url = format!("api/finding?kind=dead&id={identity}");
    let detail = api_json(&server.get(&url));
    assert_eq!(detail["review_validity"], "not_reviewed");
    assert!(detail["saved_decision"].is_null());
    assert_eq!(detail["actionable"], true);
    assert_status(
        &server.review("dead", &identity, "false_positive", "intentional"),
        "303",
        "save",
    );
    let detail = api_json(&server.get(&url));
    assert_eq!(detail["review_validity"], "unchanged");
    assert_eq!(detail["saved_decision"]["status"], "false_positive");
    assert_eq!(detail["effective_decision"], "false_positive");
    assert_eq!(detail["actionable"], false);
    let before = detail["history"].clone();
    let file = root.path().join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    // No indexing has happened: a cached fingerprint is not fresh source proof.
    fs::remove_file(&file).unwrap();
    let unavailable = api_json(&server.get(&url));
    assert_eq!(unavailable["review_validity"], "unavailable");
    assert!(unavailable["effective_decision"].is_null());
    assert_eq!(unavailable["actionable"], true);
    assert_eq!(unavailable["history"], before);
    assert_eq!(unavailable["saved_decision"], detail["saved_decision"]);
    let listing = api_json(&server.get("api/dead?status=needs_reevaluation"));
    assert!(listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["identity"] == identity));
    fs::write(
        &file,
        original.replace("fn alpha(", "fn alpha( /* changed */ "),
    )
    .unwrap();
    index(root.path());
    let changed = Server::start(root.path(), true);
    let detail = api_json(&changed.get(&url));
    assert_eq!(detail["review_validity"], "changed");
    assert_eq!(detail["state"]["status"], "needs_reevaluation");
    assert_eq!(detail["saved_decision"]["reason"], "intentional");
    assert_eq!(detail["history"], before);
    assert_status(
        &changed.review("dead", &identity, "restore", ""),
        "303",
        "reset",
    );
    let detail = api_json(&changed.get(&url));
    assert_eq!(detail["review_validity"], "reset");
    assert_eq!(detail["saved_decision"]["status"], "pending");
    assert!(detail["effective_decision"].is_null());
    assert_eq!(detail["history"].as_array().unwrap().len(), 2);
    assert_status(
        &changed.review("dead", &identity, "ignored", "retain"),
        "303",
        "decide again",
    );
    fs::remove_file(&file).unwrap();
    index(root.path());
    let orphan = Server::start(root.path(), true);
    let detail = api_json(&orphan.get(&url));
    assert_eq!(detail["review_validity"], "not_current");
    assert_eq!(detail["saved_decision"]["status"], "ignored");
    assert_eq!(detail["actionable"], false);
    assert!(detail["effective_decision"].is_null());
    assert_eq!(detail["history"].as_array().unwrap().len(), 3);
}

#[test]
fn comment_edits_and_deletes_require_current_revision_and_owner() {
    let root = project();
    let a = Server::start(root.path(), true);
    let b = Server::start(root.path(), true);
    let id = a.duplicate_id(0);
    let route = format!("api/finding?kind=duplicate&id={id}");
    let comment = api_json(&a.write(
        "api/comment",
        &json!({"kind":"duplicate","id":id,"body":"original"}),
    ))["comment_id"]
        .clone();
    let before = api_json(&b.get(&route));
    let request = json!({"kind":"duplicate","id":id,"comment_id":comment,"expected_revision":before["revision"],"body":"tab A"});
    assert_status(
        &a.write("api/comment/update", &request),
        "200",
        "first edit",
    );
    let latest = api_json(&b.get(&route));
    assert_eq!(latest["comments"][0]["body"], "tab A");
    assert_eq!(
        latest["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    let mut stale = request.clone();
    stale["body"] = json!("tab B draft");
    assert_status(&b.write("api/comment/update", &stale), "409", "stale edit");
    stale.as_object_mut().unwrap().remove("body");
    assert_status(
        &b.write("api/comment/delete", &stale),
        "409",
        "stale delete",
    );
    let mut wrong = request.clone();
    wrong["kind"] = json!("dead");
    wrong["expected_revision"] = latest["revision"].clone();
    assert_status(
        &b.write("api/comment/update", &wrong),
        "404",
        "different owner",
    );
    let mut missing = request.clone();
    missing.as_object_mut().unwrap().remove("expected_revision");
    assert_status(
        &b.write("api/comment/update", &missing),
        "400",
        "unsafe legacy edit",
    );
    assert_status(
        &b.write("api/comment/delete", &json!({"comment_id":comment})),
        "400",
        "unsafe legacy delete",
    );
    let preserved = api_json(&b.get(&route));
    assert_eq!(preserved["comments"], latest["comments"]);
    assert_eq!(preserved["revision"], latest["revision"]);
    stale["expected_revision"] = latest["revision"].clone();
    assert_status(
        &a.write("api/comment/delete", &stale),
        "200",
        "current delete",
    );
    assert_status(
        &b.write("api/comment/update", &request),
        "404",
        "deleted edit",
    );
    assert_status(
        &b.write("api/comment/delete", &stale),
        "404",
        "deleted delete",
    );
    assert!(api_json(&b.get(&route))["comments"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn one_line_dead_review_survives_unchanged_native_style_reload() {
    let root = project();
    let file = root.path().join("src/a.rs");
    fs::write(
        &file,
        format!(
            "{}\nfn dead_candidate() -> i32 {{ 42 }}\n",
            fs::read_to_string(&file).unwrap()
        ),
    )
    .unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let id = server.dead_id("dead_candidate");
    let before = api_json(&server.get(&format!("api/finding?kind=dead&id={id}")));
    let form = server.review_body("dead", &id, "false_positive", &[]);
    assert_status(
        &server.form("review", &form, ORIGIN),
        "303",
        "save rendered form",
    );
    index(root.path());
    let refreshed_page = server.get("muerto?status=false_positive");
    assert!(
        refreshed_page.contains("dead_candidate"),
        "rendered refresh lost unchanged false positive"
    );
    let reloaded = Server::start(root.path(), true);
    assert_eq!(reloaded.dead_id("dead_candidate"), id);
    let after = api_json(&reloaded.get(&format!("api/finding?kind=dead&id={id}")));
    assert_eq!(before["fingerprint"], after["fingerprint"], "{after}");
    assert_eq!(after["effective_decision"], "false_positive", "{after}");
}

#[test]
fn comment_http_latency_fixture_checks_commits_and_conflicts() {
    let root = project();
    let server = Server::start(root.path(), true);
    let id = server.duplicate_id(0);
    let route = format!("api/finding?kind=duplicate&id={id}");
    let comment = api_json(&server.write(
        "api/comment",
        &json!({"kind":"duplicate","id":id,"body":"initial"}),
    ))["comment_id"]
        .clone();
    let base_revision = api_json(&server.get(&route))["revision"].as_u64().unwrap();
    let (mut saves, mut conflicts, mut reads) = (Vec::new(), Vec::new(), Vec::new());
    for n in 0..32 {
        let revision = base_revision + n;
        let body = format!("committed note {n}");
        let request = json!({"kind":"duplicate","id":id,"comment_id":comment,"expected_revision":revision,"body":body});
        let start = std::time::Instant::now();
        let response = server.write("api/comment/update", &request);
        let save = start.elapsed().as_micros();
        assert_status(&response, "200", "measured save");
        let start = std::time::Instant::now();
        let conflict = server.write("api/comment/update", &request);
        let rejected = start.elapsed().as_micros();
        assert_status(&conflict, "409", "measured stale save");
        let start = std::time::Instant::now();
        let current = api_json(&server.get(&route));
        let read = start.elapsed().as_micros();
        assert_eq!(current["comments"][0]["body"], body);
        assert_eq!(current["revision"].as_u64().unwrap(), revision + 1);
        if n > 0 {
            saves.push(save);
            conflicts.push(rejected);
            reads.push(read);
        }
    }
    saves.sort();
    conflicts.sort();
    reads.sort();
    eprintln!(
        "phase7_comment_http_fixture:\n  samples: 31\n  warmup: 1\n  save_p50_us: {}\n  save_p95_us: {}\n  conflict_p50_us: {}\n  conflict_p95_us: {}\n  detail_p50_us: {}\n  detail_p95_us: {}",
        saves[15], saves[29], conflicts[15], conflicts[29], reads[15], reads[29]
    );
}

#[test]
fn unrelated_file_incremental_reindex_preserves_dead_and_duplicate_reviews() {
    let root = duplicate_fixture();
    let dashboard = Server::start(root.path(), true);
    let dead = dashboard.dead_id("a0");
    let duplicate = dashboard.duplicate_id(0);
    assert_status(
        &dashboard.review("dead", &dead, "false_positive", "stable dead finding"),
        "303",
        "dead review write",
    );
    assert_status(
        &dashboard.review("duplicate", &duplicate, "ignored", "stable duplicate group"),
        "303",
        "duplicate review write",
    );
    let before: Vec<_> = saved_decisions(root.path())
        .into_iter()
        .map(|row| (row.kind, row.identity, row.fingerprint, row.status))
        .collect();
    assert_eq!(before.len(), 2);
    drop(dashboard);

    fs::write(
        root.path().join("src/unrelated.rs"),
        "pub fn unrelated() { let value = 17; assert_eq!(value, 17); }\n",
    )
    .unwrap();
    let report = reindex_project(root.path(), false);
    assert_eq!(report.files_parsed, 1);
    let after: Vec<_> = saved_decisions(root.path())
        .into_iter()
        .map(|row| (row.kind, row.identity, row.fingerprint, row.status))
        .collect();
    assert_eq!(
        after, before,
        "unrelated files must not rewrite review data"
    );

    let (dead_output, duplicate_output) = run_end_to_end_queries(root.path());
    assert!(!dead_output.contains(&dead), "{dead_output}");
    assert!(
        dead_output.contains("a1"),
        "unreviewed dead code stays visible: {dead_output}"
    );
    assert!(
        dead_output.contains("c0"),
        "unreviewed dead code stays visible: {dead_output}"
    );
    assert!(!duplicate_output.contains(&duplicate), "{duplicate_output}");
    assert!(
        duplicate_output.contains("actionable: 2"),
        "unreviewed duplicate groups stay visible: {duplicate_output}"
    );
}

#[test]
fn review_writes_during_reindex_are_visible_to_mcp_and_survive_failed_publication() {
    let root = duplicate_fixture();
    let dashboard = Server::start(root.path(), true);
    let (before_dead, _) = run_end_to_end_queries(root.path());
    assert!(
        before_dead.contains("src/a0.rs:\n  function a0 L1-L9"),
        "{before_dead}"
    );
    let dead = dashboard.dead_id("a0");
    let duplicate = dashboard.duplicate_id(0);
    fs::write(root.path().join("gate.reindex"), "pause inside parser").unwrap();

    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut registry = mct_languages::build_registry();
    registry.register(Arc::new(PausingParser {
        entered: entered_tx,
        release: Mutex::new(release_rx),
    }));
    let index_path = root.path().join(".mct-index/index.sqlite3");
    let writer = Index::open(root.path(), &index_path, ExcludeSet::default()).unwrap();
    let update = std::thread::spawn(move || {
        let mut writer = writer;
        writer.reindex(&registry, true)
    });
    entered_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("writer reaches the deterministic parser barrier");

    assert_status(
        &dashboard.review("dead", &dead, "ignored", "written during indexing"),
        "303",
        "concurrent dead review write",
    );
    assert_status(
        &dashboard.review(
            "duplicate",
            &duplicate,
            "false_positive",
            "written during indexing",
        ),
        "303",
        "concurrent duplicate review write",
    );
    let committed_reviews: Vec<_> = saved_decisions(root.path())
        .into_iter()
        .map(|row| (row.kind, row.identity, row.fingerprint, row.status))
        .collect();
    assert_eq!(committed_reviews.len(), 2);

    // The independent MCP reader must see one complete old generation while
    // the writer is inside its uncommitted transaction, together with the
    // independently committed dashboard decisions.
    let (during_dead, during_duplicates) = run_end_to_end_queries(root.path());
    assert!(
        !during_dead.contains("src/a0.rs:") && !during_dead.contains("function a0 "),
        "{during_dead}"
    );
    assert!(
        during_dead.contains("src/c0.rs:\n  function c0 L1-L11"),
        "{during_dead}"
    );
    assert!(
        !during_duplicates.contains(&duplicate),
        "{during_duplicates}"
    );
    assert!(
        during_duplicates.contains("actionable: 2"),
        "{during_duplicates}"
    );

    release_tx.send(()).unwrap();
    let report = update.join().unwrap().unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(
        saved_decisions(root.path())
            .into_iter()
            .map(|row| (row.kind, row.identity, row.fingerprint, row.status))
            .collect::<Vec<_>>(),
        committed_reviews
    );
    let (after_dead, after_duplicates) = run_end_to_end_queries(root.path());
    assert!(
        !after_dead.contains("src/a0.rs:") && !after_dead.contains("function a0 "),
        "{after_dead}"
    );
    assert!(
        after_dead.contains("src/c0.rs:\n  function c0 L1-L11"),
        "{after_dead}"
    );
    assert!(!after_duplicates.contains(&duplicate), "{after_duplicates}");
    assert!(
        after_duplicates.contains("actionable: 2"),
        "{after_duplicates}"
    );
    drop(dashboard);

    // A deterministic source mutation during parse aborts publication. The
    // last complete index generation and the separate review database remain
    // readable and unchanged.
    let generation_before =
        Index::open_existing_read_only(root.path(), &index_path, ExcludeSet::default())
            .unwrap()
            .status()
            .unwrap()
            .published_generation;
    let mut failing_registry = mct_languages::build_registry();
    failing_registry.register(Arc::new(MutatingParser {
        root: root.path().to_path_buf(),
    }));
    let mut writer = Index::open(root.path(), &index_path, ExcludeSet::default()).unwrap();
    let error = writer.reindex(&failing_registry, true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source file changed while indexing"),
        "unexpected indexing failure: {error:#}"
    );
    let recovered =
        Index::open_existing_read_only(root.path(), &index_path, ExcludeSet::default()).unwrap();
    assert_eq!(
        recovered.status().unwrap().published_generation,
        generation_before
    );
    assert_eq!(saved_decisions(root.path()).len(), 2);
    let (after_rollback_dead, after_rollback_duplicates) = run_end_to_end_queries(root.path());
    assert!(
        !after_rollback_dead.contains("src/a0.rs:")
            && !after_rollback_dead.contains("function a0 "),
        "{after_rollback_dead}"
    );
    assert!(
        after_rollback_dead.contains("src/c0.rs:\n  function c0 L1-L11"),
        "{after_rollback_dead}"
    );
    assert!(
        !after_rollback_duplicates.contains(&duplicate),
        "{after_rollback_duplicates}"
    );
    assert!(
        after_rollback_duplicates.contains("actionable: 2"),
        "{after_rollback_duplicates}"
    );

    let mut recovery_registry = mct_languages::build_registry();
    recovery_registry.register(Arc::new(PassingParser));
    let mut recovery_writer = Index::open(root.path(), &index_path, ExcludeSet::default()).unwrap();
    recovery_writer.reindex(&recovery_registry, true).unwrap();
    assert_eq!(
        recovery_writer.status().unwrap().published_generation,
        generation_before + 1
    );
    assert_eq!(saved_decisions(root.path()).len(), 2);
    let (after_recovery_dead, after_recovery_duplicates) = run_end_to_end_queries(root.path());
    assert!(
        !after_recovery_dead.contains("src/a0.rs:")
            && !after_recovery_dead.contains("function a0 "),
        "{after_recovery_dead}"
    );
    assert!(
        after_recovery_dead.contains("src/c0.rs:\n  function c0 L1-L11"),
        "{after_recovery_dead}"
    );
    assert!(
        !after_recovery_duplicates.contains(&duplicate),
        "{after_recovery_duplicates}"
    );
    assert!(
        after_recovery_duplicates.contains("actionable: 2"),
        "{after_recovery_duplicates}"
    );
}

#[test]
fn identical_review_api_writes_upsert_one_decision_and_preserve_other_state() {
    let root = duplicate_fixture();
    let dashboard = Server::start(root.path(), true);
    let reviewed = dashboard.dead_id("a0");
    let unrelated = dashboard.dead_id("a1");
    assert_status(
        &dashboard.review("dead", &reviewed, "false_positive", "same reason"),
        "303",
        "first review write",
    );
    assert_status(
        &dashboard.review("dead", &unrelated, "ignored", "keep this decision"),
        "303",
        "unrelated review write",
    );
    open_store(root.path())
        .add_comment(Kind::Dead, &unrelated, "preserve this annotation", 17)
        .unwrap();

    // The store intentionally records each accepted write in history and
    // advances its revision; the current decision row remains a single upsert.
    assert_status(
        &dashboard.review("dead", &reviewed, "false_positive", "same reason"),
        "303",
        "repeated identical review write",
    );
    let store = open_store(root.path());
    let decisions = store.decisions().unwrap();
    assert_eq!(decisions.len(), 2, "one current row per logical finding");
    let reviewed_row = decisions
        .iter()
        .find(|row| row.identity == reviewed)
        .unwrap();
    assert_eq!(reviewed_row.status, Status::FalsePositive);
    assert_eq!(store.history(Kind::Dead, &reviewed).unwrap().len(), 2);
    let unrelated_row = decisions
        .iter()
        .find(|row| row.identity == unrelated)
        .unwrap();
    assert_eq!(unrelated_row.status, Status::Ignored);
    assert_eq!(
        store.comments(Kind::Dead, &unrelated).unwrap()[0].body,
        "preserve this annotation"
    );
}

/// Real newline-delimited JSON-RPC transport with bounded reads and RAII cleanup.
/// The child only sees the fixture root; startup indexing and watching cannot
/// target the user's checkout. Build the workspace MCP binary before this test.
struct StdioMcp {
    child: std::process::Child,
    input: std::process::ChildStdin,
    messages: mpsc::Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
    id: u64,
}

impl StdioMcp {
    fn start(root: &Path) -> Self {
        use std::io::BufRead;
        let binary = std::env::var_os("MCT_TEST_MCP_BINARY").map_or_else(
            || {
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join(format!("mct-mcp-server{}", std::env::consts::EXE_SUFFIX))
            },
            std::path::PathBuf::from,
        );
        assert!(
            binary.is_file(),
            "build mct-mcp-server before this transport test"
        );
        let mut child = std::process::Command::new(binary)
            .args(["--root"])
            .arg(root)
            .args(["--reindex-delay", "off", "--no-obsidian"])
            .env_remove("MCT_TOOL_ATTENTION")
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, messages) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
            reader: Some(reader),
            id: 0,
        };
        let initialized = client.request(
            "initialize",
            json!({
                "protocolVersion":"2024-11-05", "capabilities":{},
                "clientInfo":{"name":"isolated-review-persistence", "version":"1"}
            }),
        );
        assert!(
            initialized["capabilities"]["tools"].is_object(),
            "{initialized}"
        );
        client.send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
        let tools = client.request("tools/list", json!({}));
        for name in ["find_dead_code", "find_duplicate_code"] {
            assert!(tools["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == name));
        }
        client
    }

    fn send(&mut self, message: serde_json::Value) {
        use std::io::Write;
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.id += 1;
        self.send(json!({"jsonrpc":"2.0", "id":self.id, "method":method, "params":params}));
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let line = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("MCP response must arrive within 30 seconds");
            let message: serde_json::Value = serde_json::from_str(&line).unwrap();
            if message["id"] == self.id {
                assert!(message.get("error").is_none(), "{message}");
                return message["result"].clone();
            }
        }
    }

    fn tool(&mut self, name: &str, arguments: serde_json::Value) -> String {
        let result = self.request("tools/call", json!({"name":name, "arguments":arguments}));
        assert_ne!(result["isError"], true, "{result}");
        result["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for StdioMcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn stdio_queries(root: &Path) -> (String, String) {
    let mut client = StdioMcp::start(root);
    let dead = client.tool("find_dead_code", json!({"format":"text", "limit":100}));
    let duplicates = client.tool("find_duplicate_code", json!({"limit":100}));
    // Duplicate output has its own TOON contract, without a format argument.
    assert!(duplicates.contains("generation:"));
    assert!(duplicates.contains("returned:"));
    let toon = client.tool("find_dead_code", json!({"format":"toon", "limit":100}));
    for name in ["a0", "a1", "a2", "b0", "b1", "b2", "c0", "c1", "c2"] {
        assert_eq!(
            dead.contains(name),
            toon.contains(name),
            "format parity for {name}"
        );
    }
    let first = client.tool(
        "find_dead_code",
        json!({"format":"text", "limit":1, "offset":0}),
    );
    let second = client.tool(
        "find_dead_code",
        json!({"format":"text", "limit":1, "offset":1}),
    );
    let dead_row = |page: &str| {
        page.lines()
            .find(|line| line.starts_with("  function "))
            .unwrap()
            .to_string()
    };
    assert_ne!(
        dead_row(&first),
        dead_row(&second),
        "pages must contain different symbols"
    );
    let first = client.tool(
        "find_duplicate_code",
        json!({"include_reviewed":true, "limit":1, "offset":0}),
    );
    let second = client.tool(
        "find_duplicate_code",
        json!({"include_reviewed":true, "limit":1, "offset":1}),
    );
    let group_id = |page: &str| {
        page.lines()
            .filter_map(|line| line.trim().split(',').next())
            .find(|field| field.len() == 64 && field.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .unwrap()
            .to_string()
    };
    assert_ne!(
        group_id(&first),
        group_id(&second),
        "pages must contain different groups"
    );
    (dead, duplicates)
}

#[test]
fn sqlite_reviews_survive_reindex_and_restart_over_real_mcp_stdio() {
    const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    for (group, repeated) in [("a", 0), ("b", 1), ("c", 2)] {
        let body = BODY.replace(
            "total += 8;",
            &format!("{}total += 8;", "total *= 3;\n".repeat(repeated)),
        );
        for copy in 0..3 {
            fs::write(
                root.path().join(format!("src/{group}{copy}.rs")),
                format!("pub fn {group}{copy}{body}"),
            )
            .unwrap();
        }
    }
    let initial_index = reindex_project(root.path(), false);
    assert_eq!(initial_index.files_parsed, 9);

    let (initial_dead, initial_duplicates) = stdio_queries(root.path());
    assert!(initial_dead.contains("a0") && initial_dead.contains("a1"));
    assert!(initial_duplicates.contains("actionable: 3"));

    let dashboard = Server::start(root.path(), true);
    let dead_false_positive = dashboard.dead_id("a0");
    let dead_ignored = dashboard.dead_id("a1");
    let group_id = |prefix: &str| {
        let site = dashboard.site.borrow();
        let group = site
            .duplicates
            .groups
            .iter()
            .position(|group| {
                group
                    .members
                    .iter()
                    .any(|member| member.path.ends_with(&format!("/{prefix}0.rs")))
            })
            .unwrap();
        site.duplicate_keys[group].identity.clone()
    };
    let duplicate_false_positive = group_id("a");
    let duplicate_ignored = group_id("b");
    let duplicate_pending = group_id("c");

    for (kind, identity, action) in [
        ("dead", dead_false_positive.as_str(), "false_positive"),
        ("dead", dead_ignored.as_str(), "ignored"),
        (
            "duplicate",
            duplicate_false_positive.as_str(),
            "false_positive",
        ),
        ("duplicate", duplicate_ignored.as_str(), "ignored"),
    ] {
        assert_status(
            &dashboard.review(kind, identity, action, "phase 5 stdio SQLite persistence"),
            "303",
            "dashboard review write",
        );
    }
    let records = saved_decisions(root.path());
    assert_eq!(records.len(), 4);
    assert!(records.iter().any(|r| r.kind == Kind::Dead
        && r.identity == dead_false_positive
        && r.status == Status::FalsePositive));
    assert!(records.iter().any(|r| r.kind == Kind::Dead
        && r.identity == dead_ignored
        && r.status == Status::Ignored));
    assert!(records.iter().any(|r| r.kind == Kind::Duplicate
        && r.identity == duplicate_false_positive
        && r.status == Status::FalsePositive));
    assert!(records.iter().any(|r| r.kind == Kind::Duplicate
        && r.identity == duplicate_ignored
        && r.status == Status::Ignored));
    assert!(!root.path().join(".mct-index/reviews.toon").exists());

    drop(dashboard);
    let forced = reindex_project(root.path(), true);
    assert_eq!(forced.files_parsed, 9);
    let after_forced = saved_decisions(root.path());
    assert_eq!(after_forced.len(), 4);
    for saved in &records {
        assert!(after_forced.iter().any(|current| {
            current.kind == saved.kind
                && current.identity == saved.identity
                && current.fingerprint == saved.fingerprint
                && current.status == saved.status
        }));
    }
    let (forced_dead, forced_duplicates) = stdio_queries(root.path());
    assert!(!forced_dead.contains("a0") && !forced_dead.contains("a1"));
    assert!(forced_dead.contains("c0"));
    assert!(!forced_duplicates.contains(&duplicate_false_positive));
    assert!(!forced_duplicates.contains(&duplicate_ignored));
    assert!(forced_duplicates.contains(&duplicate_pending));
    assert!(forced_duplicates.contains("actionable: 1"));

    let incremental = reindex_project(root.path(), false);
    assert_eq!(incremental.files_parsed, 0);
    assert_eq!(incremental.files_unchanged, 9);
    assert_eq!(saved_decisions(root.path()).len(), 4);

    // Reopening Store and checking the exact persisted rows precedes the final
    // MCP request; both MCP tools read the same dashboard SQLite database.
    let reopened = open_store(root.path());
    let reopened_records = reopened.decisions().unwrap();
    assert_eq!(reopened_records.len(), 4);
    assert!(reopened_records
        .iter()
        .any(|record| record.identity == duplicate_false_positive));
    drop(reopened);

    let (dead, duplicates) = stdio_queries(root.path());
    assert!(!dead.contains("a0") && !dead.contains("a1"), "{dead}");
    assert!(
        dead.contains("c0"),
        "unreviewed dead code must remain visible: {dead}"
    );
    assert!(
        dead.contains("suppressed by valid saved review decisions"),
        "{dead}"
    );
    assert!(
        !duplicates.contains(&duplicate_false_positive),
        "{duplicates}"
    );
    assert!(!duplicates.contains(&duplicate_ignored), "{duplicates}");
    assert!(
        duplicates.contains(&duplicate_pending),
        "unreviewed duplicate group must remain visible: {duplicates}"
    );
    assert!(duplicates.contains("actionable: 1"), "{duplicates}");
    assert_eq!(saved_decisions(root.path()).len(), 4);

    // Material edits invalidate only their matching reviewed fingerprints.
    let dead_file = root.path().join("src/a0.rs");
    fs::write(
        &dead_file,
        format!("pub fn a0{}", BODY.replace("total += 1", "total += 101")),
    )
    .unwrap();
    let duplicate_file = root.path().join("src/a1.rs");
    fs::write(
        &duplicate_file,
        format!("pub fn a1{}", BODY.replace("total += 1", "total += 101")),
    )
    .unwrap();
    reindex_project(root.path(), false);
    let (changed_dead, changed_duplicates) = stdio_queries(root.path());
    assert!(
        changed_dead.contains("a0"),
        "changed dead finding must be actionable: {changed_dead}"
    );
    assert!(
        changed_duplicates.contains("actionable: 2"),
        "changed duplicate group and untouched group must remain actionable: {changed_duplicates}"
    );
    assert!(
        changed_dead.contains("a1"),
        "changed ignored dead finding must be actionable"
    );
    assert!(
        !changed_duplicates.contains(&duplicate_ignored),
        "unrelated dismissal remains valid"
    );
    assert!(changed_duplicates.contains(&duplicate_pending));
    assert!(
        changed_duplicates.contains("src/a0.rs") && changed_duplicates.contains("src/a1.rs"),
        "materially changed duplicate members must be actionable: {changed_duplicates}"
    );
    let historical = saved_decisions(root.path());
    assert_eq!(historical.len(), 4);
    for saved in &records {
        assert!(historical.iter().any(|row| row.kind == saved.kind
            && row.identity == saved.identity
            && row.fingerprint == saved.fingerprint
            && row.status == saved.status));
    }
    assert!(root.path().join(".mct-index/dashboard.sqlite3").is_file());
    assert!(!root.path().join(".mct-index/reviews.toon").exists());
    assert!(!root
        .path()
        .join(".mct-index/duplicate-reviews.toon")
        .exists());
}
