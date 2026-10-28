//! `find_duplicate_code`: review-aware duplicate groups, using the real
//! detector and decisions saved through the shared TOON review store.
//!
//! Each test builds its own throwaway project so nothing is shared.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use mct_index::reviews::{duplicate_findings, Change, Reviews, State};
use mct_index::{DuplicateOptions, ExcludeSet, Index};
use mct_mcp_server::server::{FindDuplicateCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

static NEXT: AtomicU64 = AtomicU64::new(0);

const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

/// `groups` independent duplicate groups of `copies` identical functions each.
struct Project(PathBuf);

impl Project {
    fn new(groups: usize, copies: usize) -> Self {
        let path = std::env::temp_dir().join(format!(
            "mct-mcp-duplicate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".mct-index")).unwrap();
        let this = Self(path.canonicalize().unwrap());
        for g in 0..groups {
            for c in 0..copies {
                // Each group differs from the others by `g` extra statements.
                let body = BODY.replace(
                    "total += 8;",
                    &format!("{}total += 8;", "total *= 3;\n".repeat(g)),
                );
                fs::write(
                    this.0.join(format!("g{g}c{c}.rs")),
                    format!("pub fn f{g}_{c}{body}"),
                )
                .unwrap();
            }
        }
        this
    }

    fn index(&self) -> Index {
        let mut index = Index::open_in_memory(&self.0, ExcludeSet::default()).unwrap();
        index
            .reindex(&mct_mcp_server::registry::build_registry(), false)
            .unwrap();
        index
    }

    fn server(&self) -> MctServer {
        MctServer::new(self.index(), mct_mcp_server::registry::build_registry())
    }

    /// Saves `state` for the group at `position` in the detector's order,
    /// the way the dashboard and the CLI do.
    fn decide(&self, position: usize, state: State, note: &str) {
        let index = self.index();
        let report = mct_index::find_duplicate_groups(
            &index,
            &mct_languages::build_token_sources(),
            DuplicateOptions::default(),
        )
        .unwrap();
        let findings = duplicate_findings(
            &report.groups,
            report.options.normalization,
            &index.file_hashes().unwrap(),
        );
        let mut reviews = Reviews::load(&self.0);
        let revision = reviews.resolve(&findings)[position].revision();
        reviews
            .save(
                &findings,
                position,
                &Change {
                    state,
                    note: note.to_string(),
                    expected_revision: revision,
                },
                1_700_000_000_000,
            )
            .unwrap();
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args() -> FindDuplicateCodeArgs {
    FindDuplicateCodeArgs::default()
}

async fn query(server: &MctServer, args: FindDuplicateCodeArgs) -> String {
    let result = server.find_duplicate_code(Parameters(args)).await.unwrap();
    result.content[0].as_text().unwrap().text.clone()
}

#[tokio::test]
async fn pending_confirmed_and_suppressing_decisions_are_distinct() {
    let p = Project::new(1, 3);
    let server = p.server();
    let pending = query(&server, args()).await;
    assert!(
        pending.contains("detected: 1\nactionable: 1\nsuppressed: 0"),
        "{pending}"
    );
    // Confirmed and in-review stay on the to-do list; false-positive and
    // dismissed hide the group while the code is unchanged.
    for (state, hidden) in [
        (State::Confirmed, false),
        (State::InReview, false),
        (State::FalsePositive, true),
        (State::Dismissed, true),
    ] {
        p.decide(0, state, "private note must never leak");
        let text = query(&server, args()).await;
        let expected = if hidden {
            "status: all_suppressed"
        } else {
            "status: groups"
        };
        assert!(text.contains(expected), "{} {text}", state.value());
        let all = query(
            &server,
            FindDuplicateCodeArgs {
                include_reviewed: Some(true),
                ..args()
            },
        )
        .await;
        assert!(all.contains(state.value()), "{all}");
        assert!(!all.contains("private note"), "{all}");
    }
}

#[tokio::test]
async fn an_edit_to_a_member_brings_a_dismissed_group_back_for_reevaluation() {
    let p = Project::new(1, 2);
    p.decide(0, State::FalsePositive, "");
    assert!(query(&p.server(), args())
        .await
        .contains("status: all_suppressed"));

    // The decision was about the old code: it must not hide the new code.
    fs::write(
        p.0.join("g0c0.rs"),
        format!("pub fn f0_0{}// edited\n", BODY),
    )
    .unwrap();
    let text = query(&p.server(), args()).await;
    assert!(text.contains("actionable: 1\nsuppressed: 0"), "{text}");
    assert!(text.contains("needs-reevaluation"), "{text}");
}

#[tokio::test]
async fn dismissals_are_filtered_before_limit_and_offset() {
    let p = Project::new(3, 2);
    let first = query(
        &p.server(),
        FindDuplicateCodeArgs {
            limit: Some(1),
            ..args()
        },
    )
    .await;
    assert!(first.contains("detected: 3"), "{first}");
    assert!(first.contains("returned: 1"), "{first}");
    let top = first
        .lines()
        .find(|l| l.contains("g") && l.contains(".rs"))
        .unwrap()
        .to_string();

    p.decide(0, State::Dismissed, "");
    let page = query(
        &p.server(),
        FindDuplicateCodeArgs {
            limit: Some(1),
            ..args()
        },
    )
    .await;
    // The dismissed group no longer takes up the first page.
    assert!(
        page.contains("detected: 3\nactionable: 2\nsuppressed: 1"),
        "{page}"
    );
    assert!(page.contains("returned: 1"), "{page}");
    assert!(page.contains("next_offset: 1"), "{page}");
    assert!(!page.contains(&top), "{page}");

    let last = query(
        &p.server(),
        FindDuplicateCodeArgs {
            limit: Some(1),
            offset: Some(1),
            ..args()
        },
    )
    .await;
    assert!(last.contains("next_offset: none"), "{last}");
}

#[tokio::test]
async fn nothing_found_is_not_the_same_as_everything_suppressed() {
    let none = Project::new(0, 0);
    let text = query(&none.server(), args()).await;
    assert!(text.contains("status: no_groups"), "{text}");

    let p = Project::new(1, 2);
    p.decide(0, State::Dismissed, "");
    let text = query(&p.server(), args()).await;
    assert!(text.contains("status: all_suppressed"), "{text}");
    assert!(
        text.contains("detected: 1\nactionable: 0\nsuppressed: 1"),
        "{text}"
    );
}

#[tokio::test]
async fn an_unreadable_review_file_hides_nothing_and_says_so() {
    let p = Project::new(1, 2);
    p.decide(0, State::Dismissed, "");
    fs::write(
        p.0.join(".mct-index/review-decisions.toon"),
        "not toon at all",
    )
    .unwrap();
    let text = query(&p.server(), args()).await;
    assert!(text.contains("review_store: degraded"), "{text}");
    assert!(text.contains("actionable: 1\nsuppressed: 0"), "{text}");
}

#[tokio::test]
async fn invalid_parameters_are_rejected_before_any_analysis() {
    let server = Project::new(1, 2).server();
    for bad in [
        FindDuplicateCodeArgs {
            limit: Some(0),
            ..args()
        },
        FindDuplicateCodeArgs {
            limit: Some(101),
            ..args()
        },
        FindDuplicateCodeArgs {
            max_tokens: Some(100),
            ..args()
        },
        FindDuplicateCodeArgs {
            min_lines: Some(0),
            ..args()
        },
        FindDuplicateCodeArgs {
            path: Some("../outside".into()),
            ..args()
        },
        FindDuplicateCodeArgs {
            path: Some("/etc".into()),
            ..args()
        },
        FindDuplicateCodeArgs {
            normalization: Some("fuzzy".into()),
            ..args()
        },
        FindDuplicateCodeArgs {
            language: Some("rust; drop".into()),
            ..args()
        },
    ] {
        let error = server
            .find_duplicate_code(Parameters(bad))
            .await
            .unwrap_err();
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
}

#[tokio::test]
async fn a_small_budget_never_silently_drops_members() {
    let p = Project::new(1, 40);
    let text = query(
        &p.server(),
        FindDuplicateCodeArgs {
            max_tokens: Some(512),
            ..args()
        },
    )
    .await;
    assert!(text.contains("truncated: true"), "{text}");
    assert!(text.contains("members_omitted:"), "{text}");
    assert!(!text.contains("members_omitted: 0"), "{text}");
    assert!(
        text.contains("next_offset: 0"),
        "omitted members must be recoverable: {text}"
    );
    assert!(text.contains("increase max_tokens"), "{text}");
}

#[tokio::test]
async fn the_tool_is_dispatchable_through_batch() {
    let p = Project::new(1, 2);
    let server = p.server();
    let result = server
        .batch(Parameters(
            serde_json::from_value(serde_json::json!({
                "queries": [{"tool": "find_duplicate_code", "args": {}}]
            }))
            .unwrap(),
        ))
        .await
        .unwrap();
    let text = result.content[0].as_text().unwrap().text.clone();
    assert!(text.contains("detected: 1"), "{text}");
}
