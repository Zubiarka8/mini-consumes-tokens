//! A Java call `x.m()` resolves to a method of `x`'s declared type when the
//! source declares it (parameter, local, field, `for`, pattern, `catch`),
//! and `Type.m()` to a method of `Type`. Any other receiver is a member call
//! whose target stays ambiguous even when only one same-named method exists,
//! through the real parse -> SQLite -> query pipeline.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. Every file compiles with
//! `javac` and `Report.main` prints 20.

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
    public static class Rates {
        public static int unit() {
            return 0;
        }
    }

    public static Money of(int cents) {
        return new Money();
    }

    public int amount() {
        return 1;
    }

    public int cents() {
        return 1;
    }
}
";

const RANGE: &str = "\
package shop;

public class Range {
    public int max(int a, int b) {
        return a;
    }
}
";

const PAID: &str = "\
package shop;

public record Paid(Money amount) {}
";

const REPORT: &str = "\
package shop;

import java.util.List;
import java.util.function.Function;

public class Report<T extends Money> {
    private final Money fee = Money.of(2);

    public int print(Paid p, List<Paid> all, T t) {
        Money local = Money.of(1);
        var inferred = local;
        int sum = p.amount().amount() + local.amount() + fee.amount();
        for (Paid each : all) {
            sum += each.amount().amount();
        }
        Function<Money, Integer> f = q -> q.cents();
        sum += inferred.cents() + t.cents() + Math.max(1, 2);
        return sum + this.fee.amount() + shop.Money.Rates.unit() + Money.Rates.unit();
    }

    public static void main(String[] args) {
        Paid paid = new Paid(Money.of(3));
        System.out.println(new Report<Money>().print(paid, List.of(paid), new Money()) + 11);
    }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-java-receiver-{}-{unique}",
        std::process::id()
    ));
    let dir = root.join("src/shop");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, contents) in [
        ("Money.java", MONEY),
        ("Range.java", RANGE),
        ("Paid.java", PAID),
        ("Report.java", REPORT),
    ] {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    (index, root)
}

/// The `calls` relation named `to` on `line` of `Report.java` whose name
/// starts at `column` (1-based, as the index reports it).
fn call(index: &Index, to: &str, line: u32, column: u32) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| {
            h.relative_path == "src/shop/Report.java"
                && h.kind == "calls"
                && h.line == line
                && h.column == column
        })
        .collect();
    assert_eq!(hits.len(), 1, "calls {to} at {line}:{column}: {hits:?}");
    hits.into_iter().next().unwrap()
}

fn column_of(line: u32, needle: &str) -> u32 {
    let text = REPORT.lines().nth(line as usize - 1).unwrap();
    text.find(needle).unwrap() as u32 + 1
}

/// The file the call resolves to, or `None` when it is not resolved.
fn resolved_file(index: &Index, hit: &RelationHit) -> Option<String> {
    if hit.resolution != Resolution::Resolved {
        return None;
    }
    let id = hit.target_id?;
    index
        .symbols_by_ids(&[id])
        .unwrap()
        .into_iter()
        .next()
        .map(|s| s.relative_path)
}

fn at(index: &Index, line: u32, needle: &str, name: &str) -> (Resolution, Option<String>) {
    let column = column_of(line, needle) + needle.find(name).unwrap() as u32;
    let hit = call(index, name, line, column);
    (hit.resolution, resolved_file(index, &hit))
}

#[test]
fn a_typed_parameter_local_field_or_loop_variable_qualifies_the_call() {
    let (index, root) = indexed();
    let paid = Some("src/shop/Paid.java".to_string());
    let money = Some("src/shop/Money.java".to_string());
    assert_eq!(at(&index, 12, "p.amount()", "amount").1, paid);
    assert_eq!(at(&index, 12, "local.amount()", "amount").1, money);
    assert_eq!(at(&index, 12, "fee.amount()", "amount").1, money);
    assert_eq!(at(&index, 14, "each.amount()", "amount").1, paid);
    assert_eq!(at(&index, 18, "this.fee.amount()", "amount").1, money);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_type_name_receiver_qualifies_a_static_call() {
    let (index, root) = indexed();
    let money = Some("src/shop/Money.java".to_string());
    assert_eq!(at(&index, 10, "Money.of(1)", "of").1, money);
    // A nested type through its outer type, with or without the package.
    let rates = "Money.Rates.unit()";
    let column = column_of(18, rates) + rates.find("unit").unwrap() as u32;
    let hit = call(&index, "unit", 18, column);
    assert_eq!(resolved_file(&index, &hit), money, "{hit:?}");
    let column = column_of(18, "shop.Money") + "shop.Money.Rates.".len() as u32;
    let hit = call(&index, "unit", 18, column);
    assert_eq!(resolved_file(&index, &hit), money, "{hit:?}");
    // `Math` declares no `max` here: never Range.max by name.
    let (resolution, file) = at(&index, 17, "Math.max", "max");
    assert_eq!(file, None, "{resolution:?}");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn an_unproven_receiver_stays_ambiguous() {
    let (index, root) = indexed();
    // A call's result has two same-named candidates.
    let column = column_of(12, "p.amount().amount()") + "p.amount().".len() as u32;
    let hit = call(&index, "amount", 12, column);
    assert_eq!(hit.resolution, Resolution::Ambiguous, "{hit:?}");
    // An untyped lambda parameter, `var` and a type variable: `cents` has a
    // single candidate, which still is not proof of the receiver's type.
    for (line, needle) in [
        (16, "q.cents()"),
        (17, "inferred.cents()"),
        (17, "t.cents()"),
    ] {
        let (resolution, file) = at(&index, line, needle, "cents");
        assert_eq!(resolution, Resolution::Ambiguous, "{needle}: {file:?}");
    }
    std::fs::remove_dir_all(root).ok();
}
