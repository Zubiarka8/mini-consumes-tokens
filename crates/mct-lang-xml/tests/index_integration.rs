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

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(XmlParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 1, "settings.xml");
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
fn status_reports_full_xml_coverage_with_no_relations() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let xml = status.languages.iter().find(|l| l.language == "xml").unwrap();
    assert_eq!(xml.file_count, 1);
    assert_eq!(xml.symbol_count, 6, "module root + api + health + metrics + worker + jobs");
}
