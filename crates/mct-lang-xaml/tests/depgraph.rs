//! File-level `Imports` evidence: a merged `ResourceDictionary` names the
//! `.xaml` file it pulls in, pinned to its path. Event handlers keep their
//! code-behind evidence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageRegistry, RelationKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_xaml::XamlParser;

#[test]
fn a_merged_dictionary_resolves_to_its_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/depgraph-fixture");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(XamlParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let hits: Vec<_> = index
        .find_references("Colors")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path == "App.xaml")
        .collect();
    // `Colors.xaml` also exists at the root; the relative path decides.
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].resolution, Resolution::Resolved);
    assert_eq!(hits[0].candidates[0].relative_path, "Styles/Colors.xaml");
}

#[test]
fn an_assembly_resource_uri_records_no_import() {
    use mct_core::{LanguageParser, SourceFile};
    let parsed = XamlParser
        .parse(&SourceFile {
            relative_path: "App.xaml".into(),
            contents: "<ResourceDictionary><ResourceDictionary Source=\"/Other;component/Theme.xaml\"/></ResourceDictionary>".into(),
        })
        .unwrap();
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Imports));
}
