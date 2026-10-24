//! Decision-write contracts from the dashboard adversarial audit (AUD-01,
//! AUD-04, AUD-05, AUD-11, AUD-12): every write path is exercised through the
//! real HTTP handlers on a disposable project, for both the JSON API and the
//! server-rendered forms.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use serde_json::json;

use super::super::reviews::{Kind, State, Status};
use super::{
    api_json, fingerprint_like_adr_015, index, open_store, project, saved_decisions, Server,
};

const ORIGIN: &str = "Origin: http://127.0.0.1:8787\r\n";

fn status_line(response: &str) -> &str {
    response.lines().next().unwrap_or_default()
}

fn assert_status(response: &str, expected: &str, context: &str) {
    assert!(
        status_line(response).starts_with(&format!("HTTP/1.1 {expected}")),
        "{context}: {response}"
    );
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
