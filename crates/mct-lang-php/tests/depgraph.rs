//! File-level `Imports` evidence: a PHP `use` names the `Module` of the file
//! declaring the type (PSR-4) and an `include` with a literal path is pinned
//! to the exact file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_php::PhpParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PhpParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// Resolutions and first candidate files of the `imports` relations named
/// `to` in `index.php`.
fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path == "index.php" && h.to_name == to)
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

fn resolved(file: &str) -> (Resolution, Option<String>) {
    (Resolution::Resolved, Some(file.to_string()))
}

#[test]
fn use_resolves_to_the_file_under_the_namespace_directory() {
    let index = open_indexed();
    // `src/User.php` also holds a `User` module; the `Models` segment picks
    // the right one.
    assert!(import_hits(&index, "User").contains(&resolved("src/Models/User.php")));
    // Group use, with and without alias, names the real type's file.
    assert!(import_hits(&index, "Post").contains(&resolved("src/Models/Post.php")));
    assert!(import_hits(&index, "Tag").contains(&resolved("src/Models/Tag.php")));
}

#[test]
fn literal_includes_are_pinned_to_their_file() {
    let index = open_indexed();
    assert!(import_hits(&index, "helpers").contains(&resolved("lib/helpers.php")));
    assert!(import_hits(&index, "boot").contains(&resolved("lib/boot.php")));
}

#[test]
fn function_use_and_computed_includes_add_no_module_relation() {
    use mct_core::{LanguageParser, SourceFile};
    let parsed = PhpParser
        .parse(&SourceFile {
            relative_path: "a.php".into(),
            contents:
                "<?php\nuse function A\\f;\nuse const A\\C;\ninclude $x;\ninclude '/abs/y.php';\n"
                    .into(),
        })
        .unwrap();
    assert!(parsed.relation_targets.is_empty());
}
