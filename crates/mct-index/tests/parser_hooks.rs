//! The two language-neutral hooks a parser can opt into: claiming a
//! dependency manifest's file name (the manifest is then indexed as source
//! too), and [`mct_core::RootedPaths`] (scheme-prefixed relation paths rooted
//! at the nearest marker directory).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

#[path = "common/scratch_dir.rs"]
mod scratch_dir;
use scratch_dir::scratch_dir;

#[path = "common/write.rs"]
mod fixture_files;
use fixture_files::write;

use mct_core::{
    LanguageParser, LanguageRegistry, Location, ParseError, ParsedFile, RelationKind,
    RelationTarget, RootedPaths, SourceFile, SymbolKind, SymbolRecord, SymbolRelation,
};
use mct_index::{ExcludeSet, Index, UnsupportedKind};

/// Claims `requirements.txt` (a dependency manifest) and `.fake` sources.
/// Every file is a module; `name==version` declares a class `name`, and
/// `import <path>` imports `<path>`.
struct FakeParser;

impl LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["txt", "fake"]
    }

    fn rooted_paths(&self) -> Option<RootedPaths> {
        Some(RootedPaths {
            scheme: "fake://",
            marker: "root.marker",
        })
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        let at = |line: usize| Location {
            line: line as u32 + 1,
            column: 1,
            byte_len: 1,
            end_line: None,
        };
        let name = file.relative_path.rsplit('/').next().unwrap_or_default();
        parsed.symbols.push(SymbolRecord {
            id: 0,
            name: name.into(),
            kind: SymbolKind::Module,
            location: at(0),
            parent: None,
            level: None,
        });
        for (number, line) in file.contents.lines().enumerate() {
            if let Some(path) = line.strip_prefix("import ") {
                parsed.relation_targets.push(RelationTarget {
                    relation: parsed.relations.len(),
                    path: Some(path.into()),
                    kind: Some(SymbolKind::Module),
                    ..Default::default()
                });
                parsed.relations.push(SymbolRelation {
                    from: 0,
                    kind: RelationKind::Imports,
                    to_name: path.rsplit('/').next().unwrap_or(path).into(),
                    location: at(number),
                });
            } else if let Some((class, _)) = line.split_once("==") {
                parsed.symbols.push(SymbolRecord {
                    id: parsed.symbols.len() as u32,
                    name: class.into(),
                    kind: SymbolKind::Class,
                    location: at(number),
                    parent: None,
                    level: None,
                });
            }
        }
        Ok(parsed)
    }
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn open(root: &Path) -> Index {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

fn dependencies(index: &Index) -> Vec<String> {
    index
        .status()
        .unwrap()
        .dependencies
        .into_iter()
        .filter(|manifest| manifest.manifest_path == "requirements.txt")
        .flat_map(|manifest| manifest.dependencies)
        .map(|dep| dep.name)
        .collect()
}

fn classes(index: &Index, name: &str) -> usize {
    index.find_symbol(name).unwrap().len()
}

#[test]
fn a_manifest_a_parser_claims_is_also_a_source_file() {
    let root = scratch_dir("mct-hooks-manifest");
    let manifest = write(&root, "requirements.txt", "flask==3.0\n");
    let registry = registry();
    let mut index = open(&root);
    assert_eq!(dependencies(&index), ["flask"]);
    assert_eq!(classes(&index, "flask"), 1);

    // An unchanged manifest is hash-skipped like any source file.
    let again = index.reindex(&registry, false).unwrap();
    assert_eq!(again.files_parsed, 0, "{again:?}");
    assert_eq!(classes(&index, "flask"), 1);

    // An edit replaces both kinds of rows.
    fs::write(&manifest, "django==5.0\n").unwrap();
    index
        .reindex_paths(&registry, std::slice::from_ref(&manifest))
        .unwrap();
    assert_eq!(dependencies(&index), ["django"]);
    assert_eq!(classes(&index, "flask"), 0);
    assert_eq!(classes(&index, "django"), 1);

    // Deleting it drops both.
    fs::remove_file(&manifest).unwrap();
    index.reindex_paths(&registry, &[manifest]).unwrap();
    assert!(dependencies(&index).is_empty());
    assert_eq!(classes(&index, "django"), 0);

    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_utf8_in_a_claimed_manifest_is_a_syntax_error_like_any_source() {
    let root = scratch_dir("mct-hooks-utf8");
    let manifest = write(&root, "requirements.txt", "flask==3.0\n");
    let registry = registry();
    let mut index = open(&root);

    fs::write(&manifest, b"caf\xe9==1.0\n").unwrap();
    index.reindex(&registry, false).unwrap();
    let status = index.status().unwrap();
    assert_eq!(status.syntax_errors.len(), 1, "{status:?}");
    assert_eq!(status.syntax_errors[0].relative_path, "requirements.txt");
    assert_eq!(status.syntax_errors[0].kind, UnsupportedKind::SyntaxError);
    assert_eq!(classes(&index, "flask"), 0, "no stale symbols stay");
    // The dependency list is only re-read from valid UTF-8.
    assert_eq!(dependencies(&index), ["flask"]);

    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rooted_paths_start_at_the_nearest_marker_directory() {
    let root = scratch_dir("mct-hooks-rooted");
    write(&root, "lib/util.fake", "");
    write(&root, "game/root.marker", "");
    write(&root, "game/lib/util.fake", "");
    write(&root, "game/src/main.fake", "import fake://lib/util.fake\n");
    write(&root, "tool.fake", "import fake://lib/util.fake\n");
    let index = open(&root);

    for (from, to) in [
        ("game/src/main.fake", "game/lib/util.fake"),
        // No marker above it: the repository root.
        ("tool.fake", "lib/util.fake"),
    ] {
        let graph = index.dependency_graph(from, 1, 10).unwrap();
        let edges: Vec<_> = graph.edges.iter().map(|e| e.to.as_str()).collect();
        assert_eq!(edges, [to], "{from}: {graph:?}");
    }

    drop(index);
    fs::remove_dir_all(root).unwrap();
}
