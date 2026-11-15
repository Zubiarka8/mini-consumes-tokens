//! An `enum class` is an `enum`, with or without a body, entries or an
//! annotation before it; each entry stays a `field` of it and a member it
//! declares a `method` or `field`, as in Java. Any other class stays a
//! `class`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_kotlin::KotlinParser;

fn rows(source: &str) -> Vec<(SymbolKind, String, Option<String>)> {
    mct_corpus::parse_source(&KotlinParser, "src/Color.kt", "Kotlin", source)
        .symbols
        .iter()
        .skip(1)
        .map(|s| (s.kind, s.name.clone(), s.parent.clone()))
        .collect()
}

fn row(kind: SymbolKind, name: &str, parent: Option<&str>) -> (SymbolKind, String, Option<String>) {
    (kind, name.to_string(), parent.map(str::to_string))
}

#[test]
fn an_enum_class_is_an_enum() {
    let source = r#"enum class Color { RED, GREEN }
enum class Op(val sign: String) {
    ADD("+") { override fun apply() = 1 };
    abstract fun apply(): Int
}
enum class Empty
class Plain
"#;
    assert_eq!(
        rows(source),
        [
            row(SymbolKind::Enum, "Color", None),
            row(SymbolKind::Field, "RED", Some("Color")),
            row(SymbolKind::Field, "GREEN", Some("Color")),
            row(SymbolKind::Enum, "Op", None),
            row(SymbolKind::Field, "sign", Some("Op")),
            row(SymbolKind::Field, "ADD", Some("Op")),
            // A member of an entry's body belongs to the entry.
            row(SymbolKind::Method, "apply", Some("ADD")),
            row(SymbolKind::Method, "apply", Some("Op")),
            row(SymbolKind::Enum, "Empty", None),
            row(SymbolKind::Class, "Plain", None),
        ]
    );
}

#[test]
fn an_annotated_one_entry_enum_class_is_an_enum() {
    assert_eq!(
        rows("@Other\nenum class Color { RED }\n"),
        [
            row(SymbolKind::Enum, "Color", None),
            row(SymbolKind::Field, "RED", Some("Color")),
        ]
    );
}
