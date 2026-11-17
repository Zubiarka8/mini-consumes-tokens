//! A `require`/`include` of a literal path is an import of that file
//! whatever the literal's quotes or parentheses, through the real parse ->
//! SQLite -> query pipeline: `require_once 'lib.php'`, `require_once
//! "lib.php"` and `require_once("lib.php")` each give the dependency graph
//! an edge to `lib.php`, and the path as written is not listed as an
//! external import. An interpolated string (`"$dir/lib.php"`) has no static
//! path and records nothing. The PHP manual (`include`) documents it as a
//! language construct whose operand is any expression, so the parentheses
//! only group it.
//!
//! The project is written to a temporary directory, so its names never
//! become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_php::PhpParser;

const FILES: &[(&str, &str)] = &[
    ("lib.php", "<?php\nfunction helper() { return 1; }\n"),
    ("single.php", "<?php\nrequire_once 'lib.php';\nhelper();\n"),
    (
        "double.php",
        "<?php\nrequire_once \"lib.php\";\nhelper();\n",
    ),
    (
        "parens.php",
        "<?php\nrequire_once(\"lib.php\");\nhelper();\n",
    ),
    ("include.php", "<?php\ninclude ('./lib.php');\nhelper();\n"),
    (
        "dynamic.php",
        "<?php\n$dir = '.';\nrequire \"$dir/lib.php\";\n",
    ),
];

#[test]
fn every_literal_include_is_an_edge_and_no_external_import() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-php-includes-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, source) in FILES {
        std::fs::write(root.join(path), source).unwrap();
    }
    let index = mct_corpus::open_indexed(&root, Arc::new(PhpParser), FILES.len(), "*.php");
    for file in ["single.php", "double.php", "parens.php", "include.php"] {
        let graph = index.dependency_graph(file, 1, 50).unwrap();
        assert_eq!(graph.nodes, [file, "lib.php"], "{file}");
        assert!(graph.external.is_empty(), "{file}: {:?}", graph.external);
    }
    let graph = index.dependency_graph("dynamic.php", 1, 50).unwrap();
    assert_eq!(graph.nodes, ["dynamic.php"]);
    assert!(graph.external.is_empty(), "{:?}", graph.external);
    std::fs::remove_dir_all(root).ok();
}
