//! D5: `x.m()` whose receiver type the code states is qualified by that
//! type, never resolved by the name alone. Proven: `self.m()` / `cls.m()`
//! when the class itself defines `m`, a parameter annotated with a class
//! (`cat: Catalogue`), a local bound once to `T(…)`, `T.m()` on a class the
//! file defines or imports, and `self.attr.m()` when the class annotates
//! `attr` and nothing in the file states another type for it. A receiver typed by a builtin (`opts: dict`,
//! `rows = []`) is external. Any other receiver is left as it was.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_python::PythonParser;

const LEDGER: &str = r#"from shop.catalogue import Catalogue


class Ledger:
    def add(self, amount):
        return self.total(amount)

    def total(self, amount):
        return amount

    def report(self):
        return self.render()

    @classmethod
    def empty(cls):
        return cls.total(0)

    @staticmethod
    def check(self):
        return self.total(1)


def run(cat: Catalogue, opts: dict, n):
    ledger = Ledger()
    rows = []
    ledger.add(1)
    cat.find("x")
    opts.get("k")
    rows.append(1)
    n.go()
    Ledger.empty()
    other = Ledger()
    other = n
    other.add(2)
    for item in rows:
        item.run()
"#;

const CATALOGUE: &str = r#"class Catalogue:
    def find(self, title):
        return title


class Shelf:
    def find(self, title):
        return None


class View:
    def get(self, key):
        return key
"#;

fn parsed(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, path, "Python", source)
}

/// Every attribute call (`x.m()`) as `name`, plus ` q=T` when qualified by a
/// receiver type, or ` external` when the parser proved it so.
fn attribute_calls(parsed: &ParsedFile, source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .filter(|(_, r)| {
            let line = lines[r.location.line as usize - 1];
            let column = r.location.column as usize - 1;
            column > 0 && line.as_bytes()[column - 1] == b'.'
        })
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let suffix = match target {
                Some(t) if t.external => " external".to_string(),
                Some(t) => t
                    .qualifier
                    .as_ref()
                    .map(|q| format!(" q={q}"))
                    .unwrap_or_default(),
                None => String::new(),
            };
            format!("{}{suffix}", r.to_name)
        })
        .collect()
}

#[test]
fn a_proven_receiver_qualifies_the_call() {
    assert_eq!(
        attribute_calls(&parsed("shop/ledger.py", LEDGER), LEDGER),
        [
            "total q=Ledger",
            "render",
            "total q=Ledger",
            "total",
            "add q=Ledger",
            "find q=Catalogue",
            "get external",
            "append external",
            "go",
            "empty q=Ledger",
            "add",
            "run",
        ]
    );
}

#[test]
fn a_typed_receiver_reaches_its_own_class_only() {
    let root = std::env::temp_dir().join(format!("mct-py-recv-{}", std::process::id()));
    std::fs::create_dir_all(root.join("shop")).unwrap();
    std::fs::write(root.join("shop/ledger.py"), LEDGER).unwrap();
    std::fs::write(root.join("shop/catalogue.py"), CATALOGUE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let mut hits: Vec<String> = ["find", "get"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{} {}:{} {:?}",
                h.to_name, h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "find shop/ledger.py:27 Resolved",
            "get shop/ledger.py:28 External",
        ]
    );
}

/// `self.attr.m()`: the class annotates `attr`, in its body or in a method.
const STORE: &str = r#"from shop.catalogue import Catalogue


class Store:
    catalogue: Catalogue

    def __init__(self, names):
        self.names: list = names
        self.catalogue = Catalogue()

    def lookup(self, title):
        self.catalogue.find(title)
        self.names.append(title)
        self.missing.find(title)

    @staticmethod
    def helper(self):
        self.catalogue.find("x")

    def rebound(self, other):
        self = other
        self.catalogue.find("x")


def outside(store):
    store.catalogue.find("y")
"#;

#[test]
fn an_attribute_the_class_annotates_types_the_call() {
    assert_eq!(
        attribute_calls(&parsed("shop/store.py", STORE), STORE),
        [
            "find q=Catalogue",
            "append external",
            "find",
            "find",
            "find",
            "find",
        ]
    );
}

/// Nothing is proven when the file states another type for `attr`, the
/// class reads `attr` through a method, or attributes are set dynamically.
const MIXED: &str = r#"from shop.catalogue import Catalogue, Shelf


class Base:
    catalogue: Catalogue
    shelf: Catalogue
    view: Catalogue

    def go(self):
        self.catalogue.find("a")
        self.shelf.find("b")
        self.view.find("c")

    def view(self):
        return None


class Child(Base):
    catalogue: Shelf


def swap(base):
    base.shelf = Shelf()
"#;

const DYNAMIC: &str = r#"from shop.catalogue import Catalogue


class Store:
    catalogue: Catalogue

    def go(self, name, value):
        setattr(self, name, value)
        self.catalogue.find("a")
"#;

#[test]
fn an_attribute_typed_otherwise_anywhere_in_the_file_proves_nothing() {
    assert_eq!(
        attribute_calls(&parsed("shop/mixed.py", MIXED), MIXED),
        ["find", "find", "find"]
    );
    assert_eq!(
        attribute_calls(&parsed("shop/dynamic.py", DYNAMIC), DYNAMIC),
        ["find"]
    );
}
