//! Review-aware duplicate discovery, using real detector and persisted fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::{review_keys, DuplicateOptions, ExcludeSet, Index};
use mct_index::{EstimatedCounter, TokenCounter};
use mct_mcp_server::server::{FindDuplicateCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
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
            .reindex(&mct_languages::build_registry(), false)
            .unwrap();
        index
    }
    fn server(&self) -> MctServer {
        MctServer::new(self.index(), mct_languages::build_registry())
    }
    fn keys(&self) -> (Vec<String>, Vec<String>) {
        let index = self.index();
        let report = mct_index::find_duplicate_groups(
            &index,
            &mct_languages::build_token_sources(),
            DuplicateOptions::default(),
        )
        .unwrap();
        let ids = review_keys::group_identities(&report.groups);
        let fps = review_keys::Fingerprints::new(&self.0, &index.navigation_snapshot().unwrap())
            .group_keys(&report.groups, report.options.normalization);
        (ids, fps)
    }
    fn save(&self, status: &str) -> Vec<u8> {
        let (ids, fps) = self.keys();
        let rows = ids
            .iter()
            .zip(fps)
            .map(|(id, fp)| {
                format!("  duplicate,{id},{fp},{status},1,private note must never leak\n")
            })
            .collect::<String>();
        let text=format!("version: 2\nreviews[{}]{{kind,identity,fingerprint,status,at,reason}}:\n{rows}history[0]{{kind,identity,fingerprint,status,at,reason}}:\n",ids.len());
        fs::write(self.0.join(".mct-index/reviews.toon"), &text).unwrap();
        text.into_bytes()
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
async fn pending_confirmed_and_suppressed_decisions_are_distinct_and_read_only() {
    let p = Project::new(1, 3);
    let s = p.server();
    let pending = query(&s, args()).await;
    assert!(
        pending.contains("detected: 1\nactionable: 1\nsuppressed: 0"),
        "{pending}"
    );
    for (status, suppressed) in [
        ("confirmed", false),
        ("false_positive", true),
        ("ignored", true),
        ("pending", false),
    ] {
        let saved = p.save(status);
        let text = query(&s, args()).await;
        assert!(
            text.contains(if suppressed {
                "status: all_suppressed"
            } else {
                "status: groups"
            }),
            "{text}"
        );
        let all = query(
            &s,
            FindDuplicateCodeArgs {
                include_reviewed: Some(true),
                ..args()
            },
        )
        .await;
        assert!(all.contains(status), "{all}");
        assert!(!all.contains("private note"));
        assert_eq!(
            fs::read(p.0.join(".mct-index/reviews.toon")).unwrap(),
            saved
        );
        assert!(!p.0.join(".mct-index/dashboard.sqlite3").exists());
    }
}
#[tokio::test]
async fn unchanged_reindex_and_line_movement_preserve_dismissal_material_edits_invalidate() {
    let p = Project::new(1, 2);
    p.save("false_positive");
    let server = p.server();
    let handle = server.index_handle();
    handle
        .lock()
        .await
        .reindex(&mct_languages::build_registry(), true)
        .unwrap();
    assert!(query(&server, args())
        .await
        .contains("status: all_suppressed"));
    let file = p.0.join("g0c0.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("// line shift\n{original}")).unwrap();
    handle
        .lock()
        .await
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(query(&server, args())
        .await
        .contains("status: all_suppressed"));
    fs::write(&file, original.replace("+= 1", "+= 99")).unwrap();
    handle
        .lock()
        .await
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    let changed = query(&server, args()).await;
    assert!(changed.contains("status: groups"), "{changed}");
    assert!(
        changed.contains("false_positive,pending,changed,true,true"),
        "{changed}"
    );
    fs::write(&file, &original).unwrap();
    handle
        .lock()
        .await
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(query(&server, args())
        .await
        .contains("status: all_suppressed"));
}
#[tokio::test]
async fn renames_membership_changes_and_unavailable_sources_never_inherit_suppression() {
    let p = Project::new(1, 2);
    let saved = p.save("ignored");
    fs::rename(p.0.join("g0c0.rs"), p.0.join("renamed.rs")).unwrap();
    assert!(query(&p.server(), args()).await.contains("actionable: 1"));
    fs::rename(p.0.join("renamed.rs"), p.0.join("g0c0.rs")).unwrap();
    fs::write(p.0.join("new.rs"), format!("pub fn another{BODY}")).unwrap();
    assert!(query(&p.server(), args()).await.contains("actionable: 1"));
    fs::remove_file(p.0.join("new.rs")).unwrap();
    let s = p.server();
    fs::remove_file(p.0.join("g0c0.rs")).unwrap();
    let missing = query(&s, args()).await;
    assert!(missing.contains("coverage_complete: false"), "{missing}");
    assert!(missing.contains("stale-location"), "{missing}");
    assert_eq!(
        fs::read(p.0.join(".mct-index/reviews.toon")).unwrap(),
        saved
    );
}
#[tokio::test]
async fn filters_and_pagination_are_deterministic_and_counts_precede_pagination() {
    let p = Project::new(3, 2);
    let s = p.server();
    let (ids, _) = p.keys();
    let mut pages = Vec::new();
    for offset in 0..3 {
        let a = FindDuplicateCodeArgs {
            limit: Some(1),
            offset: Some(offset),
            ..args()
        };
        let text = query(&s, a).await;
        assert!(text.contains("detected: 3\nactionable: 3"), "{text}");
        assert_eq!(
            ids.iter().filter(|id| text.contains(id.as_str())).count(),
            1
        );
        pages.push(text);
    }
    assert_ne!(pages[0], pages[1]);
    assert_eq!(
        pages[0],
        query(
            &s,
            FindDuplicateCodeArgs {
                limit: Some(1),
                ..args()
            }
        )
        .await
    );
    p.save("ignored");
    let hidden = query(
        &s,
        FindDuplicateCodeArgs {
            offset: Some(999),
            ..args()
        },
    )
    .await;
    assert!(hidden.contains("status: all_suppressed"));
    assert!(hidden.contains("suppressed: 3"));
    assert!(query(
        &s,
        FindDuplicateCodeArgs {
            language: Some("not-a-language".into()),
            ..args()
        }
    )
    .await
    .contains("status: no_groups"));
    assert!(query(
        &s,
        FindDuplicateCodeArgs {
            path: Some("g0c0.rs".into()),
            ..args()
        }
    )
    .await
    .contains("status: no_groups"));
}
#[tokio::test]
async fn estimated_budget_caps_large_groups_and_never_silently_discards_members() {
    let p = Project::new(1, 30);
    let s = p.server();
    let text = query(
        &s,
        FindDuplicateCodeArgs {
            max_tokens: Some(512),
            ..args()
        },
    )
    .await;
    assert!(EstimatedCounter.count(&text).value <= 512, "{text}");
    assert!(text.contains("truncated: true"), "{text}");
    assert!(!text.contains("members_omitted: 0"), "{text}");
    let large = query(
        &s,
        FindDuplicateCodeArgs {
            max_tokens: Some(16000),
            ..args()
        },
    )
    .await;
    assert!(large.contains("members[30]"));
    assert!(large.contains("members_omitted: 0"));
    let beyond = query(
        &s,
        FindDuplicateCodeArgs {
            offset: Some(usize::MAX),
            ..args()
        },
    )
    .await;
    assert!(beyond.contains("returned: 0"));
}
#[tokio::test]
async fn invalid_parameters_fail_before_analysis() {
    let p = Project::new(0, 0);
    let s = p.server();
    for raw in [
        r#"{"limit":0}"#,
        r#"{"limit":101}"#,
        r#"{"max_tokens":511}"#,
        r#"{"min_lines":0}"#,
        r#"{"normalization":"similar"}"#,
        r#"{"path":"../secret"}"#,
        r#"{"path":"/private"}"#,
        r#"{"path":"C:\\secret"}"#,
        r#"{"language":"bad language"}"#,
    ] {
        let a: FindDuplicateCodeArgs = serde_json::from_str(raw).unwrap();
        assert!(s.find_duplicate_code(Parameters(a)).await.is_err(), "{raw}");
    }
    assert!(serde_json::from_str::<FindDuplicateCodeArgs>(r#"{"similarity":0.8}"#).is_err());
}
#[tokio::test]
async fn corrupt_review_state_is_not_silently_bypassed() {
    let p = Project::new(1, 2);
    let s = p.server();
    fs::write(p.0.join(".mct-index/reviews.toon"), "corrupt").unwrap();
    assert!(s.find_duplicate_code(Parameters(args())).await.is_err());
}
#[tokio::test]
async fn ambiguous_identities_and_unverified_fingerprints_are_actionable() {
    let p = Project::new(1, 2);
    p.save("ignored");
    let i = p.index();
    let report = mct_index::find_duplicate_groups(
        &i,
        &mct_languages::build_token_sources(),
        DuplicateOptions::default(),
    )
    .unwrap();
    let navigation = i.navigation_snapshot().unwrap();
    let reviews = mct_index::review_state::ReviewSnapshot::load(&p.0).unwrap();
    let twice = vec![report.groups[0].clone(), report.groups[0].clone()];
    let states = reviews
        .duplicate_states(&p.0, &navigation, &twice, report.options.normalization)
        .unwrap();
    assert!(states
        .iter()
        .all(|s| s.actionable && s.validity == "ambiguous"));
    fs::write(p.0.join("g0c0.rs"), "// changed outside index").unwrap();
    let states = reviews
        .duplicate_states(
            &p.0,
            &navigation,
            &report.groups,
            report.options.normalization,
        )
        .unwrap();
    assert!(states[0].actionable);
    assert!(!states[0].source_verified);
    assert_eq!(states[0].validity, "unavailable");
}
#[tokio::test]
async fn real_mcp_client_lists_and_calls_the_read_only_tool_and_batch() {
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    let p = Project::new(1, 2);
    let s = p.server();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let task = tokio::spawn(async move {
        s.serve(server_io).await.unwrap().waiting().await.unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|t| t.name == "find_duplicate_code")
        .unwrap();
    assert_eq!(
        tool.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    assert_eq!(
        tool.input_schema.get("additionalProperties"),
        Some(&serde_json::Value::Bool(false))
    );
    let result = client
        .call_tool(CallToolRequestParams::new("find_duplicate_code"))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    assert!(result.content[0]
        .as_text()
        .unwrap()
        .text
        .contains("detected: 1"));
    let bad = client
        .call_tool(
            CallToolRequestParams::new("find_duplicate_code").with_arguments(
                serde_json::json!({"offset":-1})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await;
    assert!(bad.is_err() || bad.unwrap().is_error == Some(true));
    let batch = client
        .call_tool(
            CallToolRequestParams::new("batch").with_arguments(
                serde_json::json!({"queries":[{"tool":"find_duplicate_code","args":{}}]})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert!(batch.content[0]
        .as_text()
        .unwrap()
        .text
        .contains("detected: 1"));
    client.cancel().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn queued_query_observes_the_completed_generation_after_a_writer() {
    let p = Project::new(1, 2);
    let server = std::sync::Arc::new(p.server());
    let handle = server.index_handle();
    let mut writer = handle.lock().await;
    let old = writer.published_generation().unwrap();
    let reader = std::sync::Arc::clone(&server);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        ready_tx.send(()).unwrap();
        query(&reader, args()).await
    });
    ready_rx.await.unwrap();
    // Both sources change while the query is queued; publication is one run.
    for c in 0..2 {
        let path = p.0.join(format!("g0c{c}.rs"));
        fs::write(
            &path,
            fs::read_to_string(&path).unwrap().replace("+= 1", "+= 21"),
        )
        .unwrap();
    }
    writer
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    let new = writer.published_generation().unwrap();
    assert!(new > old);
    drop(writer);
    let response = task.await.unwrap();
    assert!(
        response.contains(&format!("generation: {new}\n")),
        "{response}"
    );
    assert!(response.contains("detected: 1"));
    assert!(response.contains("coverage_complete: true"));
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_sources_cannot_escape_the_project() {
    let p = Project::new(1, 2);
    let outside = Project::new(1, 2);
    let server = p.server();
    fs::remove_file(p.0.join("g0c0.rs")).unwrap();
    std::os::unix::fs::symlink(outside.0.join("g0c0.rs"), p.0.join("g0c0.rs")).unwrap();
    let response = query(&server, args()).await;
    assert!(response.contains("detected: 0"));
    assert!(response.contains("coverage_complete: false"));
}
