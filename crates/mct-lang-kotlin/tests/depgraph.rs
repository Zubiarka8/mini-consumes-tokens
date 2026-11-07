//! File-level `Imports` evidence: a Kotlin type import names the `Module`
//! of the file declaring the type, scoped by its package segment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_kotlin::KotlinParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser))
}

/// The resolutions of every `imports` relation named `to` in `Main.kt`.
fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path.ends_with("app/Main.kt"))
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

#[test]
fn type_imports_resolve_to_the_declaring_file() {
    let index = open_indexed();
    let money = import_hits(&index, "Money");
    assert!(
        money.contains(&(Resolution::Resolved, Some("com/acme/model/Money.kt".into()))),
        "{money:?}"
    );
    // An aliased import still depends on the real type's file.
    let strings = import_hits(&index, "Strings");
    assert!(
        strings.contains(&(
            Resolution::Resolved,
            Some("com/acme/util/Strings.kt".into())
        )),
        "{strings:?}"
    );
}

#[test]
fn a_stdlib_import_never_resolves_to_a_local_namesake() {
    let index = open_indexed();
    let list = import_hits(&index, "List");
    assert!(
        list.iter().any(|(r, _)| *r == Resolution::External),
        "{list:?}"
    );
    assert!(!list.iter().any(|(r, f)| {
        *r == Resolution::Resolved && f.as_deref() == Some("com/acme/util/List.kt")
    }));
}
