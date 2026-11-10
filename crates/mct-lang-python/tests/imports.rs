//! A Python call to an imported name carries the imported module as
//! evidence, through the real parse -> SQLite -> query pipeline:
//! `from app.util import parse; parse()` is `util`'s `parse`, never
//! another module's, and `import app.legacy as lg; lg.parse()` is
//! `legacy`'s. A third-party module (`requests`) declares nothing here, so a
//! same-named repo function never resolves it. A name the file binds again
//! (a `def` after the import) gets no import evidence.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. Each claim was checked by
//! running the same files with `python -I` (with a stub `requests` module).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_python::PythonParser;

const FILES: &[(&str, &str)] = &[
    ("app/__init__.py", ""),
    (
        "app/util.py",
        "def parse(text):\n    return text.split(\",\")\n",
    ),
    (
        "app/legacy.py",
        "def parse(text):\n    return text.split(\";\")\n",
    ),
    ("app/http.py", "def get(url):\n    return url\n"),
    (
        "app/report.py",
        "\
from app.util import parse
from requests import get
import app.legacy as lg
import app.util


def summarize(text):
    return parse(text) + lg.parse(text) + app.util.parse(text) + [get(text)]
",
    ),
    (
        "app/shadow.py",
        "\
from app.util import parse


def parse(text):
    return []


def run(text):
    return parse(text)
",
    ),
];

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-python-imports-{}-{unique}",
        std::process::id()
    ));
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
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
    let files: Vec<_> = calls(&index, "app/report.py", "parse", 8)
        .iter()
        .map(|h| resolved_file(&index, h))
        .collect();
    assert_eq!(
        files,
        [
            Some("app/util.py".to_string()),
            Some("app/legacy.py".to_string()),
            Some("app/util.py".to_string()),
        ]
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_third_party_import_never_resolves_to_a_repo_function() {
    let (index, root) = indexed();
    let hits = calls(&index, "app/report.py", "get", 8);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(resolved_file(&index, &hits[0]), None, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_name_bound_again_after_the_import_gets_no_import_evidence() {
    let (index, root) = indexed();
    let hits = calls(&index, "app/shadow.py", "parse", 9);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_ne!(
        resolved_file(&index, &hits[0]).as_deref(),
        Some("app/util.py"),
        "{:?}",
        hits[0]
    );
    std::fs::remove_dir_all(root).ok();
}
