//! Qualified relation resolution (issue #97): a relation is resolved only to
//! the one definition its evidence leaves, ambiguous among several, external
//! only on parser evidence, and walks never cross an unproven edge.
//!
//! Uses a toy parser, one symbol per line:
//! `def NAME [in PARENT] [calls TARGET [q=QUALIFIER] [path=PATH] [lang=LANG] [ext]]`,
//! registered for two languages (`alpha` = `.a`, `beta` = `.b`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind,
    SymbolRecord, SymbolRelation,
};
use mct_index::{ExcludeSet, Index, RelationHit, Resolution};

struct FakeParser {
    language: &'static str,
    extensions: &'static [&'static str],
}

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        self.language
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        self.extensions
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
            let parent = (rest.get(1) == Some(&"in")).then(|| rest[2].to_string());
            parsed.symbols.push(SymbolRecord {
                id,
                name: rest[0].to_string(),
                kind: if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                location,
                parent,
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
                    Some(("lang", v)) => target.language = Some(v.to_string()),
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

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser {
        language: "alpha",
        extensions: &["a"],
    }));
    registry.register(Arc::new(FakeParser {
        language: "beta",
        extensions: &["b"],
    }));
    registry
}

fn tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-index-resolution-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn index_of(files: &[(&str, &str)]) -> (PathBuf, Index) {
    let dir = tempdir();
    for (path, contents) in files {
        fs::write(dir.join(path), contents).unwrap();
    }
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    (dir, index)
}

/// The one call made by `from`.
fn call_of(index: &Index, from: &str) -> RelationHit {
    let mut hits = index.find_calls(from).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    hits.remove(0)
}

/// `path:line parent` of each candidate of `hit`, for terse assertions.
fn candidates(index: &Index, hit: &RelationHit) -> Vec<String> {
    index
        .relation_candidates(hit.relation_id)
        .unwrap()
        .iter()
        .map(|s| format!("{}:{} {:?}", s.relative_path, s.line, s.parent))
        .collect()
}

#[test]
fn a_qualified_call_resolves_to_its_type_and_an_unqualified_one_stays_ambiguous() {
    let (_dir, index) = index_of(&[(
        "a.a",
        "def run in A\n\
         def run in B\n\
         def main calls run q=A\n\
         def guess calls run\n",
    )]);

    let qualified = call_of(&index, "main");
    assert_eq!(qualified.resolution, Resolution::Resolved);
    let target = index
        .symbols_by_ids(&[qualified.target_id.unwrap()])
        .unwrap();
    assert_eq!(target[0].parent.as_deref(), Some("A"));
    assert_eq!(target[0].line, 1);
    assert_eq!(candidates(&index, &qualified), vec!["a.a:1 Some(\"A\")"]);

    // Same-file homonyms: neither is picked.
    let bare = call_of(&index, "guess");
    assert_eq!(bare.resolution, Resolution::Ambiguous);
    assert_eq!(bare.target_id, None);
    assert_eq!(bare.candidate_count, 2);
    assert_eq!(
        candidates(&index, &bare),
        vec!["a.a:1 Some(\"A\")", "a.a:2 Some(\"B\")"]
    );
}

#[test]
fn a_qualifier_matches_a_parent_written_with_generic_arguments() {
    let (_dir, index) = index_of(&[(
        "a.a",
        "def new in Stack<T>\n\
         def new in Stacked\n\
         def main calls new q=Stack\n",
    )]);
    let hit = call_of(&index, "main");
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(candidates(&index, &hit), vec!["a.a:1 Some(\"Stack<T>\")"]);
}

#[test]
fn spelling_alone_never_links_two_languages() {
    let (_dir, index) = index_of(&[
        (
            "a.a",
            "def main calls helper\ndef cross calls helper lang=beta\n",
        ),
        ("b.b", "def helper\n"),
    ]);
    let same_language = call_of(&index, "main");
    assert_eq!(same_language.resolution, Resolution::Unresolved);
    assert_eq!(same_language.candidate_count, 0);

    // Explicit evidence makes the cross-language edge.
    let explicit = call_of(&index, "cross");
    assert_eq!(explicit.resolution, Resolution::Resolved);
    assert_eq!(candidates(&index, &explicit), vec!["b.b:1 None"]);
}

#[test]
fn path_evidence_picks_the_same_file_definition() {
    let (_dir, index) = index_of(&[
        ("a.a", "def helper\ndef main calls helper path=a.a\n"),
        ("z.a", "def helper\n"),
        ("y.a", "def other calls helper\n"),
    ]);
    assert_eq!(call_of(&index, "main").resolution, Resolution::Resolved);
    assert_eq!(call_of(&index, "other").resolution, Resolution::Ambiguous);
}

#[test]
fn external_needs_evidence_and_an_unknown_name_is_only_unresolved() {
    let (_dir, index) = index_of(&[(
        "a.a",
        "def take\n\
         def proven calls take ext\n\
         def unknown calls mystery\n",
    )]);
    // External evidence wins over a same-named repository definition.
    let proven = call_of(&index, "proven");
    assert_eq!(proven.resolution, Resolution::External);
    assert!(candidates(&index, &proven).is_empty());
    assert_eq!(
        call_of(&index, "unknown").resolution,
        Resolution::Unresolved
    );
}

fn walk_index() -> (PathBuf, Index) {
    index_of(&[(
        "a.a",
        "def main calls run q=A\n\
         def run in A calls a_only\n\
         def run in B calls b_only\n\
         def a_only\n\
         def b_only\n\
         def guess calls run\n",
    )])
}

fn names(hits: &[RelationHit], pick: impl Fn(&RelationHit) -> &str) -> Vec<String> {
    hits.iter()
        .map(|h| format!("{}@{}", pick(h), h.depth))
        .collect()
}

#[test]
fn a_forward_walk_follows_only_the_resolved_definition() {
    let (_dir, index) = walk_index();
    let from_main = index.find_calls_bfs("main", 3, 50, 0).unwrap();
    assert_eq!(
        names(&from_main, |h| &h.to_name),
        vec!["run@1", "a_only@2"],
        "must never enter B::run"
    );

    // An ambiguous callee is reported, never expanded into either candidate.
    let from_guess = index.find_calls_bfs("guess", 3, 50, 0).unwrap();
    assert_eq!(names(&from_guess, |h| &h.to_name), vec!["run@1"]);
}

#[test]
fn a_backward_walk_reports_ambiguous_callers_but_continues_only_from_proven_ones() {
    let (_dir, index) = walk_index();
    let to_a = index.find_callers_bfs("a_only", 3, 50, 0).unwrap();
    assert_eq!(
        names(&to_a, |h| &h.from_symbol),
        vec!["run@1", "main@2", "guess@2"]
    );
    let guess = to_a.iter().find(|h| h.from_symbol == "guess").unwrap();
    assert_eq!(guess.resolution, Resolution::Ambiguous);

    // `main` resolved to A::run, so it is no caller of B::run's callee.
    let to_b = index.find_callers_bfs("b_only", 3, 50, 0).unwrap();
    assert_eq!(names(&to_b, |h| &h.from_symbol), vec!["run@1", "guess@2"]);
}

#[test]
fn a_walk_from_exact_rows_ignores_same_named_definitions() {
    let (_dir, index) = walk_index();
    let a_run = index
        .find_symbol("run")
        .unwrap()
        .into_iter()
        .find(|s| s.parent.as_deref() == Some("A"))
        .unwrap();
    let callees = index.find_calls_from_symbols(&[a_run.id], 3, 50).unwrap();
    assert_eq!(names(&callees, |h| &h.to_name), vec!["a_only@1"]);
    let callers = index.find_callers_of_symbols(&[a_run.id], 1, 50).unwrap();
    assert_eq!(
        names(&callers, |h| &h.from_symbol),
        vec!["main@1", "guess@1"]
    );
}

#[test]
fn reindexing_recomputes_resolution_from_the_current_symbols() {
    let (dir, mut index) = index_of(&[
        ("a.a", "def main calls helper\n"),
        ("x.a", "def helper\n"),
        ("y.a", "def helper\n"),
    ]);
    assert_eq!(call_of(&index, "main").resolution, Resolution::Ambiguous);

    fs::remove_file(dir.join("y.a")).unwrap();
    index.reindex(&registry(), false).unwrap();
    let hit = call_of(&index, "main");
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(candidates(&index, &hit), vec!["x.a:1 None"]);

    fs::remove_file(dir.join("x.a")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(call_of(&index, "main").resolution, Resolution::Unresolved);
    let _ = fs::remove_dir_all(Path::new(&dir));
}
