//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `inv.format()` on `inv: Invoice` is `Invoice.format`, never a
//! same-named method of another class, and a receiver whose type the file
//! does not declare is a member call, which stays ambiguous instead of
//! resolving by a unique name.
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
use mct_lang_kotlin::KotlinParser;

const INVOICE: &str = "\
class Invoice(val total: Int) {
    fun format(): String = \"#$total\"
    fun render(): String = this.format()
}

object Money {
    fun of(cents: Int): Int = cents
}
";

const RECEIPT: &str = "\
class Receipt {
    fun format(): String = \"receipt\"
    fun close() {}
}

class Bank {
    fun of(cents: Int): Int = cents
}
";

const USES: &str = "\
class Printer(private val current: Invoice) {
    private val last: Invoice = current

    fun print(inv: Invoice, other: Any): String {
        val made = Invoice(1)
        val typed: Invoice = made
        return inv.format() + made.format() + typed.format() + Invoice(2).format() +
            this.current.format() + last.format() + Money.of(1) + other.format()
    }

    fun shut(items: List<Receipt>) {
        items.forEach { it.close() }
    }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-kotlin-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("Invoice.kt", INVOICE),
        ("Receipt.kt", RECEIPT),
        ("Printer.kt", USES),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    (index, root)
}

fn calls(index: &Index, file: &str, to: &str) -> Vec<RelationHit> {
    let mut hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.kind == "calls")
        .collect();
    hits.sort_by_key(|h| (h.line, h.column));
    hits
}

fn targets(hit: &RelationHit) -> Vec<String> {
    hit.candidates
        .iter()
        .map(|c| format!("{}:{}", c.relative_path, c.line))
        .collect()
}

#[test]
fn a_call_on_a_declared_type_resolves_to_that_type() {
    let (index, root) = indexed();
    let hits = calls(&index, "Printer.kt", "format");
    assert_eq!(hits.len(), 7, "{hits:?}");
    // inv: Invoice, = Invoice(1), `: Invoice`, Invoice(2).format(), a
    // constructor property through `this` and a class property.
    for hit in &hits[..6] {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
        assert_eq!(targets(hit), ["Invoice.kt:2"], "{hit:?}");
    }
    // `other: Any` declares no type of the repository: unproven.
    assert_ne!(hits[6].resolution, Resolution::Resolved, "{:?}", hits[6]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_an_object_name_resolves_to_that_object() {
    let (index, root) = indexed();
    let hits = calls(&index, "Printer.kt", "of");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(targets(&hits[0]), ["Invoice.kt:7"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_untyped_receiver_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    let hits = calls(&index, "Printer.kt", "close");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Ambiguous, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_this_resolves_to_the_enclosing_class() {
    let (index, root) = indexed();
    let hits = calls(&index, "Invoice.kt", "format");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{:?}", hits[0]);
    assert_eq!(targets(&hits[0]), ["Invoice.kt:2"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}
