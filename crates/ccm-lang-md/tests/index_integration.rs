//! Runs the real `ccm-index` pipeline against a single small docs fixture
//! with nested ATX headings, mirroring `crates/ccm-lang-xml`'s integration
//! test shape.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use ccm_core::LanguageRegistry;
use ccm_index::{ExcludeSet, Index};
use ccm_lang_md::MarkdownParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-project")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(MarkdownParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 1, "architecture.md");
    assert!(
        report.issues.is_empty(),
        "no parse issues expected: {:?}",
        report.issues
    );
    index
}

#[test]
fn find_symbol_locates_a_nested_heading() {
    let index = open_indexed();
    assert_eq!(index.find_symbol("Indexer").unwrap().len(), 1);
}

#[test]
fn find_symbol_reports_element_kind_and_correct_parent() {
    let index = open_indexed();
    let indexer = index.find_symbol("Indexer").unwrap();
    assert_eq!(indexer.len(), 1);
    assert_eq!(indexer[0].kind, "element");
    assert_eq!(indexer[0].parent.as_deref(), Some("Components"));
}

#[test]
fn a_second_top_level_heading_does_not_inherit_the_first_ones_hierarchy() {
    let index = open_indexed();
    let unit_tests = index.find_symbol("Unit Tests").unwrap();
    assert_eq!(unit_tests.len(), 1);
    assert_eq!(unit_tests[0].parent.as_deref(), Some("Testing"));
}

#[test]
fn status_reports_full_markdown_coverage() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let md = status
        .languages
        .iter()
        .find(|l| l.language == "markdown")
        .unwrap();
    assert_eq!(md.file_count, 1);
    assert_eq!(
        md.symbol_count, 7,
        "Architecture, Overview, Components, Indexer, Parser, Testing, Unit Tests"
    );
}

#[test]
fn a_wikilink_in_the_fixture_resolves_as_a_references_relation() {
    let index = open_indexed();
    let hits = index.find_references("Parser").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

#[test]
fn a_tag_in_the_fixture_resolves_as_a_tag_prefixed_reference() {
    let index = open_indexed();
    let hits = index.find_references("tag:core").unwrap();
    assert!(hits.iter().any(|h| h.from_symbol == "Indexer"), "{hits:?}");
}

#[test]
fn a_wikilink_anchors_heading_part_is_discoverable_by_name() {
    let index = open_indexed();
    // `[[Parser#Testing]]` must index its note part exactly like a plain
    // `[[Parser]]` link (already covered by the test above) and must ALSO
    // index its heading part as an independent relation, discoverable by
    // `find_references("Testing")` — "Testing" is a real H1 heading in this
    // fixture. Note: the public `Index`/`RelationHit` API has no accessor
    // for the resolved `to_symbol_id` column, so this only proves the
    // relation is stored and discoverable by name (exactly what
    // `find_references` guarantees regardless of resolution), not that the
    // foreign key itself is non-NULL.
    let hits = index.find_references("Testing").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

#[test]
fn a_wikilink_to_a_nonexistent_target_is_still_discoverable_by_name() {
    let index = open_indexed();
    // No heading named "Nonexistent Page" exists anywhere in the fixture, so
    // `to_symbol_id` stays NULL at insert time — but the relation row is
    // still inserted unconditionally, and `find_references` matches purely
    // on the `to_name` string column, never on `to_symbol_id`. Pre-existing
    // `ccm-index` behavior, not new machinery; reconfirmed here because
    // Phase 3 introduces a new way (the heading-part split) to end up with
    // an unresolved name.
    let hits = index.find_references("Nonexistent Page").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}
