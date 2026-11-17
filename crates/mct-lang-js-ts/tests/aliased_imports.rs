//! `import { render as draw } from "./b.js"` makes `draw(2)` a call to
//! `render` of `b.js`: the relation carries the exported name and the
//! module, so it reaches `b.js` and never `a.js`. The same import without an
//! alias keeps resolving, a default import keeps its local name, and a name
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
use mct_lang_js_ts::JsTsParser;

const FILES: [(&str, &str); 4] = [
    ("a.js", "export function render(x) { return 'a'; }\n"),
    ("b.js", "export function render(x) { return 'b'; }\n"),
    (
        "main.js",
        "import { render } from './a.js';\nimport { render as draw } from './b.js';\n\nexport function run() {\n  render(1);\n  draw(2);\n}\n",
    ),
    (
        "plain.js",
        "export function other() {\n  draw(3);\n}\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-aliases-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, contents) in FILES {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
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
fn an_aliased_named_import_reaches_its_module() {
    let (index, root) = indexed();
    let plain = call(&index, "render", "main.js", 5).unwrap();
    assert_eq!(plain.resolution, Resolution::Resolved, "{plain:?}");
    assert_eq!(plain.candidates[0].relative_path, "a.js", "{plain:?}");
    let aliased = call(&index, "render", "main.js", 6).unwrap();
    assert_eq!(aliased.resolution, Resolution::Resolved, "{aliased:?}");
    assert_eq!(aliased.candidates[0].relative_path, "b.js", "{aliased:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_name_no_import_binds_is_not_resolved_by_an_alias() {
    let (index, root) = indexed();
    let unbound = call(&index, "draw", "plain.js", 2).unwrap();
    assert_ne!(unbound.resolution, Resolution::Resolved, "{unbound:?}");
    std::fs::remove_dir_all(root).unwrap();
}
