//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `inv.Format()` on `Invoice inv` is `Invoice.Format`, never a
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
use mct_lang_csharp::CSharpParser;

const INVOICE: &str = "\
public class Invoice
{
    public string Format() => \"#\";
    public string Render() => this.Format();
}

public static class Money
{
    public static int Of(int cents) => cents;
}
";

const RECEIPT: &str = "\
public class Receipt
{
    public string Format() => \"receipt\";
    public void Close() {}
}

public class Bank
{
    public int Of(int cents) => cents;
}
";

const USES: &str = "\
using System.Collections.Generic;

public class Printer(Invoice current)
{
    private readonly Invoice _last = new Invoice();
    public Invoice? Next { get; set; }

    public string Print(Invoice inv, object other)
    {
        var made = new Invoice();
        Invoice typed = made;
        return inv.Format() + made.Format() + typed.Format() + new Invoice().Format() +
            this._last.Format() + current.Format() + Next.Format() + Money.Of(1) +
            other.Format();
    }

    public void Shut(List<Receipt> items)
    {
        items.ForEach(r => r.Close());
    }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-csharp-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("Invoice.cs", INVOICE),
        ("Receipt.cs", RECEIPT),
        ("Printer.cs", USES),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
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
    let hits = calls(&index, "Printer.cs", "Format");
    assert_eq!(hits.len(), 8, "{hits:?}");
    // Invoice inv, var = new Invoice(), a typed local, new Invoice().Format(),
    // a field through `this`, a primary constructor parameter and a property.
    for hit in &hits[..7] {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
        assert_eq!(targets(hit), ["Invoice.cs:3"], "{hit:?}");
    }
    // `object other` declares no type of the repository: unproven.
    assert_ne!(hits[7].resolution, Resolution::Resolved, "{:?}", hits[7]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_a_type_name_resolves_to_that_type() {
    let (index, root) = indexed();
    let hits = calls(&index, "Printer.cs", "Of");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(targets(&hits[0]), ["Invoice.cs:9"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_untyped_receiver_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    let hits = calls(&index, "Printer.cs", "Close");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Ambiguous, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_this_resolves_to_the_enclosing_class() {
    let (index, root) = indexed();
    let hits = calls(&index, "Invoice.cs", "Format");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{:?}", hits[0]);
    assert_eq!(targets(&hits[0]), ["Invoice.cs:3"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}
