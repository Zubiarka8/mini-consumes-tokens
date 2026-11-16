//! A closure bound to a variable (`$cb = function () {}`) is a function named
//! after the variable. Reading that variable as a value afterwards (passing
//! it, returning it, storing it) is a `references` relation to the closure,
//! as calling it is a call. Assigning to the variable, or reading a variable
//! no closure was bound to, names nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_php::PhpParser;

fn references(source: &str) -> Vec<(String, String)> {
    let parsed = mct_corpus::parse_source(&PhpParser, "src/a.php", "PHP", source);
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
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
fn a_bound_closure_read_as_a_value_is_referenced() {
    let found = references(
        "<?php\nfunction build() {\n  $f = function () { return 1; };\n  $g = fn() => 2;\n  \
         $k = array_map($f, [1]);\n  return [$g, $k];\n}\n",
    );
    for target in ["f", "g"] {
        assert!(
            found.contains(&("build".into(), target.into())),
            "build -> {target} missing in {found:?}"
        );
    }
}

#[test]
fn an_assignment_or_an_unbound_variable_is_not_a_use() {
    let found = references(
        "<?php\nfunction build($other) {\n  $f = function () { return 1; };\n  $f = 5;\n  \
         return $other;\n}\n",
    );
    assert!(
        !found.iter().any(|(_, to)| to == "f" || to == "other"),
        "unexpected reference in {found:?}"
    );
}
