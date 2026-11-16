//! A file-level `readonly X=1` or `declare -r X=1` (any `declare`/`typeset`
//! flag group with `r`) can never change, so it is a `constant`; `export`,
//! `declare -i` and a bare assignment stay `variable`s.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_bash::BashParser;

fn kinds(source: &str) -> Vec<(String, SymbolKind)> {
    mct_corpus::parse_source(&BashParser, "bin/a.sh", "Bash", source)
        .symbols
        .into_iter()
        .filter(|s| matches!(s.kind, SymbolKind::Constant | SymbolKind::Variable))
        .map(|s| (s.name, s.kind))
        .collect()
}

#[test]
fn a_readonly_declaration_is_a_constant() {
    let found = kinds(
        "readonly RO=1\ndeclare -r DR=2\ndeclare -rx DRX=3\ndeclare -ar ARR=(1 2)\ntypeset -r TR=4\ndeclare -r -x SPLIT=5\n",
    );
    for name in ["RO", "DR", "DRX", "ARR", "TR", "SPLIT"] {
        assert!(
            found.contains(&(name.to_string(), SymbolKind::Constant)),
            "{name} should be a constant: {found:?}"
        );
    }
}

#[test]
fn any_other_assignment_stays_a_variable() {
    let found = kinds("export EXP=1\ndeclare -i INT=2\ndeclare -x X=3\nPLAIN=4\ndeclare -A MAP\n");
    for name in ["EXP", "INT", "X", "PLAIN"] {
        assert!(
            found.contains(&(name.to_string(), SymbolKind::Variable)),
            "{name} should be a variable: {found:?}"
        );
    }
    assert!(
        !found.iter().any(|(_, k)| *k == SymbolKind::Constant),
        "{found:?}"
    );
}
