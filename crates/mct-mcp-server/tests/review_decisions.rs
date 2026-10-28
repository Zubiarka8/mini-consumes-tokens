//! Shared TOON decisions must mean the same thing through MCP and the CLI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::reviews::{dead_code_findings, Change, Reviews, State};
use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-dead-reviews-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".mct-index")).unwrap();
        fs::write(
            root.join("lib.rs"),
            "pub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n",
        )
        .unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn index(&self) -> Index {
        let mut index = Index::open_in_memory(&self.0, ExcludeSet::default()).unwrap();
        index
            .reindex(&mct_languages::build_registry(), true)
            .unwrap();
        index
    }
    fn server(&self) -> MctServer {
        MctServer::new(self.index(), mct_languages::build_registry())
    }
    fn decide(&self, state: State) {
        let index = self.index();
        let scan = mct_index::scan_dead_code(&index, None, None).unwrap();
        let findings = dead_code_findings(
            &scan.symbols,
            &scan.candidates,
            &index.file_hashes().unwrap(),
        );
        let mut reviews = Reviews::load(&self.0);
        let revision = reviews.resolve(&findings)[0].revision();
        reviews
            .save(
                &findings,
                0,
                &Change {
                    state,
                    note: "private-note".into(),
                    expected_revision: revision,
                },
                1700000000000,
            )
            .unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
async fn query(server: &MctServer, args: serde_json::Value) -> String {
    server
        .call_tool_by_name("find_dead_code", args.as_object().cloned())
        .await
        .unwrap()
        .content[0]
        .as_text()
        .unwrap()
        .text
        .clone()
}
#[tokio::test]
async fn states_are_resolved_without_leaking_private_notes() {
    let p = Project::new();
    let server = p.server();
    for (state, suppressed) in [
        (State::Unreviewed, 0),
        (State::InReview, 0),
        (State::Confirmed, 0),
        (State::FalsePositive, 1),
        (State::Dismissed, 1),
        (State::Resolved, 0),
        (State::Unreviewed, 0),
    ] {
        p.decide(state);
        for format in ["text", "toon"] {
            let text = query(&server, json!({"format":format,"include_reviewed":true})).await;
            assert!(
                text.contains(&format!("suppressed: {suppressed}")),
                "{text}"
            );
            let label = if state == State::Resolved {
                "resolved-but-detected"
            } else {
                state.value()
            };
            assert!(text.contains(label), "{text}");
            assert!(!text.contains("private-note"));
        }
    }
}
#[tokio::test]
async fn review_filtering_precedes_pagination_and_inclusion_restores_hidden_rows() {
    let p = Project::new();
    p.decide(State::Dismissed);
    let server = p.server();
    let page = query(&server, json!({"limit":1})).await;
    assert!(
        page.contains("detected: 3\nactionable: 2\nsuppressed: 1"),
        "{page}"
    );
    assert!(!page.contains(",alpha,"), "{page}");
    assert!(page.contains(",beta,"), "{page}");
    assert!(page.contains("next_offset: 1"), "{page}");
    let last = query(&server, json!({"limit":1,"offset":1})).await;
    assert!(last.contains(",gamma,"), "{last}");
    assert!(last.contains("next_offset: none"), "{last}");
    let all = query(&server, json!({"include_reviewed":true,"limit":1})).await;
    assert!(all.contains(",alpha,"), "{all}");
    assert!(all.contains("dismissed,false"), "{all}");
}
#[tokio::test]
async fn an_edit_and_a_degraded_store_never_hide_candidates() {
    let p = Project::new();
    p.decide(State::FalsePositive);
    fs::write(
        p.0.join("lib.rs"),
        "// shifted\npub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n",
    )
    .unwrap();
    let text = query(&p.server(), json!({})).await;
    assert!(text.contains("needs-reevaluation"), "{text}");
    assert!(text.contains("suppressed: 0"), "{text}");
    fs::write(p.0.join(".mct-index/review-decisions.toon"), "corrupt").unwrap();
    let text = query(&p.server(), json!({})).await;
    assert!(text.contains("review_store: degraded"), "{text}");
    assert!(text.contains("actionable: 3"), "{text}");
}
#[tokio::test]
async fn a_broken_legacy_file_keeps_the_decision_visible_but_holds_the_suppression() {
    let p = Project::new();
    p.decide(State::FalsePositive);
    let healthy = query(&p.server(), json!({})).await;
    assert!(healthy.contains("suppressed: 1"), "{healthy}");
    assert!(healthy.contains("review_store: ok"), "{healthy}");
    fs::write(p.0.join(".mct-index/reviews.toon"), "version: 2\ntruncated").unwrap();
    let held = query(&p.server(), json!({})).await;
    assert!(held.contains("review_store: degraded"), "{held}");
    assert!(held.contains("suppressed: 0"), "{held}");
    assert!(held.contains("actionable: 3"), "{held}");
    assert!(held.contains(",alpha,"), "{held}");
    assert!(held.contains("needs-reevaluation"), "{held}");
    assert!(!held.contains("false-positive"), "{held}");
    fs::remove_file(p.0.join(".mct-index/reviews.toon")).unwrap();
    let repaired = query(&p.server(), json!({})).await;
    assert!(repaired.contains("suppressed: 1"), "{repaired}");
    assert!(repaired.contains("review_store: ok"), "{repaired}");
}
#[tokio::test]
async fn empty_and_suppressed_are_distinct_and_offset_overflow_is_safe() {
    let p = Project::new();
    let server = p.server();
    let empty = query(&server, json!({"language":"go"})).await;
    assert!(empty.contains("status: no_candidates"), "{empty}");
    let beyond = query(&server, json!({"offset":usize::MAX})).await;
    assert!(beyond.contains("returned: 0"), "{beyond}");
    assert!(beyond.contains("next_offset: none"), "{beyond}");
}

#[tokio::test]
async fn every_candidate_suppressed_is_reported_and_can_be_restored() {
    let p = Project::new();
    fs::write(p.0.join("lib.rs"), "pub fn alpha() {}\n").unwrap();
    p.decide(State::Dismissed);
    let server = p.server();
    let hidden = query(&server, json!({})).await;
    assert!(hidden.contains("status: all_suppressed"), "{hidden}");
    assert!(hidden.contains("returned: 0"), "{hidden}");
    p.decide(State::Unreviewed);
    let restored = query(&server, json!({})).await;
    assert!(restored.contains("actionable: 1"), "{restored}");
    assert!(restored.contains(",alpha,"), "{restored}");
}

#[tokio::test]
async fn budget_validation_and_truncation_are_explicit() {
    let p = Project::new();
    fs::write(
        p.0.join("lib.rs"),
        (0..100)
            .map(|i| format!("pub fn unused_{i}() {{}}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let server = p.server();
    for format in ["toon", "text"] {
        let text = query(
            &server,
            json!({"limit":100,"max_tokens":512,"format":format}),
        )
        .await;
        assert!(text.contains("truncated: true"), "{text}");
        assert!(!text.contains("next_offset: none"), "{text}");
        use mct_index::TokenCounter;
        assert!(
            mct_index::EstimatedCounter.count(&text).value <= 512,
            "{text}"
        );
    }
    let error = server
        .call_tool_by_name(
            "find_dead_code",
            json!({"max_tokens":0}).as_object().cloned(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}
