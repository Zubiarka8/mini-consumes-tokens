//! A `static final` field is a constant of its type, and so is any field of
//! an interface (JLS §9.3: implicitly `public static final`), one per
//! declarator. A `final` instance field, a `static` field that is not
//! `final`, and a plain field stay `field`s.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"
class Limits {
    public static final int MAX = 10, MIN = 1;
    private static final String NAME = "n";
    final int slots = 2;
    static int counter;
    int count;
    void run() { final int local = 3; }
}
interface Config {
    int TIMEOUT = 30;
    void apply();
}
enum Level { LOW, HIGH }
"#;

fn kinds() -> Vec<(String, SymbolKind, Option<String>)> {
    mct_corpus::parse_source(&JavaParser, "src/Limits.java", "Java", SOURCE)
        .symbols
        .into_iter()
        .filter(|s| matches!(s.kind, SymbolKind::Constant | SymbolKind::Field))
        .map(|s| (s.name, s.kind, s.parent))
        .collect()
}

#[test]
fn a_static_final_or_interface_field_is_a_constant() {
    let found = kinds();
    for (name, parent) in [
        ("MAX", "Limits"),
        ("MIN", "Limits"),
        ("NAME", "Limits"),
        ("TIMEOUT", "Config"),
    ] {
        assert!(
            found.contains(&(
                name.to_string(),
                SymbolKind::Constant,
                Some(parent.to_string())
            )),
            "{name} should be a constant: {found:?}"
        );
    }
}

#[test]
fn any_other_field_stays_a_field() {
    let found = kinds();
    for name in ["slots", "counter", "count", "LOW", "HIGH"] {
        assert!(
            found
                .iter()
                .any(|(n, k, _)| n == name && *k == SymbolKind::Field),
            "{name} should be a field: {found:?}"
        );
    }
    assert!(found.iter().all(|(n, _, _)| n != "local"), "{found:?}");
}
