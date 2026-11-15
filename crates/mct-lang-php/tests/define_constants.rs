//! `define('NAME', value)` declares a global constant at run time, as
//! `const NAME = value;` does at the top level. Only a call to the global
//! `define` with a plain string literal as the name is a declaration: a
//! computed name, a double-quoted name that interpolates, or a method called
//! `define` proves nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_php::PhpParser;

fn constants(source: &str) -> Vec<(String, Option<String>)> {
    mct_corpus::parse_source(&PhpParser, "src/a.php", "PHP", source)
        .symbols
        .into_iter()
        .filter(|s| s.kind == SymbolKind::Constant)
        .map(|s| (s.name, s.parent))
        .collect()
}

#[test]
fn define_with_a_literal_name_is_a_global_constant() {
    let found = constants(
        "<?php\nnamespace App;\ndefine('P_DEF', 3);\ndefine(\"P_TWO\", 4);\n\\define('P_ROOT', 5);\n",
    );
    for name in ["P_DEF", "P_TWO", "P_ROOT"] {
        assert!(
            found.contains(&(name.to_string(), None)),
            "{name} missing from {found:?}"
        );
    }
}

#[test]
fn define_inside_a_function_or_class_has_no_parent() {
    let found = constants(
        "<?php\nclass K { function m() { define('P_IN', 1); } }\nif (!defined('P_IF')) { define('P_IF', 2); }\n",
    );
    assert!(found.contains(&("P_IN".to_string(), None)), "{found:?}");
    assert!(found.contains(&("P_IF".to_string(), None)), "{found:?}");
}

#[test]
fn only_a_literal_name_to_the_global_define_counts() {
    let found = constants(
        "<?php\nnamespace App;\n$n = 'X';\ndefine($n, 1);\ndefine(\"A_$n\", 2);\ndefine('PREFIX_' . $n, 3);\n$o->define('NOT_A_CONST', 4);\nK::define('ALSO_NOT', 5);\ndefine();\n",
    );
    assert_eq!(found, Vec::new());
}
