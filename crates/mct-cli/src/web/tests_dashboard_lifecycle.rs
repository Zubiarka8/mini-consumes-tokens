//! Dashboard JSON contracts and durable SQLite state; no socket/browser claims.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{api_json, open_store, project, saved_decisions, Server};
use crate::web::reviews::Kind;
use mct_index::{ExcludeSet, Index};
use serde_json::{json, Value};

fn detail(server: &Server, kind: &str, id: &str) -> Value {
    api_json(&server.get(&format!("api/finding?kind={kind}&id={id}")))
}

fn write_decision(server: &Server, kind: &str, id: &str, action: &str) {
    // Capture precisely what the displayed detail exposes; do not let the
    // harness fill missing concurrency/fingerprint fields for this test.
    let seen = detail(server, kind, id);
    let response = api_json(&server.write(
        "api/review",
        &json!({"kind":kind,"id":id,"action":action,"reason":"intentional",
            "fingerprint":seen["fingerprint"],"expected_revision":seen["revision"]}),
    ));
    let expected = if action == "restore" {
        "pending"
    } else {
        action
    };
    assert_eq!(response["state"]["status"], expected);
}

fn assert_visibility(server: &Server, kind: &str, id: &str, status: &str) {
    let route = if kind == "dead" { "dead" } else { "duplicates" };
    let pending = api_json(&server.get(&format!("api/{route}?status=pending")));
    let contains = |page: &Value| {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["identity"] == id)
    };
    assert_eq!(contains(&pending), status == "pending", "{pending}");
    let dismissed = api_json(&server.get(&format!("api/{route}?status={status}")));
    assert!(contains(&dismissed), "{dismissed}");
    assert_eq!(detail(server, kind, id)["state"]["status"], status);
}

#[test]
fn json_decisions_remain_reviewable_through_reopen_repeated_reindex_and_restore() {
    for kind in ["dead", "duplicate"] {
        for status in ["false_positive", "ignored"] {
            let root = project();
            let server = Server::start(root.path(), true);
            let id = if kind == "dead" {
                server.dead_id("alpha")
            } else {
                server.duplicate_id(0)
            };
            write_decision(&server, kind, &id, status);
            assert_visibility(&server, kind, &id, status);
            let records = saved_decisions(root.path());
            assert_eq!(records.len(), 1);
            let parsed_kind = Kind::parse(kind).unwrap();
            let history = open_store(root.path()).history(parsed_kind, &id).unwrap();
            assert_eq!(history.len(), 1);
            drop(server);

            // Fresh Site/Store simulates server state reopening, not a real
            // process restart or a browser reload. Both still need acceptance.
            for force in [false, true, false, true] {
                let mut writer = Index::open(
                    root.path(),
                    &root.path().join(".mct-index/index.sqlite3"),
                    ExcludeSet::default(),
                )
                .unwrap();
                writer
                    .reindex(&mct_languages::build_registry(), force)
                    .unwrap();
                drop(writer);
                let reopened = Server::start(root.path(), true);
                assert_visibility(&reopened, kind, &id, status);
                // A dismissal never suppresses the unrelated beta candidate.
                let beta = reopened.dead_id("beta");
                assert_visibility(&reopened, "dead", &beta, "pending");
                assert_eq!(saved_decisions(root.path()), records);
                assert_eq!(
                    open_store(root.path()).history(parsed_kind, &id).unwrap(),
                    history
                );
            }

            let reopened = Server::start(root.path(), true);
            write_decision(&reopened, kind, &id, "restore");
            assert_visibility(&reopened, kind, &id, "pending");
            drop(reopened);
            let reopened = Server::start(root.path(), true);
            assert_visibility(&reopened, kind, &id, "pending");
            assert!(saved_decisions(root.path()).is_empty());
            assert_eq!(
                open_store(root.path())
                    .history(parsed_kind, &id)
                    .unwrap()
                    .len(),
                2
            );
        }
    }
}
