//! File-level `Imports` evidence: a PHP `use` names the `Module` of the file
//! that declares the type in the imported namespace, wherever Composer maps
//! that file, and an `include` with a literal path is pinned to the exact
//! file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_php::PhpParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser))
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
fn use_resolves_to_the_file_declaring_the_imported_namespace() {
    let index = open_indexed();
    // `src/User.php` also holds a `User` module, in another namespace.
    assert!(import_hits(&index, "User").contains(&resolved("src/Models/User.php")));
    // `App\Account` sits directly in the namespace root (`app/`), so no
    // directory is named after its namespace segment, and a same-named
    // class lives in another namespace.
    assert_eq!(
        import_hits(&index, "Account"),
        vec![resolved("app/Account.php")]
    );
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
fn function_use_and_computed_includes_do_not_target_a_module() {
    use mct_core::{LanguageParser, SourceFile};
    let parsed = PhpParser
        .parse(&SourceFile {
            relative_path: "a.php".into(),
            contents:
                "<?php\nuse function A\\f;\nuse const A\\C;\ninclude $x;\ninclude '/abs/y.php';\n"
                    .into(),
        })
        .unwrap();
    // A function or constant import targets that symbol kind in its
    // namespace; a computed include records nothing and an absolute one
    // only its literal spelling, with no target.
    assert_eq!(parsed.relations.len(), 3, "{:?}", parsed.relations);
    assert_eq!(parsed.relation_targets.len(), 2);
    assert!(parsed
        .relation_targets
        .iter()
        .all(|t| t.kind != Some(mct_core::SymbolKind::Module)));
}
