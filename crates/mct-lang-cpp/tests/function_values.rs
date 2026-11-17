//! A function of this file passed by name as a call argument
//! (`std::transform(first, last, out, dbl)`) is a `references` relation, so
//! a callback is not dead code. A parameter of the enclosing function with
//! the same name, a variable and an unknown name record nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"int dbl(int x) { return x * 2; }
bool less(int a, int b) { return a < b; }
int total = 3;

void apply(int (*dbl)(int)) {
    run(dbl);
}

void work(std::vector<int>& v) {
    std::transform(v.begin(), v.end(), v.begin(), dbl);
    std::sort(v.begin(), v.end(), less);
    run(total, unknown);
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/work.cpp", "C++", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, u32)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.clone(), r.location.line))
        .collect()
}

#[test]
fn a_function_passed_by_name_is_referenced() {
    let refs = references(&parsed());
    assert!(refs.contains(&("dbl".to_string(), 10)), "{refs:?}");
    assert!(refs.contains(&("less".to_string(), 11)), "{refs:?}");
}

#[test]
fn a_shadowing_parameter_a_variable_or_an_unknown_name_is_not_referenced() {
    let refs = references(&parsed());
    // line 6: `dbl` is `apply`'s own parameter.
    assert!(!refs.contains(&("dbl".to_string(), 6)), "{refs:?}");
    for name in ["total", "unknown"] {
        assert!(!refs.iter().any(|(to, _)| to == name), "{name}: {refs:?}");
    }
}
