//! A JS/TS call to an imported name carries the imported module as
//! evidence, through the real parse -> SQLite -> query pipeline:
//! `import { format } from "./fmt"; format()` is `fmt`'s `format`, never
//! another module's, and `import * as legacy from "./legacy/index";
//! legacy.format()` is `legacy`'s. A package (`left-pad`) declares nothing
//! here, so a same-named repo function never resolves it. A name the file
//! binds again gets no import evidence.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. Each claim was checked by
//! running the same files with Node 24 type stripping (with a stub
//! `left-pad` package).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_js_ts::JsTsParser;

const FILES: &[(&str, &str)] = &[
    (
        "src/fmt.ts",
        "export function format(v: number): string {\n  return v.toFixed(2);\n}\n",
    ),
    (
        "src/legacy/index.ts",
        "export function format(v: number): string {\n  return String(v);\n}\n",
    ),
    (
        "src/server-settings.ts",
        "export function persist(v: number): number {\n  return v;\n}\n",
    ),
    (
        "src/store.ts",
        "export function persist(v: number): number {\n  return -v;\n}\n",
    ),
    (
        "src/pad.ts",
        "export default function leftPad(s: string): string {\n  return s;\n}\n",
    ),
    (
        "src/app.ts",
        "\
import { format } from \"./fmt.ts\";
import * as legacy from \"./legacy/index.ts\";
import leftPad from \"left-pad\";
import { persist } from \"./server-settings.ts\";

export function show(v: number): string {
  return format(v) + legacy.format(v) + leftPad(\"x\") + persist(v);
}

console.log(show(1));
",
    ),
    (
        "src/shadow.ts",
        "\
import { format } from \"./fmt.ts\";

export function run(v: number): string {
  const format = (n: number) => `${n}`;
  return format(v);
}
",
    ),
];

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-imports-{}-{unique}",
        std::process::id()
    ));
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    (index, root)
}

/// The `calls` relations named `to` in `file` on `line`, by column.
fn calls(index: &Index, file: &str, to: &str, line: u32) -> Vec<RelationHit> {
    let mut hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.kind == "calls" && h.line == line)
        .collect();
    hits.sort_by_key(|h| h.column);
    hits
}

fn resolved_file(index: &Index, hit: &RelationHit) -> Option<String> {
    if hit.resolution != Resolution::Resolved {
        return None;
    }
    index
        .symbols_by_ids(&[hit.target_id?])
        .unwrap()
        .into_iter()
        .next()
        .map(|s| s.relative_path)
}

#[test]
fn an_imported_name_resolves_to_its_module() {
    let (index, root) = indexed();
    let files: Vec<_> = calls(&index, "src/app.ts", "format", 7)
        .iter()
        .map(|h| resolved_file(&index, h))
        .collect();
    assert_eq!(
        files,
        [
            Some("src/fmt.ts".to_string()),
            Some("src/legacy/index.ts".to_string()),
        ]
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_hyphenated_module_name_still_names_its_file() {
    let (index, root) = indexed();
    let hits = calls(&index, "src/app.ts", "persist", 7);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(
        resolved_file(&index, &hits[0]).as_deref(),
        Some("src/server-settings.ts"),
        "{:?}",
        hits[0]
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_package_import_never_resolves_to_a_repo_function() {
    let (index, root) = indexed();
    let hits = calls(&index, "src/app.ts", "leftPad", 7);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(resolved_file(&index, &hits[0]), None, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_name_bound_again_gets_no_import_evidence() {
    let (index, root) = indexed();
    let hits = calls(&index, "src/shadow.ts", "format", 5);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_ne!(
        resolved_file(&index, &hits[0]).as_deref(),
        Some("src/fmt.ts"),
        "{:?}",
        hits[0]
    );
    std::fs::remove_dir_all(root).ok();
}
