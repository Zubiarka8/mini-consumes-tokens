//! `find_dead_code_candidates` over `tests/fixtures/dead-code-app`, through
//! the real parse -> SQLite -> query pipeline: functions used only as a
//! value, only inside a macro, or only by the test harness are not reported,
//! while a genuinely unreferenced one — including one whose name is reused
//! as a local or a field elsewhere — still is.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{find_dead_code_candidates, ExcludeSet, Index};
use mct_lang_rust::RustParser;

fn candidates() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dead-code-app");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(RustParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    find_dead_code_candidates(&index, None, None)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[test]
fn a_function_used_as_a_value_is_not_a_candidate() {
    let names = candidates();
    assert!(!names.contains(&"actual_use".to_string()), "{names:?}");
}

#[test]
fn a_function_called_inside_a_macro_is_not_a_candidate() {
    let names = candidates();
    assert!(!names.contains(&"used_in_macro".to_string()), "{names:?}");
}

#[test]
fn a_test_harness_function_is_not_a_candidate() {
    let names = candidates();
    assert!(
        !names.contains(&"verifies_behavior".to_string()),
        "{names:?}"
    );
    // What it calls is referenced through it, as before.
    assert!(!names.contains(&"entry".to_string()), "{names:?}");
}

#[test]
fn unreferenced_functions_are_still_candidates() {
    let names = candidates();
    for expected in [
        "truly_unused",
        // Only a parameter of that name is used, never the function.
        "shadowed",
        // Only a local and a struct field of that name, in another file.
        "orphan",
        // `#[cfg(test)]` alone is not a harness attribute.
        "unused_test_helper",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "{expected} missing from {names:?}"
        );
    }
}
