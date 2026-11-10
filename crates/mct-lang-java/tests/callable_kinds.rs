//! A Java method invocation (`total()`, `x.total()`) can only call a method:
//! Java has no way to invoke a field, so a same-named field is never its
//! target, through the real parse -> SQLite -> query pipeline.
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

const LEDGER: &str = "\
package shop;

public class Ledger {
    private int total;

    public int add(int amount) {
        total += amount;
        return total;
    }
}
";

const BUDGET: &str = "\
package shop;

public class Budget {
    private final int limit = 10;

    public int limit() {
        return limit;
    }
}
";

const REPORT: &str = "\
package shop;

public class Report extends Base {
    public int print(Budget budget) {
        return total() + budget.limit();
    }
}
";

const BASE: &str = "\
package shop;

public class Base {
    protected int total() {
        return 7;
    }
}
";

/// A fresh temporary copy of the project above, indexed in memory. `base`
/// decides whether the superclass declaring `total()` is part of it.
fn indexed(base: bool) -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-java-callable-{}-{unique}",
        std::process::id()
    ));
    let dir = root.join("src/shop");
    std::fs::create_dir_all(&dir).unwrap();
    let mut files = vec![
        ("Ledger.java", LEDGER),
        ("Budget.java", BUDGET),
        ("Report.java", REPORT),
    ];
    if base {
        files.push(("Base.java", BASE));
    }
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    (index, root)
}

/// The one `calls` relation named `to` on `line` of `Report.java`.
fn call(index: &Index, to: &str, line: u32) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| {
            h.relative_path == "src/shop/Report.java" && h.kind == "calls" && h.line == line
        })
        .collect();
    assert_eq!(hits.len(), 1, "calls {to} on line {line}: {hits:?}");
    hits.into_iter().next().unwrap()
}

fn target_file(index: &Index, hit: &RelationHit) -> Option<String> {
    let id = hit.target_id?;
    index
        .symbols_by_ids(&[id])
        .unwrap()
        .into_iter()
        .next()
        .map(|s| s.relative_path)
}

#[test]
fn a_call_never_resolves_to_a_same_named_field() {
    // The superclass declaring `total()` is outside the project.
    let (index, root) = indexed(false);
    let hit = call(&index, "total", 5);
    assert_ne!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidate_count, 0, "{hit:?}");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_call_keeps_the_same_named_method_beside_a_field() {
    // `Budget` has both a field and a method named `limit`.
    let (index, root) = indexed(false);
    let hit = call(&index, "limit", 5);
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(
        target_file(&index, &hit).as_deref(),
        Some("src/shop/Budget.java")
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn an_inherited_call_resolves_to_the_superclass_method() {
    let (index, root) = indexed(true);
    let hit = call(&index, "total", 5);
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(
        target_file(&index, &hit).as_deref(),
        Some("src/shop/Base.java")
    );
    std::fs::remove_dir_all(root).ok();
}
