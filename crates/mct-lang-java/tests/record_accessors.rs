//! A record component declares an implicit accessor method
//! (`record Paid(Money amount)` → `amount()`), so a call `p.amount()` has
//! that accessor among its candidates, through the real
//! parse -> SQLite -> query pipeline. Without it, the only other `amount()`
//! in the project would win by name alone.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. The claims were checked by
//! compiling and running the same files with `javac`/`java`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_java::JavaParser;

const MONEY: &str = "\
package shop;

public class Money {
    private final int cents = 5;

    public int amount() {
        return cents;
    }
}
";

const PAID: &str = "\
package shop;

public record Paid(String id, Money amount) {
    public String id() {
        return \"paid-\" + id;
    }
}
";

const REPORT: &str = "\
package shop;

public class Report {
    public int print(Paid p) {
        return p.amount().amount() + p.id().length();
    }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-java-record-{}-{unique}",
        std::process::id()
    ));
    let dir = root.join("src/shop");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, contents) in [
        ("Money.java", MONEY),
        ("Paid.java", PAID),
        ("Report.java", REPORT),
    ] {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    (index, root)
}

/// The `calls` relations named `to` on `line` of `Report.java`, by column.
fn calls(index: &Index, to: &str, line: u32) -> Vec<RelationHit> {
    let mut hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| {
            h.relative_path == "src/shop/Report.java" && h.kind == "calls" && h.line == line
        })
        .collect();
    hits.sort_by_key(|h| h.column);
    hits
}

#[test]
fn record_components_and_explicit_accessors_are_methods_once() {
    let (index, root) = indexed();
    let methods = |name: &str| {
        index
            .find_symbol(name)
            .unwrap()
            .into_iter()
            .filter(|s| s.relative_path == "src/shop/Paid.java" && s.kind == "method")
            .count()
    };
    assert_eq!(methods("amount"), 1, "implicit accessor of `amount`");
    assert_eq!(
        methods("id"),
        1,
        "explicit `id()` replaces the implicit one"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_record_accessor_call_is_not_resolved_to_another_class_by_name() {
    let (index, root) = indexed();
    // `p.amount()`: `p` is declared `Paid`, so the record's accessor, never
    // `Money.amount()`. Then `.amount()` on that call's result, whose type
    // the index does not know: ambiguous between the two methods.
    let hits = calls(&index, "amount", 5);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{:?}", hits[0]);
    let target = index
        .symbols_by_ids(&[hits[0].target_id.unwrap()])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(target.relative_path, "src/shop/Paid.java");
    assert_eq!(hits[1].resolution, Resolution::Ambiguous, "{:?}", hits[1]);
    std::fs::remove_dir_all(root).ok();
}
