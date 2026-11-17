//! `use function B\render as draw;` makes `draw(2)` a call to `B\render`:
//! the relation carries the function's own name and its namespace, so it
//! reaches `B`'s `render` and never `A`'s. The same import without an alias
//! (`use function A\render;`) keeps resolving as before, and a bare name
//! no import binds stays unproven.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_php::PhpParser;

const FILES: [(&str, &str); 4] = [
    (
        "a.php",
        "<?php\nnamespace A;\n\nfunction render(int $x): string { return 'a'; }\n",
    ),
    (
        "b.php",
        "<?php\nnamespace B;\n\nfunction render(int $x): string { return 'b'; }\n",
    ),
    (
        "main.php",
        "<?php\nnamespace App;\n\nuse function A\\render;\nuse function B\\render as draw;\n\nfunction run() {\n    render(1);\n    draw(2);\n}\n",
    ),
    (
        "plain.php",
        "<?php\nnamespace App;\n\nfunction other() {\n    draw(3);\n}\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-php-aliases-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, contents) in FILES {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    (index, root)
}

fn call(index: &Index, name: &str, file: &str, line: u32) -> Option<RelationHit> {
    index
        .find_references(name)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == file && h.line == line)
}

#[test]
fn an_aliased_function_import_reaches_its_namespace() {
    let (index, root) = indexed();
    let plain = call(&index, "render", "main.php", 8).unwrap();
    assert_eq!(plain.resolution, Resolution::Resolved, "{plain:?}");
    assert_eq!(plain.candidates[0].relative_path, "a.php", "{plain:?}");
    let aliased = call(&index, "render", "main.php", 9).unwrap();
    assert_eq!(aliased.resolution, Resolution::Resolved, "{aliased:?}");
    assert_eq!(aliased.candidates[0].relative_path, "b.php", "{aliased:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unimported_alias_name_stays_unproven() {
    let (index, root) = indexed();
    let unbound = call(&index, "draw", "plain.php", 5).unwrap();
    assert_ne!(unbound.resolution, Resolution::Resolved, "{unbound:?}");
    std::fs::remove_dir_all(root).unwrap();
}
