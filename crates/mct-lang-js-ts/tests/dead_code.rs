//! Dead code through the real parse -> SQLite -> query pipeline, by how each
//! reference resolved (issue #322): a symbol reached only by a re-export or
//! only by an ambiguous member call is never reported dead.
//!
//! The project is written to a temporary directory, so its names never
//! become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{find_dead_code_candidates, Index, Resolution};
use mct_lang_js_ts::JsTsParser;

fn indexed(files: &[(&str, &str)]) -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-dead-code-{}-{unique}",
        std::process::id()
    ));
    for (name, contents) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    (index, root)
}

fn dead(index: &Index) -> Vec<String> {
    let mut out: Vec<String> = find_dead_code_candidates(index, None, None)
        .unwrap()
        .into_iter()
        .map(|e| format!("{}:{}", e.relative_path, e.name))
        .collect();
    out.sort();
    out
}

#[test]
fn a_symbol_reached_only_by_a_re_export_is_not_dead() {
    let (index, root) = indexed(&[
        ("lib/util.ts", "export function helper() { return 1; }\n"),
        ("index.ts", "export { helper } from \"./lib/util\";\n"),
    ]);
    // Precondition: the re-export resolves to that one symbol, so it is the
    // per-symbol count that keeps `helper` alive, not its name.
    let hits = index.find_references("helper").unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{hits:?}");
    let dead = dead(&index);
    assert!(
        !dead.contains(&"lib/util.ts:helper".to_string()),
        "{dead:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn methods_reached_only_by_an_ambiguous_member_call_are_not_dead() {
    let (index, root) = indexed(&[
        (
            "jobs.ts",
            "export class Job { run() { return 1; } }\nexport class Task { run() { return 2; } }\n",
        ),
        (
            "main.ts",
            "export function start(worker: any) { return worker.run(); }\n",
        ),
    ]);
    // Precondition: `worker.run()` names no type, so both methods are
    // candidates and neither is proven.
    let hits = index.find_references("run").unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Ambiguous, "{hits:?}");
    let dead = dead(&index);
    assert!(!dead.iter().any(|d| d.ends_with(":run")), "{dead:?}");
    std::fs::remove_dir_all(root).unwrap();
}
