//! Markdown/Obsidian notes through `build_context_pack` and `search_symbols`
//! with `detail`/`max_tokens`: sections keep their identity, ambiguous links
//! stay uncertain, stale notes disappear after a reindex, link cycles
//! terminate, and long/Unicode paths count toward the estimated budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

fn vault(files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-mcp-md-budget-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    for (path, contents) in files {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }
    dir
}

struct Vault {
    server: MctServer,
    registry: mct_core::LanguageRegistry,
    dir: PathBuf,
}

async fn open(dir: &Path) -> Vault {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    Vault {
        server: MctServer::new(index, mct_mcp_server::registry::build_registry()),
        registry,
        dir: dir.to_path_buf(),
    }
}

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let result = server
        .call_read_only_tool(tool, args.as_object().cloned())
        .await
        .unwrap_or_else(|err| panic!("`{tool}` failed: {}", err.message));
    result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn est(text: &str) -> usize {
    EstimatedCounter.count(text).value
}

const FILES: &[(&str, &str)] = &[
    ("bare.md", "Only text, no heading. See [[beta]].\n"),
    (
        "a/beta.md",
        "---\ntags: [x]\n---\n# Beta\n\nintro\n\n## Shared\n\nalpha body\n\n## Other\n\nother body\n",
    ),
    ("b/beta.md", "# Beta Two\n\n## Shared\n\nsecond body\n"),
    ("src.md", "Go to [[a/beta#Shared]] and ![[b/beta]].\n"),
];

#[tokio::test]
async fn a_section_keeps_identity_at_every_level_and_detail_stays_in_its_section() {
    let dir = vault(FILES);
    let v = open(&dir).await;
    let run = |detail: &'static str| {
        call(
            &v.server,
            "build_context_pack",
            json!({"symbol": "Shared", "path": "a/beta.md", "format": "text", "detail": detail}),
        )
    };
    let (overview, summary, detailed) = (
        run("overview").await,
        run("summary").await,
        run("detailed").await,
    );
    for text in [&overview, &summary, &detailed] {
        assert!(text.contains("a/beta.md:L"), "path:line provenance\n{text}");
        assert!(
            !text.contains("other body") && !text.contains("second body"),
            "{text}"
        );
    }
    assert!(!overview.contains("alpha body"), "{overview}");
    assert!(detailed.contains("alpha body"), "{detailed}");
    // Levels are ordered by content; the footer of a lower level is metadata
    // (`next:` costs) and may outweigh the body of a very small section.
    let body = |text: &str| {
        text.split("\nBudget:")
            .next()
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        est(&body(&overview)) <= est(&body(&summary))
            && est(&body(&summary)) <= est(&body(&detailed))
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_heading_less_note_gets_a_valid_minimal_response_then_fits_at_the_reported_budget() {
    let dir = vault(FILES);
    let v = open(&dir).await;
    let pack = |max: usize| {
        call(
            &v.server,
            "build_context_pack",
            json!({"symbol": "bare", "path": "bare.md", "format": "text", "max_tokens": max}),
        )
    };
    let tiny = pack(60).await;
    assert!(tiny.starts_with("Context pack for `bare`"), "{tiny}");
    assert!(tiny.contains("No row fits `max_tokens`"), "{tiny}");
    let needed: usize = tiny
        .rsplit('~')
        .next()
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap();
    let fitted = pack(needed).await;
    assert!(!fitted.contains("No row fits"), "{fitted}");
    assert!(fitted.contains("bare.md:L"), "{fitted}");
    assert!(
        est(&fitted) <= needed,
        "{} > {needed}\n{fitted}",
        est(&fitted)
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_ambiguous_wikilink_stays_uncertain_under_a_budget() {
    let dir = vault(FILES);
    let v = open(&dir).await;
    let text = call(
        &v.server,
        "build_context_pack",
        json!({"symbol": "beta", "path": "a/beta.md", "format": "text", "detail": "summary", "max_tokens": 400}),
    )
    .await;
    assert!(text.contains("references src"), "{text}");
    assert!(!text.contains("imports src"), "{text}");
    assert!(
        text.contains("bare") && text.contains("ambiguous"),
        "bare [[beta]] must remain an uncertain relation, not a backlink\n{text}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn modified_and_deleted_notes_are_reflected_after_reindex() {
    let dir = vault(FILES);
    let v = open(&dir).await;
    let search = |q: &'static str| {
        call(
            &v.server,
            "search_symbols_like",
            json!({"query": q, "format": "text", "detail": "overview", "max_tokens": 300}),
        )
    };
    assert!(search("Shared").await.contains("b/beta.md"));
    fs::write(
        v.dir.join("b/beta.md"),
        "# Beta Two\n\n## Renamed Part\n\nx\n",
    )
    .unwrap();
    fs::remove_file(v.dir.join("a/beta.md")).unwrap();
    // Reindex through a fresh index over the edited vault, as a watcher would.
    let mut index = Index::open_in_memory(&v.dir, ExcludeSet::default()).unwrap();
    index.reindex(&v.registry, false).unwrap();
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());
    let out = call(
        &server,
        "search_symbols_like",
        json!({"query": "Shared", "format": "text", "detail": "overview", "max_tokens": 300}),
    )
    .await;
    assert!(
        !out.contains("a/beta.md") && !out.contains("b/beta.md"),
        "{out}"
    );
    let renamed = call(
        &server,
        "search_symbols_like",
        json!({"query": "Renamed", "format": "text", "detail": "overview", "max_tokens": 300}),
    )
    .await;
    assert!(renamed.contains("b/beta.md"), "{renamed}");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn cyclic_note_links_terminate_at_depth() {
    let dir = vault(&[
        ("x.md", "# X\n\nsee [[y]]\n"),
        ("y.md", "# Y\n\nsee [[z]]\n"),
        ("z.md", "# Z\n\nsee [[x]]\n"),
    ]);
    let v = open(&dir).await;
    let text = call(
        &v.server,
        "build_context_pack",
        json!({"symbol": "x", "path": "x.md", "depth": 8, "format": "text", "detail": "summary", "max_tokens": 500}),
    )
    .await;
    assert!(text.contains("x.md:L"), "{text}");
    assert_eq!(text, call(
        &v.server,
        "build_context_pack",
        json!({"symbol": "x", "path": "x.md", "depth": 8, "format": "text", "detail": "summary", "max_tokens": 500}),
    )
    .await, "reproducible");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn long_unicode_paths_cost_budget_and_survive_truncation_intact() {
    let long = format!("notas/{}/決定記録.md", "ディレクトリ".repeat(12));
    let dir = vault(&[
        (long.as_str(), "# 決定\n\nデータベースにSQLiteを使う。\n"),
        ("plain.md", "# Decision\n\nuse SQLite\n"),
    ]);
    let v = open(&dir).await;
    let wide = call(
        &v.server,
        "search_symbols_like",
        json!({"query": "決定", "format": "text", "detail": "summary", "max_tokens": 2000}),
    )
    .await;
    assert!(wide.contains(&long), "{wide}");
    let narrow = call(
        &v.server,
        "search_symbols_like",
        json!({"query": "決定", "format": "text", "detail": "summary", "max_tokens": est(&wide) / 2}),
    )
    .await;
    assert!(
        est(&narrow) <= est(&wide) / 2 || narrow.contains("No row fits"),
        "{narrow}"
    );
    // Never a split character: the text is valid UTF-8 by construction, and
    // any path it still shows is the whole path.
    if narrow.contains("ディレクトリ") {
        assert!(narrow.contains(&long), "{narrow}");
    }
    fs::remove_dir_all(&dir).unwrap();
}
