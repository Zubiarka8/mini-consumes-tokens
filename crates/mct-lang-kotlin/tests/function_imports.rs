//! An import of a top-level function (`import a.render`) proves which
//! package a bare call reaches: `render(2)` is the `render` under `a/`,
//! never the same-named one under `b/`, and an aliased import
//! (`import b.render as draw`) makes `draw(1)` a call to `b`'s `render`. A
//! call in a file that imports neither, a name the file declares itself and
//! a wildcard import prove nothing and stay as they were.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_kotlin::KotlinParser;

const FILES: [(&str, &str); 6] = [
    ("a/Fmt.kt", "package a\n\nfun render(x: Int): String = \"a\"\n"),
    ("b/Fmt.kt", "package b\n\nfun render(x: Int): String = \"b\"\n"),
    (
        "app/Main.kt",
        "package app\n\nimport b.render as draw\nimport a.render\n\nfun main() {\n    draw(1)\n    render(2)\n}\n",
    ),
    (
        "app/Plain.kt",
        "package app\n\nfun plain() {\n    render(3)\n}\n",
    ),
    (
        "app/Own.kt",
        "package app\n\nimport a.render\n\nfun render(x: Int): String = \"own\"\n\nfun own() {\n    render(4)\n}\n",
    ),
    (
        "app/Star.kt",
        "package app\n\nimport a.*\n\nfun star() {\n    render(5)\n}\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-kotlin-imports-{}-{unique}",
        std::process::id()
    ));
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    (index, root)
}

fn call(index: &Index, file: &str, line: u32) -> RelationHit {
    index
        .find_references("render")
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == file && h.line == line)
        .unwrap()
}

#[test]
fn an_imported_function_resolves_to_its_package() {
    let (index, root) = indexed();
    let plain = call(&index, "app/Main.kt", 8);
    assert_eq!(plain.resolution, Resolution::Resolved, "{plain:?}");
    assert_eq!(plain.candidates[0].relative_path, "a/Fmt.kt", "{plain:?}");
    let aliased = call(&index, "app/Main.kt", 7);
    assert_eq!(aliased.resolution, Resolution::Resolved, "{aliased:?}");
    assert_eq!(
        aliased.candidates[0].relative_path, "b/Fmt.kt",
        "{aliased:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_with_no_function_import_proves_nothing() {
    let (index, root) = indexed();
    for (file, line) in [("app/Plain.kt", 4), ("app/Star.kt", 6)] {
        let hit = call(&index, file, line);
        assert_ne!(hit.resolution, Resolution::Resolved, "{hit:?}");
    }
    // The file declares its own `render`: the import does not decide it.
    let own = call(&index, "app/Own.kt", 8);
    assert!(
        own.resolution != Resolution::Resolved || own.candidates[0].relative_path == "app/Own.kt",
        "{own:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
