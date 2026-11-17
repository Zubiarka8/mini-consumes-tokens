//! A single-member static import (`import static a.Fmt.render;`) proves the
//! type and package a bare call reaches: `render(1)` is `a.Fmt.render`,
//! never `b.Fmt.render`. A JDK static import (`java.lang.Math.max`) is the
//! JDK's. A wildcard static import, a name the file declares itself (a
//! declared method hides the import) and a name two static imports bind
//! prove nothing.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_java::JavaParser;

const FILES: [(&str, &str); 10] = [
    (
        "a/Fmt.java",
        "package a;\npublic class Fmt {\n    public static String render(int x) { return \"a\"; }\n    public static int max(int x, int y) { return x; }\n}\n",
    ),
    (
        "b/Fmt.java",
        "package b;\npublic class Fmt {\n    public static String render(int x) { return \"b\"; }\n    public static int max(int x, int y) { return y; }\n}\n",
    ),
    (
        "app/Main.java",
        "package app;\nimport static a.Fmt.render;\nimport static java.lang.Math.max;\npublic class Main {\n    void run() {\n        render(1);\n        max(1, 2);\n    }\n}\n",
    ),
    (
        "app/Plain.java",
        "package app;\npublic class Plain {\n    void run() {\n        render(2);\n    }\n}\n",
    ),
    (
        "app/Star.java",
        "package app;\nimport static a.Fmt.*;\npublic class Star {\n    void run() {\n        render(3);\n    }\n}\n",
    ),
    (
        "app/Own.java",
        "package app;\nimport static a.Fmt.render;\npublic class Own {\n    String render(int x) { return \"own\"; }\n    void run() {\n        render(4);\n    }\n}\n",
    ),
    (
        "app/Twice.java",
        "package app;\nimport static a.Fmt.render;\nimport static b.Fmt.render;\npublic class Twice {\n    void run() {\n        render(5);\n    }\n}\n",
    ),
    (
        "app/Child.java",
        "package app;\nimport static a.Fmt.render;\npublic class Child extends Base {\n    void run() {\n        render(7);\n    }\n}\n",
    ),
    (
        "app/Anon.java",
        "package app;\nimport static a.Fmt.render;\npublic class Anon {\n    Object o = new Base() {\n        void run() {\n            render(8);\n        }\n    };\n}\n",
    ),
    (
        "app/Qualified.java",
        "package app;\nimport static a.Fmt.render;\npublic class Qualified {\n    void run(Object o) {\n        o.render(6);\n    }\n}\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-java-static-{}-{unique}",
        std::process::id()
    ));
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    (index, root)
}

fn call(index: &Index, name: &str, file: &str, line: u32) -> RelationHit {
    index
        .find_references(name)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == file && h.line == line)
        .unwrap()
}

#[test]
fn a_static_import_resolves_a_bare_call_to_its_type() {
    let (index, root) = indexed();
    let hit = call(&index, "render", "app/Main.java", 6);
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates[0].relative_path, "a/Fmt.java", "{hit:?}");
    let jdk = call(&index, "max", "app/Main.java", 7);
    assert_eq!(jdk.resolution, Resolution::External, "{jdk:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_with_no_proving_static_import_stays_unproven() {
    let (index, root) = indexed();
    for (file, line) in [
        ("app/Plain.java", 4),
        ("app/Star.java", 5),
        ("app/Twice.java", 6),
        ("app/Qualified.java", 5),
        // A superclass may declare `render`, which would hide the import.
        ("app/Child.java", 5),
        ("app/Anon.java", 6),
    ] {
        let hit = call(&index, "render", file, line);
        assert_ne!(hit.resolution, Resolution::Resolved, "{file}: {hit:?}");
    }
    // The class declares `render`: it hides the import.
    let own = call(&index, "render", "app/Own.java", 6);
    assert!(
        own.resolution != Resolution::Resolved || own.candidates[0].relative_path == "app/Own.java",
        "{own:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
