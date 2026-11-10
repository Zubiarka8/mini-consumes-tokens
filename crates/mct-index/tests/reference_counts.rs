//! Reference counts by resolution (issue #322, D7): a relation resolved to
//! one symbol counts only for that symbol, an external relation counts for
//! none, and an ambiguous or unresolved relation still counts for every
//! same-named symbol, so dead code stays conservative.
//!
//! Uses a toy parser, one symbol per line:
//! `def NAME [calls TARGET [q=QUALIFIER] [path=PATH] [ext]]`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind,
    SymbolRecord, SymbolRelation,
};
use mct_index::{find_dead_code_candidates, ExcludeSet, Index};

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
            let Some(at) = rest.iter().position(|w| *w == "calls") else {
                continue;
            };
            let mut target = RelationTarget {
                relation: parsed.relations.len(),
                ..Default::default()
            };
            for word in &rest[at + 2..] {
                match word.split_once('=') {
                    Some(("q", v)) => target.qualifier = Some(v.to_string()),
                    Some(("path", v)) => target.path = Some(v.to_string()),
                    _ if *word == "ext" => target.external = true,
                    _ => {}
                }
            }
            parsed.relation_targets.push(target);
            parsed.relations.push(SymbolRelation {
                from: id,
                kind: RelationKind::Calls,
                to_name: rest[at + 1].to_string(),
                location: Location {
                    end_line: None,
                    ..location
                },
            });
        }
        Ok(parsed)
    }
}

fn index_of(files: &[(&str, &str)]) -> (PathBuf, Index) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-index-reference-counts-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (path, contents) in files {
        fs::write(dir.join(path), contents).unwrap();
    }
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    (dir, index)
}

/// `path:name` of every dead-code candidate.
fn dead(index: &Index) -> Vec<String> {
    let mut out: Vec<String> = find_dead_code_candidates(index, None, None)
        .unwrap()
        .into_iter()
        .map(|e| format!("{}:{}", e.relative_path, e.name))
        .collect();
    out.sort();
    out
}

#[test]
fn a_call_resolved_to_one_symbol_does_not_keep_its_homonym_alive() {
    let (dir, index) = index_of(&[
        ("x.a", "def helper\n"),
        ("y.a", "def helper\n"),
        ("z.a", "def main calls helper path=x.a\n"),
    ]);
    assert_eq!(dead(&index), ["y.a:helper"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_external_call_keeps_no_indexed_symbol_alive() {
    let (dir, index) = index_of(&[("o.a", "def open\n"), ("u.a", "def main calls open ext\n")]);
    assert_eq!(dead(&index), ["o.a:open"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_ambiguous_call_keeps_every_same_named_symbol_alive() {
    let (dir, index) = index_of(&[
        ("p.a", "def run\n"),
        ("q.a", "def run\n"),
        ("r.a", "def main calls run\n"),
    ]);
    assert!(dead(&index).is_empty(), "{:?}", dead(&index));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_unresolved_call_keeps_every_same_named_symbol_alive() {
    // The qualifier matches no parent: unresolved, yet the name may still be
    // what the call reaches (an inherited or extension method).
    let (dir, index) = index_of(&[
        ("l.a", "def lonely\n"),
        ("m.a", "def main calls lonely q=Nope\n"),
    ]);
    assert!(dead(&index).is_empty(), "{:?}", dead(&index));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_call_resolved_to_its_own_caller_is_recursion_and_does_not_count() {
    let (dir, index) = index_of(&[("s.a", "def spin calls spin path=s.a\n")]);
    assert_eq!(dead(&index), ["s.a:spin"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_call_resolved_to_its_own_caller_does_not_count_even_with_a_homonym() {
    let (dir, index) = index_of(&[
        ("s.a", "def spin calls spin path=s.a\n"),
        ("t.a", "def spin\n"),
    ]);
    assert_eq!(dead(&index), ["s.a:spin", "t.a:spin"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn one_unresolved_call_keeps_every_homonym_alive_beside_a_resolved_one() {
    let (dir, index) = index_of(&[
        ("x.a", "def helper\n"),
        ("y.a", "def helper\n"),
        (
            "z.a",
            "def main calls helper path=x.a\ndef other calls helper\n",
        ),
    ]);
    // `other` itself has no caller; both `helper`s are kept by its call.
    assert_eq!(dead(&index), ["z.a:other"]);
    fs::remove_dir_all(dir).unwrap();
}
