//! Runs the real `mct-index` pipeline against a single small config file
//! mixing `id`, lowercase `name`, and capitalized `Name` attributes across
//! nested elements.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_xml::XmlParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config")
}

/// The 3-file sibling fixture, for the multi-file assertions the 1-file
/// `config/` fixture cannot make.
fn services_fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/services-config")
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(XmlParser));
    registry
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1, "settings.xml");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

fn open_services_indexed() -> Index {
    let root = services_fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 3, "api.xml, logging.xml, worker.xml");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_elements_named_by_each_attribute_convention() {
    let index = open_indexed();
    assert_eq!(index.find_symbol("api").unwrap().len(), 1);
    assert_eq!(index.find_symbol("health").unwrap().len(), 1);
    assert_eq!(index.find_symbol("jobs").unwrap().len(), 1, "Name (capitalized) must resolve too");
}

#[test]
fn find_symbol_reports_element_kind_and_parent() {
    let index = open_indexed();
    let health = index.find_symbol("health").unwrap();
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].kind, "element");
    assert_eq!(health[0].parent.as_deref(), Some("api"));
}

#[test]
fn find_symbol_reports_file_language_and_exact_source_line() {
    let index = open_indexed();
    // XML has no calls or scopes to cross-check a symbol against, so the
    // recorded position is the only thing tying an indexed element back to
    // the source text — assert it exactly, not just that it is non-zero.
    // Line numbers are 1-based (see `mct_core::Location`).
    let api = index.find_symbol("api").unwrap();
    assert_eq!(api[0].relative_path, "settings.xml");
    assert_eq!(api[0].language, "xml", "files.language via the symbols->files JOIN");
    assert_eq!(api[0].line, 2, "`<server id=\"api\">` is on line 2");

    let health = index.find_symbol("health").unwrap();
    assert_eq!(health[0].line, 3);

    let jobs = index.find_symbol("jobs").unwrap();
    assert_eq!(jobs[0].line, 7);
}

#[test]
fn status_reports_full_xml_coverage_with_no_relations() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let xml = status.languages.iter().find(|l| l.language == "xml").unwrap();
    assert_eq!(xml.file_count, 1);
    assert_eq!(xml.symbol_count, 6, "module root + api + health + metrics + worker + jobs");

    // The "with no relations" half of this test's name was never actually
    // checked. `mct-lang-xml` emits no relations by design (see this crate's
    // `parse.rs::no_relations_are_ever_emitted`); assert that contract still
    // holds *through the index*, not only at the parser boundary.
    for name in ["api", "health", "metrics", "worker", "jobs", "settings"] {
        assert!(
            index.find_references(name).unwrap().is_empty(),
            "xml must emit no relations, but {name} has some"
        );
        assert!(index.find_calls(name).unwrap().is_empty(), "{name}");
        assert!(index.find_callers(name).unwrap().is_empty(), "{name}");
    }
}

#[test]
fn elements_stay_scoped_to_their_own_file_across_a_multi_file_project() {
    let index = open_services_indexed();

    // Each name resolves to exactly one element, in the one file that
    // declares it — nothing bleeds across files.
    let console = index.find_symbol("console").unwrap();
    assert_eq!(console.len(), 1, "{console:?}");
    assert_eq!(console[0].relative_path, "logging.xml");
    assert_eq!(console[0].kind, "element");
    assert_eq!(console[0].parent.as_deref(), Some("app-logger"));
    assert_eq!(console[0].language, "xml");

    let emails = index.find_symbol("emails").unwrap();
    assert_eq!(emails.len(), 1, "{emails:?}");
    assert_eq!(emails[0].relative_path, "worker.xml");
    assert_eq!(emails[0].line, 4);

    let metrics = index.find_symbol("metrics").unwrap();
    assert_eq!(metrics.len(), 1, "{metrics:?}");
    assert_eq!(metrics[0].relative_path, "api.xml");
}

#[test]
fn a_second_reindex_skips_unchanged_files_and_force_reparses_them() {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();

    let first = index.reindex(&registry(), false).unwrap();
    assert_eq!(first.files_parsed, 1);

    // Incremental: nothing on disk changed, so the real grammar must not be
    // re-run at all.
    let second = index.reindex(&registry(), false).unwrap();
    assert_eq!(second.files_parsed, 0, "{second:?}");
    assert_eq!(second.files_unchanged, 1);

    // Forced: re-parses despite being unchanged, and must land on exactly
    // the same symbol set rather than duplicating rows.
    let forced = index.reindex(&registry(), true).unwrap();
    assert_eq!(forced.files_parsed, 1, "{forced:?}");
    assert!(forced.issues.is_empty(), "{:?}", forced.issues);

    let status = index.status().unwrap();
    assert_eq!(status.total_symbols, 6, "a forced reparse must not duplicate symbols");
    assert_eq!(index.find_symbol("health").unwrap().len(), 1);
}
