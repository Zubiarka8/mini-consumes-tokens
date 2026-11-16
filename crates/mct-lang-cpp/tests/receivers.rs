//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `x.m()` and `p->m()` on a receiver declared with one class are
//! `A::m`, never a same-named method of another class. A receiver whose type
//! the declaration does not state (`auto`), a template parameter, or a name
//! declared with two types in the function proves nothing, so the call stays
//! ambiguous instead of resolving by a unique name.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_cpp::CppParser;

const SOURCE: &str = "\
struct A { int m() { return 1; } };
struct B { int m() { return 2; } };
int useRef(A& x) { return x.m(); }
int useConstRef(const A& x) { return x.m(); }
int usePtr(A* p) { return p->m(); }
int useLocal() { B b; return b.m(); }
int useNew() { B* q = new B(); return q->m(); }
int useAuto() { auto z = make(); return z.m(); }
int useTwice(bool f) { if (f) { A v; return v.m(); } B v; return v.m(); }
template <class T> int useT(T& t) { return t.m(); }
";

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-cpp-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("shapes.cpp"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    (index, root)
}

fn calls_from(index: &Index, from: &str) -> Vec<RelationHit> {
    index
        .find_references("m")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls" && h.from_symbol == from)
        .collect()
}

#[test]
fn a_call_on_a_declared_class_resolves_to_that_class() {
    let (index, root) = indexed();
    for (from, line) in [
        ("useRef", 1),
        ("useConstRef", 1),
        ("usePtr", 1),
        ("useLocal", 2),
        ("useNew", 2),
    ] {
        let hits = calls_from(&index, from);
        assert_eq!(hits.len(), 1, "{from}: {hits:?}");
        assert_eq!(
            hits[0].resolution,
            Resolution::Resolved,
            "{from}: {:?}",
            hits[0]
        );
        let target: Vec<u32> = hits[0].candidates.iter().map(|c| c.line).collect();
        assert_eq!(target, [line], "{from}: {:?}", hits[0]);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unproven_receiver_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    for (from, count) in [("useAuto", 1), ("useTwice", 2), ("useT", 1)] {
        let hits = calls_from(&index, from);
        assert_eq!(hits.len(), count, "{from}: {hits:?}");
        for hit in &hits {
            assert_ne!(hit.resolution, Resolution::Resolved, "{from}: {hit:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
