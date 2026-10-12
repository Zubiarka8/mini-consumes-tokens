//! Module-level dependency graph over resolved `imports` relations.
//!
//! Toy parser, one symbol per line: `def NAME [imports TARGET [path=PATH] [ext]]`.

// Test code: a panic here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind,
    SymbolRecord, SymbolRelation,
};
use mct_index::{DependencyEdge, ExcludeSet, Index};

struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "alpha"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["a"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some((&"def", rest)) = words.split_first() else {
                continue;
            };
            let location = Location {
                line: line_no as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: Some(line_no as u32 + 1),
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: rest[0].to_string(),
                kind: SymbolKind::Function,
                location,
                parent: None,
                level: None,
            });
            if rest.get(1) != Some(&"imports") {
                continue;
            }
            let mut target = RelationTarget {
                relation: parsed.relations.len(),
                ..Default::default()
            };
            for word in &rest[3..] {
                match word.split_once('=') {
                    Some(("path", v)) => target.path = Some(v.to_string()),
                    _ if *word == "ext" => target.external = true,
                    _ => {}
                }
            }
            parsed.relation_targets.push(target);
            parsed.relations.push(SymbolRelation {
                from: id,
                kind: RelationKind::Imports,
                to_name: rest[2].to_string(),
                location: Location {
                    end_line: None,
                    ..location
                },
            });
        }
        Ok(parsed)
    }
}

fn index_of(files: &[(&str, &str)]) -> Index {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "mct-index-depgraph-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    for (path, contents) in files {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    index
}

fn edge(from: &str, to: &str, depth: u32) -> DependencyEdge {
    DependencyEdge {
        from: from.into(),
        to: to.into(),
        depth,
    }
}

/// a -> b -> c, with c defining `leaf`.
fn chain() -> Index {
    index_of(&[
        ("a.a", "def fa imports fb path=b.a\n"),
        ("b.a", "def fb imports fc path=c.a\n"),
        ("c.a", "def fc\n"),
    ])
}

#[test]
fn depth_one_stops_at_direct_imports() {
    let g = chain().dependency_graph("a.a", 1, 50).unwrap();
    assert_eq!(g.root, "a.a");
    assert_eq!(g.nodes, vec!["a.a", "b.a"]);
    assert_eq!(g.edges, vec![edge("a.a", "b.a", 1)]);
    assert!(!g.truncated);
}

#[test]
fn depth_two_follows_the_chain_and_orders_deterministically() {
    let g = chain().dependency_graph("a.a", 2, 50).unwrap();
    assert_eq!(g.nodes, vec!["a.a", "b.a", "c.a"]);
    assert_eq!(g.edges, vec![edge("a.a", "b.a", 1), edge("b.a", "c.a", 2)]);
    // depth 0 clamps to 1
    assert_eq!(
        chain().dependency_graph("a.a", 0, 50).unwrap().nodes.len(),
        2
    );
}

#[test]
fn a_cycle_keeps_its_back_edge_and_visits_each_file_once() {
    let index = index_of(&[
        ("a.a", "def fa imports fb path=b.a\n"),
        ("b.a", "def fb imports fa path=a.a\n"),
    ]);
    let g = index.dependency_graph("a.a", 5, 50).unwrap();
    assert_eq!(g.nodes, vec!["a.a", "b.a"]);
    assert_eq!(g.edges, vec![edge("a.a", "b.a", 1), edge("b.a", "a.a", 2)]);
}

#[test]
fn self_and_same_file_imports_are_dropped_and_unresolved_ones_are_external() {
    let index = index_of(&[(
        "a.a",
        "def helper\n\
             def fa imports helper path=a.a\n\
             def fb imports serde ext\n\
             def fc imports missing\n\
             def fd imports serde ext\n",
    )]);
    let g = index.dependency_graph("a.a", 3, 50).unwrap();
    assert_eq!(g.nodes, vec!["a.a"]);
    assert!(g.edges.is_empty());
    assert_eq!(g.external, vec!["missing", "serde"]);
}

#[test]
fn limit_caps_nodes_and_flags_truncation() {
    let index = index_of(&[
        (
            "a.a",
            "def f1 imports fb path=b.a\ndef f2 imports fc path=c.a\n",
        ),
        ("b.a", "def fb\n"),
        ("c.a", "def fc\n"),
    ]);
    let g = index.dependency_graph("a.a", 2, 2).unwrap();
    assert_eq!(g.nodes, vec!["a.a", "b.a"]);
    assert_eq!(g.edges, vec![edge("a.a", "b.a", 1)]);
    assert!(g.truncated);
    let full = index.dependency_graph("a.a", 2, 3).unwrap();
    assert!(!full.truncated);
    assert_eq!(full.nodes.len(), 3);
}

#[test]
fn a_directory_prefix_is_the_union_of_its_files() {
    let index = index_of(&[
        (
            "pkg/x.a",
            "def fx imports fy path=pkg/y.a\ndef gx imports fo path=other.a\n",
        ),
        ("pkg/y.a", "def fy imports fx path=pkg/x.a\n"),
        ("other.a", "def fo imports fy path=pkg/y.a\n"),
    ]);
    let g = index.dependency_graph("pkg/", 3, 50).unwrap();
    assert_eq!(g.root, "pkg");
    assert_eq!(g.nodes, vec!["pkg", "other.a"]);
    // internal pkg edges are dropped; other.a imports back into the directory
    assert_eq!(
        g.edges,
        vec![edge("pkg", "other.a", 1), edge("other.a", "pkg", 2)]
    );
}

#[test]
fn an_unknown_module_yields_an_empty_graph_naming_the_root() {
    let g = chain().dependency_graph("nope.a", 3, 50).unwrap();
    assert_eq!(g.root, "nope.a");
    assert!(g.nodes.is_empty() && g.edges.is_empty() && g.external.is_empty());
    assert!(!g.truncated);
}
