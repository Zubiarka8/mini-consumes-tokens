//! D5: `x.m()` whose receiver type the code states is qualified by that
//! type, never resolved by the name alone. Proven: a parameter typed `T`,
//! `&T`, `&mut T` or `T<..>`, a `let x: T` and a `let x = T { .. }`, each
//! the only binding of that name in the function. A type parameter, a `dyn`
//! or `impl` type, and a standard library type (`String`, `Box<T>`, which
//! may deref to something else) are not proof. Any other receiver is left
//! as it was.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_rust::RustParser;

const LEDGER: &str = r#"use crate::catalogue::Catalogue;

pub struct Ledger {
    total: u32,
}

impl Ledger {
    pub fn add(&mut self, amount: u32) {
        self.total += amount;
    }
}

pub fn run<T: Shelf>(
    ledger: &mut Ledger,
    cat: &Catalogue,
    name: String,
    shelf: T,
    boxed: Box<Ledger>,
    any: &dyn Shelf,
    n: u32,
) {
    ledger.add(1);
    cat.find("x");
    name.find("x");
    shelf.find("x");
    boxed.add(2);
    any.find("y");
    let typed: Catalogue = Catalogue::default();
    typed.find("z");
    let built = Ledger { total: n };
    built.add(3);
    let guess = Catalogue::default();
    guess.find("w");
    let again: Ledger = Ledger { total: 0 };
    let again = n;
    again.add(4);
    for item in [1, 2] {
        item.find("v");
    }
    println!("{:?} {:?}", cat.find("m"), ledger.cat.find("n"));
}

pub trait Shelf {
    fn find(&self, title: &str);
}
"#;

const SETTINGS: &str = r#"use serde_json::Value;
use crate::model::{dimensions::Size};
use crate::model::dimensions::Size as Dim;

pub fn theme(value: &Value) -> bool {
    value.as_str().is_some()
}

pub fn fits(shelf: &Size, other: &Dim) -> bool {
    shelf.volume() > other.volume()
}
"#;

const MODEL: &str = r#"pub mod dimensions {
    pub struct Size;

    impl Size {
        pub fn volume(&self) -> u32 {
            1
        }
    }
}

pub struct Crate;

impl Crate {
    pub fn volume(&self) -> u32 {
        2
    }
}
"#;

const DATA: &str = r#"pub enum Value {
    Text(String),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        None
    }
}

pub struct Index {
    root: String,
}

impl Index {
    pub fn root(&self) -> &str {
        &self.root
    }
}

pub fn root_of(index: &Index) -> &str {
    index.root()
}
"#;

const CATALOGUE: &str = r#"#[derive(Default)]
pub struct Catalogue;

impl Catalogue {
    pub fn find(&self, title: &str) {}
}

pub struct Archive;

impl Archive {
    pub fn find(&self, title: &str) {}
}
"#;

fn parsed(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&RustParser, path, "Rust", source)
}

/// Every method call (`x.m()`) as `name`, plus ` q=T` when qualified by a
/// receiver type.
fn method_calls(parsed: &ParsedFile, source: &str) -> Vec<String> {
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
            let qualifier = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.as_ref())
                .map(|q| format!(" q={q}"))
                .unwrap_or_default();
            format!("{}{qualifier}", r.to_name)
        })
        .collect()
}

#[test]
fn a_stated_receiver_type_qualifies_the_call() {
    assert_eq!(
        method_calls(&parsed("src/ledger.rs", LEDGER), LEDGER),
        [
            "add q=Ledger",
            "find q=Catalogue",
            "find",
            "find",
            "add",
            "find",
            "find q=Catalogue",
            "add q=Ledger",
            "find",
            "add",
            "find",
        ]
    );
}

#[test]
fn a_receiver_inside_a_macro_is_the_name_before_the_call_only() {
    let parsed = parsed("src/ledger.rs", LEDGER);
    let in_macro: Vec<String> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References && r.to_name == "find")
        .map(|(i, r)| {
            let qualifier = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.clone());
            format!("{}:{} {qualifier:?}", r.location.line, r.location.column)
        })
        .collect();
    assert_eq!(in_macro, ["40:31 Some(\"Catalogue\")", "40:53 None"]);
}

#[test]
fn a_typed_receiver_reaches_a_method_of_that_type_only() {
    let root = std::env::temp_dir().join(format!("mct-rs-recv-ext-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/settings.rs"), SETTINGS).unwrap();
    std::fs::write(root.join("src/data.rs"), DATA).unwrap();
    std::fs::write(root.join("src/model.rs"), MODEL).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let hits: Vec<String> = ["as_str", "root", "volume"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    // `index.root()` is the method, never the same-named field. A
    // `crate::` path reaches an inline module's type; an alias proves
    // nothing, since the methods' parent is the type's real name.
    assert_eq!(
        hits,
        [
            "src/settings.rs:6 Unresolved",
            "src/data.rs:22 Resolved",
            "src/settings.rs:10 Resolved",
            "src/settings.rs:10 Ambiguous",
        ]
    );
}

#[test]
fn a_typed_receiver_reaches_its_own_type_only() {
    let root = std::env::temp_dir().join(format!("mct-rs-recv-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/ledger.rs"), LEDGER).unwrap();
    std::fs::write(root.join("src/catalogue.rs"), CATALOGUE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = index
        .find_references("find")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .filter(|h| [23, 29].contains(&h.line))
        .map(|h| {
            format!(
                "{}:{} {:?} {}",
                h.relative_path, h.line, h.resolution, h.candidate_count
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        ["src/ledger.rs:23 Resolved 1", "src/ledger.rs:29 Resolved 1",]
    );
}
