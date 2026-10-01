//! Markdown notes through the existing MCP tools (issue #98): notes show up in
//! the project overview, a heading's context pack is its section, a note's is
//! the note, and `find_references` keeps note links apart by resolved target.
//! No Markdown-specific tool: everything below is `get_project_overview`,
//! `build_context_pack` and `find_references`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

fn vault(files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-mcp-md-notes-{}-{}",
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

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let Value::Object(map) = args else {
        panic!("args must be an object")
    };
    let result = server
        .call_read_only_tool(tool, Some(map))
        .await
        .unwrap_or_else(|err| panic!("`{tool}` failed: {}", err.message));
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn server_for(dir: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
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
async fn the_overview_lists_every_note_including_heading_less_ones() {
    let dir = vault(FILES);
    let server = server_for(&dir).await;
    let out = call(&server, "get_project_overview", json!({})).await;
    for note in ["bare.md", "a/beta.md", "b/beta.md", "src.md"] {
        assert!(
            out.contains(&format!("\n{note}:\n")),
            "{note} missing:\n{out}"
        );
    }
    assert!(out.contains("[module] bare"), "{out}");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_heading_pack_is_its_section_and_stops_at_the_next_sibling() {
    let dir = vault(FILES);
    let server = server_for(&dir).await;
    let out = call(
        &server,
        "build_context_pack",
        json!({"symbol": "Shared", "path": "a/beta.md"}),
    )
    .await;
    assert!(out.contains("alpha body"), "{out}");
    assert!(
        !out.contains("other body"),
        "section must end at `## Other`:\n{out}"
    );
    assert!(!out.contains("second body"), "{out}");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_note_pack_is_the_note_with_its_links() {
    let dir = vault(FILES);
    let server = server_for(&dir).await;
    let out = call(
        &server,
        "build_context_pack",
        json!({"symbol": "bare", "path": "bare.md"}),
    )
    .await;
    assert!(out.contains("Only text, no heading"), "{out}");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_note_pack_lists_only_the_notes_that_link_to_that_exact_path() {
    let dir = vault(FILES);
    let server = server_for(&dir).await;
    let pack = |path: &str| {
        call(
            &server,
            "build_context_pack",
            json!({"symbol": "beta", "path": path, "depth": 1}),
        )
    };
    let first = pack("a/beta.md").await;
    let second = pack("b/beta.md").await;
    // `src.md` links `a/beta#Shared` (a reference to a/beta.md) and embeds
    // `b/beta`; `bare.md`'s bare `[[beta]]` is ambiguous between the two, so
    // it is reported as uncertain, never as a confirmed backlink.
    assert!(first.contains("references src"), "{first}");
    assert!(!first.contains("imports src"), "{first}");
    assert!(second.contains("imports src"), "{second}");
    assert!(!second.contains("references src"), "{second}");
    for pack in [&first, &second] {
        assert!(pack.contains("Uncertain relations"), "{pack}");
        assert!(pack.contains("bare"), "{pack}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn references_flag_an_ambiguous_note_link_and_keep_raw_spelling() {
    let dir = vault(FILES);
    let server = server_for(&dir).await;
    let out = call(&server, "find_references", json!({"symbol": "beta"})).await;
    assert!(
        out.contains("bare.md:1:1 [markdown] bare --references--> beta (ambiguous: 2)"),
        "{out}"
    );
    assert!(
        out.contains("src --imports--> beta"),
        "embed keeps its own kind:\n{out}"
    );
    let _ = fs::remove_dir_all(&dir);
}
