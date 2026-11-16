//! A name listed in a module's `__all__` is exported on purpose: a
//! `references` relation from the module to the function or class of this
//! file it names, so a public API that is only re-exported is not dead
//! code. A name that is not defined here, an `__all__` inside a function and
//! one that is not a list of strings record nothing.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_python::PythonParser;

fn exported(source: &str) -> Vec<(String, bool)> {
    let parsed = mct_corpus::parse_source(&PythonParser, "pkg/api.py", "Python", source);
    let mut rows: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let pinned = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.path.as_deref() == Some("pkg/api.py"));
            (r.to_name.clone(), pinned)
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn a_listed_name_defined_here_is_referenced() {
    let rows = exported(
        "__all__ = [\"public\", \"Widget\", 'missing']\n__all__ += (\"later\",)\ndef public(): pass\nclass Widget: pass\ndef later(): pass\n",
    );
    assert_eq!(
        rows,
        [
            ("Widget".to_string(), true),
            ("later".to_string(), true),
            ("public".to_string(), true),
        ]
    );
}

#[test]
fn an_all_inside_a_function_or_not_a_list_records_nothing() {
    assert!(exported("def f():\n    __all__ = [\"f\"]\n").is_empty());
    assert!(exported("def g(): pass\n__all__ = compute(\"g\")\n").is_empty());
    assert!(exported("def g(): pass\nother = [\"g\"]\n").is_empty());
}
