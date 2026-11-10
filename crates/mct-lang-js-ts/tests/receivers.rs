//! A method call's receiver through the real parse -> SQLite -> query
//! pipeline: `inv.format()` on `inv: Invoice` is `Invoice.format`, never a
//! same-named method of another class or a same-named free function, and a
//! receiver whose type the file does not declare is a member call, which
//! stays ambiguous instead of resolving by a unique name.
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
use mct_lang_js_ts::JsTsParser;

const INVOICE: &str = "\
export class Invoice {
  total = 0;
  format(): string {
    return `#${this.total}`;
  }
  render(): string {
    return this.format();
  }
}
";

const RECEIPT: &str = "\
export class Receipt {
  format(): string {
    return \"receipt\";
  }
  close(): void {}
}

export function format(value: number): string {
  return value.toFixed(2);
}
";

const USES: &str = "\
import { Invoice } from \"./invoice\";

export class Printer {
  private last: Invoice;
  constructor(private readonly current: Invoice) {
    this.last = current;
  }
  print(inv: Invoice, other): string {
    const made = new Invoice();
    const typed: Invoice = made;
    return inv.format() + made.format() + typed.format() + new Invoice().format()
      + this.current.format() + this.last.format() + other.format();
  }
  shut(handle): void {
    handle.close();
  }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-receivers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("invoice.ts", INVOICE),
        ("receipt.ts", RECEIPT),
        ("uses.ts", USES),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
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
    let hits = calls(&index, "uses.ts", "format");
    assert_eq!(hits.len(), 7, "{hits:?}");
    // inv: Invoice, new Invoice(), `: Invoice`, new Invoice().format(),
    // a parameter property and a declared field.
    for hit in &hits[..6] {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
        assert_eq!(targets(hit), ["invoice.ts:3"], "{hit:?}");
    }
    // `other` has no declared type: a member call, never resolved by name.
    assert_eq!(hits[6].resolution, Resolution::Ambiguous, "{:?}", hits[6]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_untyped_receiver_is_not_resolved_by_a_unique_name() {
    let (index, root) = indexed();
    let hits = calls(&index, "uses.ts", "close");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Ambiguous, "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_this_keeps_its_same_file_resolution() {
    let (index, root) = indexed();
    let hits = calls(&index, "invoice.ts", "format");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(targets(&hits[0]), ["invoice.ts:3"], "{:?}", hits[0]);
    std::fs::remove_dir_all(root).unwrap();
}
