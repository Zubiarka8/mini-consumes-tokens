//! File-level `Imports` evidence: a quoted `#include` is pinned to the exact
//! file it names, so a header is never confused with its same-named source.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_cpp::CppParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CppParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// Resolution and first candidate of the `imports` relation named `to` in
/// `from_file`.
fn import_hit(index: &Index, to: &str, from_file: &str) -> Vec<(Resolution, Option<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path == from_file && h.to_name == to)
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

#[test]
fn quoted_includes_resolve_to_the_exact_header() {
    let index = open_indexed();
    // `math.h` and `math.cpp` both hold a `math` module; the path decides.
    let hits = import_hit(&index, "math", "src/main.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("src/util/math.h".to_string()))]
    );
    let hits = import_hit(&index, "math", "src/util/math.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("src/util/math.h".to_string()))]
    );
    let hits = import_hit(&index, "config", "src/main.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("include/config.h".to_string()))]
    );
}

#[test]
fn includes_found_only_through_an_include_path_stay_unresolved() {
    let index = open_indexed();
    let hits = import_hit(&index, "missing_on_include_path", "src/main.cpp");
    assert_eq!(hits, [(Resolution::Unresolved, None)]);
    // An `<angle>` include proves nothing: only the plain name relation
    // exists, with no file evidence.
    let hits = import_hit(&index, "vector", "src/main.cpp");
    assert_eq!(hits, [(Resolution::Unresolved, None)]);
}
