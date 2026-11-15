//! A `fun` declared inside a function or method is a local function: it
//! stays a `function`, and its `parent` is the function it is declared in.
//! A member keeps its type as parent, and a top-level `fun` has none.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"fun outer() {
    fun inner() {
        fun deepest() {}
    }
    val callback = { fun inLambda() {} }
}

class Shop {
    fun total(): Int {
        fun twice(v: Int) = v * 2
        return twice(1)
    }
}

fun sibling() {}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/Shop.kt", "Kotlin", SOURCE)
}

fn symbol(file: &ParsedFile, name: &str) -> (SymbolKind, Option<String>) {
    let s = file
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol `{name}`"));
    (s.kind, s.parent.clone())
}

#[test]
fn a_local_function_has_the_enclosing_function_as_parent() {
    let file = parsed();
    assert_eq!(symbol(&file, "outer"), (SymbolKind::Function, None));
    assert_eq!(symbol(&file, "sibling"), (SymbolKind::Function, None));
    assert_eq!(
        symbol(&file, "inner"),
        (SymbolKind::Function, Some("outer".into()))
    );
    assert_eq!(
        symbol(&file, "deepest"),
        (SymbolKind::Function, Some("inner".into()))
    );
    assert_eq!(
        symbol(&file, "inLambda"),
        (SymbolKind::Function, Some("outer".into()))
    );
}

#[test]
fn a_local_function_in_a_method_belongs_to_the_method_and_members_keep_their_type() {
    let file = parsed();
    assert_eq!(
        symbol(&file, "twice"),
        (SymbolKind::Function, Some("total".into()))
    );
    assert_eq!(
        symbol(&file, "total"),
        (SymbolKind::Method, Some("Shop".into()))
    );
}
