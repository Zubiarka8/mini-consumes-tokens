//! A method reference `A::run` names a method and is a `references` relation
//! from the code that writes it, so a method used only that way is not dead.
//! An enum constant's arguments (`FIRST(make())`) run with the constant, so
//! their calls are indexed.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_java::JavaParser;

fn parse(src: &str) -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "src/shop/Cart.java", "Java", src)
}

/// `(from, to)` of every relation of `kind`.
fn pairs(parsed: &ParsedFile, kind: RelationKind) -> Vec<(String, String)> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| {
            let from = parsed
                .symbols
                .iter()
                .find(|s| s.id == r.from)
                .map_or(String::new(), |s| s.name.clone());
            (from, r.to_name.clone())
        })
        .collect()
}

#[test]
fn a_method_reference_references_its_method() {
    let parsed = parse(
        "class Cart {\n  int total() { return 1; }\n  static int size(Cart c) { return 2; }\n\
         void run(java.util.List<Cart> l) {\n    l.stream().map(Cart::size);\n    \
         Runnable r = this::total;\n  }\n}\n",
    );
    let found = pairs(&parsed, RelationKind::References);
    for target in ["size", "total"] {
        assert!(
            found.contains(&("run".into(), target.into())),
            "run -> {target} missing in {found:?}"
        );
    }
}

#[test]
fn a_constructor_reference_names_no_method() {
    let parsed = parse(
        "class Cart {\n  void run() { java.util.function.Supplier<Cart> s = Cart::new; }\n}\n",
    );
    let found = pairs(&parsed, RelationKind::References);
    assert!(
        !found.iter().any(|(_, to)| to == "new"),
        "unexpected `new` reference in {found:?}"
    );
}

#[test]
fn a_call_in_an_enum_constant_argument_belongs_to_the_constant() {
    let parsed = parse(
        "enum Mode {\n  FIRST(make());\n  Mode(int v) {}\n  static int make() { return 1; }\n}\n",
    );
    let found = pairs(&parsed, RelationKind::Calls);
    assert!(
        found.contains(&("FIRST".into(), "make".into())),
        "FIRST -> make missing in {found:?}"
    );
}
