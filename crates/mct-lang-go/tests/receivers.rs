//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `inv.Format()` on `inv *Invoice` is `Invoice.Format`, never a
//! same-named method of another type. A receiver whose declared type has no
//! name is a member call, which stays ambiguous instead of resolving by a
//! unique name; one whose type the syntax does not state is left to name
//! resolution, which says so (`ResolutionEvidence::Name`).
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution, ResolutionEvidence};
use mct_lang_go::GoParser;

const INVOICE: &str = "\
package billing

type Invoice struct{ Total int }

func (i *Invoice) Format() string { return \"#\" }

func (i *Invoice) Render() string { return i.Format() }
";

const RECEIPT: &str = "\
package billing

type Receipt struct{}

func (r Receipt) Format() string { return \"receipt\" }

func (r Receipt) Close() {}
";

const USES: &str = "\
package billing

var fallback Invoice

type Printer struct {
	last *Invoice
}

func (p *Printer) Print(inv *Invoice, other Formatter) string {
	made := &Invoice{}
	var typed Invoice
	fresh := new(Invoice)
	return inv.Format() + made.Format() + typed.Format() + fresh.Format() +
		p.last.Format() + fallback.Format() + other.Format() + lookup().Format()
}

func Shut(items []Receipt) {
	for _, r := range items {
		r.Close()
	}
}

func lookup() *Receipt { return nil }

// Service embeds Receipt: Close is promoted, Format is its own.
type Service struct {
	*Receipt
}

func (s *Service) Format() string { return \"service\" }

func (s *Service) Stop() string {
	s.Close()
	return s.Format()
}

type Formatter interface{ Show() string }

func Drop(closer interface{ Close() }) {
	closer.Close()
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-go-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("invoice.go", INVOICE),
        ("receipt.go", RECEIPT),
        ("printer.go", USES),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
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
    let hits = calls(&index, "printer.go", "Format");
    assert_eq!(hits.len(), 9, "{hits:?}");
    // inv *Invoice, &Invoice{}, var typed Invoice, new(Invoice), a struct
    // field and a package-level var.
    for hit in &hits[..6] {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
        assert_eq!(targets(hit), ["invoice.go:5"], "{hit:?}");
    }
    // An interface without `Format` and a call result: unproven.
    for hit in &hits[6..8] {
        assert_ne!(hit.resolution, Resolution::Resolved, "{hit:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_receiver_of_unstated_type_is_left_to_name_resolution() {
    let (index, root) = indexed();
    let hits = calls(&index, "printer.go", "Close");
    assert_eq!(hits.len(), 3, "{hits:?}");
    // `r` is a `range` variable: no evidence either way, so the unique name
    // resolves it and the hit says that is all it rests on.
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{:?}", hits[0]);
    assert_eq!(hits[0].evidence, ResolutionEvidence::Name, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_receiver_of_an_unnamed_type_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    let hits = calls(&index, "printer.go", "Close");
    assert_eq!(hits.len(), 3, "{hits:?}");
    // `closer interface{ Close() }`: no `Receipt`, whatever the name says.
    assert_eq!(hits[2].resolution, Resolution::Ambiguous, "{:?}", hits[2]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_promoted_method_through_an_embedded_field_is_a_member_call() {
    let (index, root) = indexed();
    let close = calls(&index, "printer.go", "Close");
    assert_eq!(close.len(), 3, "{close:?}");
    // `s.Close()`: Service declares no Close here, but embeds *Receipt, so
    // the method may be promoted: ambiguous with Receipt.Close a candidate,
    // never unresolved through the `Service` qualifier.
    assert_eq!(close[1].resolution, Resolution::Ambiguous, "{:?}", close[1]);
    assert_eq!(targets(&close[1]), ["receipt.go:7"], "{:?}", close[1]);
    // `s.Format()`: declared on Service in this file.
    let format = calls(&index, "printer.go", "Format");
    let own = format.last().unwrap();
    assert_eq!(own.resolution, Resolution::Resolved, "{own:?}");
    assert_eq!(targets(own), ["printer.go:30"], "{own:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_the_method_receiver_resolves_to_its_type() {
    let (index, root) = indexed();
    let hits = calls(&index, "invoice.go", "Format");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{:?}", hits[0]);
    assert_eq!(targets(&hits[0]), ["invoice.go:5"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}
