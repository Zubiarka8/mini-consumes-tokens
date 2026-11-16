//! A function of this file used by name as a call argument
//! (`http.HandleFunc("/", handler)`, `apply(helper)`) or as a value in a
//! composite literal (`map[string]func(int) int{"h": helper}`) is a
//! `references` relation from where it is used, so a callback is not dead
//! code. A parameter or local of the enclosing function with the same name
//! shadows it; a variable and an unknown name record nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package main

func handler() {}
func helper(x int) int { return x }
func later() {}
var count = 3

func apply(helper func(int) int) int {
	return run(helper)
}

func shadowed() {
	handler := func() {}
	run(handler)
}

func main() {
	serve("/", handler)
	run(helper)
	m := map[string]func(int) int{"h": helper, "l": later}
	run(count, unknown, m)
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "main.go", "Go", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, u32)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.clone(), r.location.line))
        .collect()
}

#[test]
fn a_function_passed_or_stored_by_name_is_referenced() {
    let refs = references(&parsed());
    for expected in [
        ("handler", 18),
        ("helper", 19),
        ("helper", 20),
        ("later", 20),
    ] {
        assert!(
            refs.iter()
                .any(|(to, line)| (to.as_str(), *line) == expected),
            "{expected:?} missing from {refs:?}"
        );
    }
}

#[test]
fn a_shadowing_parameter_or_local_a_variable_or_an_unknown_name_is_not_referenced() {
    let refs = references(&parsed());
    // line 9: `helper` is `apply`'s parameter; line 14: `handler` is a local.
    assert!(!refs.contains(&("helper".to_string(), 9)), "{refs:?}");
    assert!(!refs.contains(&("handler".to_string(), 14)), "{refs:?}");
    for name in ["count", "unknown", "m"] {
        assert!(!refs.iter().any(|(to, _)| to == name), "{name}: {refs:?}");
    }
}
