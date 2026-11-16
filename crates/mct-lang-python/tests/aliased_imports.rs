//! `from b import render as draw` makes `draw(2)` a call to `b`'s `render`:
//! the relation carries the function's own name and the module it came
//! from, so it reaches `b.py` and never `a.py`. The same import without an
//! alias keeps resolving, a name no import binds stays unproven, and a name
//! two imports bind to different modules proves nothing.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_python::PythonParser;

const FILES: [(&str, &str); 5] = [
    ("a.py", "def render(x):\n    return 'a'\n"),
    ("b.py", "def render(x):\n    return 'b'\n"),
    (
        "main.py",
        "from a import render\nfrom b import render as draw\n\n\ndef run():\n    render(1)\n    draw(2)\n",
    ),
    ("plain.py", "def other():\n    draw(3)\n"),
    (
        "twice.py",
        "from a import render as pick\nfrom b import render as pick\n\n\ndef run():\n    pick(4)\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-python-aliases-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, contents) in FILES {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
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
fn an_aliased_from_import_reaches_its_module() {
    let (index, root) = indexed();
    let plain = call(&index, "render", "main.py", 6).unwrap();
    assert_eq!(plain.resolution, Resolution::Resolved, "{plain:?}");
    assert_eq!(plain.candidates[0].relative_path, "a.py", "{plain:?}");
    let aliased = call(&index, "render", "main.py", 7).unwrap();
    assert_eq!(aliased.resolution, Resolution::Resolved, "{aliased:?}");
    assert_eq!(aliased.candidates[0].relative_path, "b.py", "{aliased:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unbound_or_doubly_bound_alias_proves_nothing() {
    let (index, root) = indexed();
    // No import binds `draw` in plain.py: the call keeps its own name.
    let unbound = call(&index, "draw", "plain.py", 2).unwrap();
    assert_ne!(unbound.resolution, Resolution::Resolved, "{unbound:?}");
    // `pick` is bound to two modules: either call site could be either.
    let twice = index
        .find_references("render")
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == "twice.py");
    if let Some(hit) = twice {
        assert_ne!(hit.resolution, Resolution::Resolved, "{hit:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
