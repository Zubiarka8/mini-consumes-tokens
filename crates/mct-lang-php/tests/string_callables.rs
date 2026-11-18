//! A function named by a string in a callback position
//! (`array_map('shout', $xs)`, `usort($xs, 'by_name')`) is a `references`
//! relation to that function, so a callback is not dead code. Only a
//! function declared in this file, and only as an argument of a PHP
//! function that takes a callable, counts.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_php::PhpParser;

const SOURCE: &str = r#"<?php
function shout($s) { return strtoupper($s); }
function by_name($a, $b) { return strcmp($a, $b); }
function on_exit() {}
function plain($s) { return $s; }

array_map('shout', ['a']);
usort($rows, "by_name");
register_shutdown_function('on_exit');
array_map('missing', []);
echo 'plain';
strlen('shout');
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PhpParser, "src/cb.php", "PHP", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, u32)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.clone(), r.location.line))
        .collect()
}

#[test]
fn a_function_named_by_a_string_callback_is_referenced() {
    let refs = references(&parsed());
    for expected in [("shout", 7), ("by_name", 8), ("on_exit", 9)] {
        assert!(
            refs.iter()
                .any(|(to, line)| (to.as_str(), *line) == expected),
            "{expected:?} missing from {refs:?}"
        );
    }
}

#[test]
fn other_strings_and_unknown_functions_are_not_references() {
    let refs = references(&parsed());
    // `missing` is no function of this file; `echo 'plain'` and
    // `strlen('shout')` take no callable.
    for name in ["missing", "plain"] {
        assert!(!refs.iter().any(|(to, _)| to == name), "{name}: {refs:?}");
    }
    assert!(!refs.iter().any(|(_, line)| *line == 12), "{refs:?}");
}
