//! A running server must observe externally published generations, including caches.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Project(PathBuf);
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    server
        .call_tool_by_name(tool, args.as_object().cloned())
        .await
        .unwrap()
        .content[0]
        .as_text()
        .unwrap()
        .text
        .clone()
}
#[tokio::test]
async fn external_reindex_updates_findings_source_and_cached_rankings_without_restart() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let p = Project(std::env::temp_dir().join(format!(
        "mct-live-generation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir_all(p.0.join(".mct-index")).unwrap();
    let body = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
    fs::write(p.0.join("a.rs"), format!("pub fn alpha{body}")).unwrap();
    fs::write(p.0.join("b.rs"), format!("pub fn beta{body}")).unwrap();
    let db = p.0.join(".mct-index/index.sqlite3");
    let mut writer = Index::open(&p.0, &db, ExcludeSet::default()).unwrap();
    writer
        .reindex(&mct_languages::build_registry(), true)
        .unwrap();
    use mct_index::reviews::{dead_code_findings, duplicate_findings, Change, Reviews, State};
    let report = mct_index::find_duplicate_groups(
        &writer,
        &mct_languages::build_token_sources(),
        mct_index::DuplicateOptions::default(),
    )
    .unwrap();
    let hashes = writer.file_hashes().unwrap();
    let duplicates = duplicate_findings(&report.groups, report.options.normalization, &hashes);
    let scan = mct_index::scan_dead_code(&writer, None, None).unwrap();
    let dead = dead_code_findings(&scan.symbols, &scan.candidates, &hashes);
    let alpha = scan
        .candidates
        .iter()
        .position(|id| scan.symbols[*id].name == "alpha")
        .unwrap();
    let mut reviews = Reviews::load(&p.0);
    for (findings, position) in [(&duplicates, 0), (&dead, alpha)] {
        let revision = reviews.resolve(findings)[position].revision();
        reviews
            .save(
                findings,
                position,
                &Change {
                    state: State::Dismissed,
                    note: String::new(),
                    expected_revision: revision,
                },
                1700000000000,
            )
            .unwrap();
    }
    let reader = Index::open(&p.0, &db, ExcludeSet::default()).unwrap();
    let server = MctServer::new(reader, mct_languages::build_registry());
    let search = json!({"query":"alpha","cache":true,"snippet_lines":3});
    let before = call(&server, "search_symbols_like", search.clone()).await;
    assert!(before.contains("pub fn alpha"), "{before}");
    call(&server, "search_symbols_like", search.clone()).await;
    let duplicates = call(&server, "find_duplicate_code", json!({})).await;
    assert!(duplicates.contains("suppressed: 1"), "{duplicates}");
    fs::write(p.0.join("a.rs"), format!("// moved\npub fn alpha{body}")).unwrap();
    writer
        .reindex(&mct_languages::build_registry(), true)
        .unwrap();
    for tool in ["find_dead_code", "find_duplicate_code"] {
        let text = call(&server, tool, json!({})).await;
        assert!(text.contains("needs-reevaluation"), "{tool}: {text}");
        assert!(text.contains("suppressed: 0"), "{tool}: {text}");
    }
    fs::write(p.0.join("a.rs"), format!("// moved\npub fn zeta{body}")).unwrap();
    writer
        .reindex(&mct_languages::build_registry(), true)
        .unwrap();
    let after = call(&server, "search_symbols_like", search).await;
    assert!(!after.contains("pub fn alpha"), "{after}");
    assert!(!after.contains("unchanged"), "old cache digest: {after}");
    let symbols = call(&server, "find_symbol", json!({"name":"zeta"})).await;
    assert!(symbols.contains(",function,zeta,"), "{symbols}");
    let dead = call(&server, "find_dead_code", json!({})).await;
    assert!(dead.contains(",zeta,"), "{dead}");
    assert!(!dead.contains(",alpha,"), "{dead}");
    let duplicates = call(&server, "find_duplicate_code", json!({})).await;
    assert!(duplicates.contains("detected: 1"), "{duplicates}");
    assert!(duplicates.contains(",zeta,"), "{duplicates}");
    assert!(!duplicates.contains(",alpha,"), "{duplicates}");
    let source = call(&server, "build_context_pack", json!({"symbol":"zeta"})).await;
    assert!(source.contains("pub fn zeta"), "{source}");
}
