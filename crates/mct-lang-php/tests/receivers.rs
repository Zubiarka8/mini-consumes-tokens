//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `$x->m()` on a parameter typed `A`, on a variable assigned once
//! `new A()` and on `(new A())->m()` is `A::m`, never a same-named method of
//! another class. A receiver whose type is not proven (an untyped
//! parameter, a variable assigned twice, a union type) is a member call,
//! which stays ambiguous instead of resolving by a unique name.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_php::PhpParser;

const CLASSES: &str = "<?php
namespace App;
class Invoice { public function format() { return 1; } }
class Receipt { public function format() { return 2; } }
";

const USES: &str = "<?php
namespace App;
class Printer {
    public function typed(Invoice $inv, ?Invoice $maybe) { return $inv->format() + $maybe->format(); }
    public function made() { $i = new Invoice(); return $i->format(); }
    public function inline() { return (new Invoice())->format(); }
    public function untyped($x) { return $x->format(); }
    public function twice() { $i = new Invoice(); $i = new Receipt(); return $i->format(); }
    public function union(Invoice|Receipt $u) { return $u->format(); }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-php-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Classes.php"), CLASSES).unwrap();
    std::fs::write(root.join("Printer.php"), USES).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    (index, root)
}

fn calls_from(index: &Index, from: &str) -> Vec<RelationHit> {
    index
        .find_references("format")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "Printer.php" && h.kind == "calls" && h.from_symbol == from)
        .collect()
}

fn targets(hit: &RelationHit) -> Vec<String> {
    hit.candidates
        .iter()
        .map(|c| format!("{}:{}", c.relative_path, c.line))
        .collect()
}

#[test]
fn a_call_on_a_proven_receiver_type_resolves_to_that_type() {
    let (index, root) = indexed();
    for from in ["typed", "made", "inline"] {
        let hits = calls_from(&index, from);
        assert!(!hits.is_empty(), "{from}: no call found");
        for hit in &hits {
            assert_eq!(hit.resolution, Resolution::Resolved, "{from}: {hit:?}");
            assert_eq!(targets(hit), ["Classes.php:3"], "{from}: {hit:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unproven_receiver_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    for from in ["untyped", "twice", "union"] {
        let hits = calls_from(&index, from);
        assert_eq!(hits.len(), 1, "{from}: {hits:?}");
        assert_ne!(
            hits[0].resolution,
            Resolution::Resolved,
            "{from}: {:?}",
            hits[0]
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
