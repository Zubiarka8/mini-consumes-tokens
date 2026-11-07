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
    mct_corpus::import_hits_with_path_suffix(index, "app/Main.kt", to)
}

#[test]
fn type_imports_resolve_to_the_declaring_file() {
    let index = open_indexed();
    mct_corpus::assert_import_targets(
        &index,
        "app/Main.kt",
        &[
            ("Money", "com/acme/model/Money.kt"),
            // An aliased import still depends on the real type's file.
            ("Strings", "com/acme/util/Strings.kt"),
        ],
    );
}

#[test]
fn a_stdlib_import_never_resolves_to_a_local_namesake() {
    let index = open_indexed();
    let list = import_hits(&index, "List");
    mct_corpus::assert_external_not_local(&list, "com/acme/util/List.kt");
}
