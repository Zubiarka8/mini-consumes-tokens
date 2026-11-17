//! A method called on a literal is called on the literal's standard type:
//! `"x".trim()` is `String.trim`, never a same-named method of a repository
//! class. The receiver is proven by its syntax alone, so the call carries
//! the type as qualifier and no repository method is a candidate; a call on
//! a local of unproven type stays ambiguous.
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

const SOURCE: &str = "\
package shop

class Util { fun trim(): String = \"\"; fun toString(x: Int): String = \"\" }
class Other { fun trim(): String = \"\"; fun toString(x: Int): String = \"\" }

fun literals() {
    val a = \"x\".trim()
    val b = 5.toString(2)
}

fun untyped() {
    val x = make()
    val c = x.trim()
}
";

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-kotlin-literals-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Shop.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    (index, root)
}

fn calls(index: &Index, to: &str) -> Vec<RelationHit> {
    let mut hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .collect();
    hits.sort_by_key(|h| (h.line, h.column));
    hits
}

#[test]
fn a_call_on_a_literal_matches_no_repository_method() {
    let (index, root) = indexed();
    let trim = calls(&index, "trim");
    let literal = trim.iter().find(|h| h.line == 7).unwrap();
    assert!(literal.candidates.is_empty(), "{literal:?}");
    assert_ne!(literal.resolution, Resolution::Ambiguous, "{literal:?}");
    let to_string = calls(&index, "toString");
    let number = to_string.iter().find(|h| h.line == 8).unwrap();
    assert!(number.candidates.is_empty(), "{number:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_a_local_of_unproven_type_stays_ambiguous() {
    let (index, root) = indexed();
    let trim = calls(&index, "trim");
    let unproven = trim.iter().find(|h| h.line == 13).unwrap();
    assert_eq!(unproven.resolution, Resolution::Ambiguous, "{unproven:?}");
    std::fs::remove_dir_all(root).unwrap();
}
