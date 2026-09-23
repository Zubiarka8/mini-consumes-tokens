//! Scoped lookups (`QueryScope`), the filesystem-backed file-vs-directory
//! resolution behind `list_symbols`/`path_is_directory`, and the aggregate
//! `fan_in_counts`. Covers §B1, §B4, §B5 and §C4 of `investigacion.md`.
//!
//! Uses the same toy `fn NAME calls OTHER`-per-line fake parser as
//! `reindex.rs`/`traversal.rs`, duplicated locally since Rust integration test
//! files don't share code — here parameterized by language id so a
//! `language` filter has two languages to tell apart.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use mct_index::{ExcludeSet, Index, QueryScope};

/// One line per symbol: `fn NAME` or `fn NAME calls OTHER`.
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
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if parts.next() != Some("fn") {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            }
            let name = parts.next().unwrap_or_default().to_string();
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
            });
            if parts.next() == Some("calls") {
                if let Some(callee) = parts.next() {
                    parsed.relations.push(SymbolRelation {
                        from: id,
                        kind: RelationKind::Calls,
                        to_name: callee.to_string(),
                        location: Location {
                            line: line_no as u32 + 1,
                            column: 1,
                            byte_len: line.len() as u32,
                            end_line: None,
                        },
                    });
                }
            }
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser {
        language: "fake",
        extensions: &["fake"],
    }));
    registry.register(Arc::new(FakeParser {
        language: "other",
        extensions: &["other"],
    }));
    registry
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-scope-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn open(dir: &Path) -> Index {
    let mut index = Index::open_in_memory(dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

/// Four callers of `target`, spread over two directories, two languages and a
/// dot-prefixed directory — every axis a `QueryScope` can narrow on.
fn scoped_index() -> (PathBuf, Index) {
    let dir = tempdir();
    write(&dir, "src/a.fake", "fn a calls target\n");
    write(&dir, "src/b.fake", "fn b calls target\n");
    write(&dir, "libx/c.fake", "fn c calls target\n");
    write(&dir, "libx/d.other", "fn d calls target\n");
    write(&dir, ".hidden-pkg/e.fake", "fn e calls target\n");
    write(&dir, "target_def.fake", "fn target\n");
    let index = open(&dir);
    (dir, index)
}

/// Sorted `relative_path:from_symbol` pairs, for terse assertions.
fn hits(index_hits: &[mct_index::RelationHit]) -> Vec<String> {
    let mut out: Vec<String> = index_hits
        .iter()
        .map(|h| format!("{}:{}", h.relative_path, h.from_symbol))
        .collect();
    out.sort();
    out
}

#[test]
fn indexed_relative_paths_use_forward_slashes_on_every_os() {
    // Every scope predicate below (`= path` / `LIKE path/%`) assumes this.
    let (_dir, index) = scoped_index();
    let all = hits(&index.find_callers("target").unwrap());
    assert!(
        all.contains(&"src/a.fake:a".to_string()),
        "unexpected path spelling: {all:?}"
    );
}

#[test]
fn scope_narrows_by_directory_prefix() {
    let (_dir, index) = scoped_index();
    let scoped = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: Some("src"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec!["src/a.fake:a", "src/b.fake:b"]);
}

#[test]
fn scope_narrows_by_exact_file() {
    let (_dir, index) = scoped_index();
    let scoped = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: Some("src/a.fake"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec!["src/a.fake:a"]);
}

#[test]
fn scope_narrows_by_language() {
    let (_dir, index) = scoped_index();
    let scoped = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: None,
                language: Some("other"),
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec!["libx/d.other:d"]);
}

#[test]
fn scope_combines_path_and_language_with_and() {
    let (_dir, index) = scoped_index();
    let both = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: Some("libx"),
                language: Some("other"),
            },
        )
        .unwrap();
    assert_eq!(hits(&both), vec!["libx/d.other:d"]);

    // Same directory, the other language: the two predicates are ANDed, not ORed.
    let contradictory = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: Some("src"),
                language: Some("other"),
            },
        )
        .unwrap();
    assert!(contradictory.is_empty(), "{contradictory:?}");
}

#[test]
fn scope_applies_to_find_symbol_and_find_references_and_find_calls_too() {
    let (_dir, index) = scoped_index();
    let defs = index
        .find_symbol_scoped(
            "a",
            QueryScope {
                path: Some("src"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].relative_path, "src/a.fake");

    let out_of_scope = index
        .find_symbol_scoped(
            "a",
            QueryScope {
                path: Some("libx"),
                language: None,
            },
        )
        .unwrap();
    assert!(out_of_scope.is_empty(), "{out_of_scope:?}");

    let refs = index
        .find_references_scoped(
            "target",
            QueryScope {
                path: Some(".hidden-pkg"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(hits(&refs), vec![".hidden-pkg/e.fake:e"]);

    let calls = index
        .find_calls_scoped(
            "a",
            QueryScope {
                path: Some("libx"),
                language: None,
            },
        )
        .unwrap();
    assert!(
        calls.is_empty(),
        "`a` is defined in src/, not libx/: {calls:?}"
    );
}

/// The compatibility guarantee: an empty scope must reproduce the unscoped
/// method exactly, since 228 existing call sites depend on it.
#[test]
fn default_scope_returns_exactly_what_the_unscoped_method_returns() {
    let (_dir, index) = scoped_index();
    let empty = QueryScope::default();
    assert!(empty.is_empty());

    let debug = |v: &[mct_index::RelationHit]| format!("{v:?}");

    assert_eq!(
        debug(&index.find_callers("target").unwrap()),
        debug(&index.find_callers_scoped("target", empty).unwrap())
    );
    assert_eq!(
        debug(&index.find_references("target").unwrap()),
        debug(&index.find_references_scoped("target", empty).unwrap())
    );
    assert_eq!(
        debug(&index.find_calls("a").unwrap()),
        debug(&index.find_calls_scoped("a", empty).unwrap())
    );
    assert_eq!(
        format!("{:?}", index.find_symbol("target").unwrap()),
        format!("{:?}", index.find_symbol_scoped("target", empty).unwrap())
    );

    for depth in [1u32, 3] {
        assert_eq!(
            debug(&index.find_callers_bfs("target", depth, 50, 0).unwrap()),
            debug(
                &index
                    .find_callers_bfs_scoped("target", depth, 50, 0, empty)
                    .unwrap()
            )
        );
        assert_eq!(
            debug(&index.find_calls_bfs("a", depth, 50, 0).unwrap()),
            debug(
                &index
                    .find_calls_bfs_scoped("a", depth, 50, 0, empty)
                    .unwrap()
            )
        );
        assert_eq!(
            debug(&index.find_references_bfs("target", depth, 50, 0).unwrap()),
            debug(
                &index
                    .find_references_bfs_scoped("target", depth, 50, 0, empty)
                    .unwrap()
            )
        );
    }
}

/// `a` (in `src/`) calls `b` (in `libx/`), which calls `c`. Scoped to
/// `src/`, the depth-1 hit survives and the depth-2 one must not, because the
/// symbol that *makes* the second call lives outside the scope.
fn two_hop_index() -> (PathBuf, Index) {
    let dir = tempdir();
    write(&dir, "src/a.fake", "fn a calls b\n");
    write(&dir, "libx/b.fake", "fn b calls c\n");
    write(&dir, "libx/c.fake", "fn c\n");
    let index = open(&dir);
    (dir, index)
}

#[test]
fn bfs_applies_the_scope_at_every_hop_not_just_the_first() {
    let (_dir, index) = two_hop_index();

    // Unscoped: both hops are reported.
    let unscoped = index.find_calls_bfs("a", 3, 50, 0).unwrap();
    assert_eq!(hits(&unscoped), vec!["libx/b.fake:b", "src/a.fake:a"]);

    // Scoped to src/: hop 2 is made by `b`, which lives in libx/, so it is
    // neither reported nor expanded.
    let scoped = index
        .find_calls_bfs_scoped(
            "a",
            3,
            50,
            0,
            QueryScope {
                path: Some("src"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec!["src/a.fake:a"]);
    assert_eq!(scoped[0].depth, 1);
}

#[test]
fn bfs_scope_by_language_also_holds_at_every_hop() {
    let dir = tempdir();
    write(&dir, "src/a.fake", "fn a calls b\n");
    write(&dir, "src/b.other", "fn b calls c\n");
    write(&dir, "src/c.fake", "fn c\n");
    let index = open(&dir);

    assert_eq!(index.find_calls_bfs("a", 3, 50, 0).unwrap().len(), 2);

    let scoped = index
        .find_calls_bfs_scoped(
            "a",
            3,
            50,
            0,
            QueryScope {
                path: None,
                language: Some("fake"),
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec!["src/a.fake:a"]);
}

#[test]
fn path_is_directory_uses_the_filesystem_not_the_dot_heuristic() {
    let (dir, index) = scoped_index();

    // A dot-prefixed directory: the old `contains('.')` heuristic called this
    // a file and silently matched nothing (§B5).
    assert!(index.path_is_directory(".hidden-pkg"));
    assert!(index.path_is_directory("src"));

    // A dotted file is still a file.
    assert!(!index.path_is_directory("src/a.fake"));
    assert!(!index.path_is_directory("target_def.fake"));

    // A dotted directory with no leading dot, e.g. a version-named folder.
    fs::create_dir_all(dir.join("v1.2")).unwrap();
    assert!(index.path_is_directory("v1.2"));

    // A path that does not exist is not a directory.
    assert!(!index.path_is_directory("nope"));
}

#[test]
fn list_symbols_on_a_dot_prefixed_directory_returns_its_symbols() {
    let (_dir, index) = scoped_index();
    let entries = index.list_symbols(".hidden-pkg", None, None).unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].name, "e");
    assert_eq!(entries[0].relative_path, ".hidden-pkg/e.fake");
}

#[test]
fn list_symbols_on_a_dotted_directory_created_after_indexing_still_matches_it() {
    let dir = tempdir();
    write(&dir, "v1.2/a.fake", "fn a\n");
    let index = open(&dir);
    let entries = index.list_symbols("v1.2", None, None).unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].relative_path, "v1.2/a.fake");
}

#[test]
fn list_symbols_falls_back_to_the_string_heuristic_for_a_deleted_path() {
    let dir = tempdir();
    write(&dir, "src/a.fake", "fn a\n");
    let index = open(&dir);

    // The file is gone from disk but still indexed (no reindex since): the
    // filesystem can't classify it, so the last-segment heuristic decides,
    // exactly as it always did.
    fs::remove_file(dir.join("src").join("a.fake")).unwrap();
    let entries = index.list_symbols("src/a.fake", None, None).unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].name, "a");
}

#[test]
fn scope_resolves_a_dot_prefixed_directory_as_a_prefix_too() {
    let (_dir, index) = scoped_index();
    let scoped = index
        .find_callers_scoped(
            "target",
            QueryScope {
                path: Some(".hidden-pkg"),
                language: None,
            },
        )
        .unwrap();
    assert_eq!(hits(&scoped), vec![".hidden-pkg/e.fake:e"]);
}

#[test]
fn fan_in_counts_matches_find_callers_len_for_every_name() {
    let dir = tempdir();
    write(&dir, "src/a.fake", "fn a calls target\n");
    write(&dir, "src/b.fake", "fn b calls target\n");
    write(&dir, "libx/c.fake", "fn c calls target\nfn c2 calls a\n");
    write(&dir, "libx/d.other", "fn d calls a\n");
    write(&dir, "lonely.fake", "fn lonely\n");
    let index = open(&dir);

    let counts = index.fan_in_counts().unwrap();
    for name in ["target", "a", "b", "c", "d", "lonely"] {
        let expected = index.find_callers(name).unwrap().len();
        let actual = counts.get(name).copied().unwrap_or(0);
        assert_eq!(actual, expected, "fan-in mismatch for `{name}`");
    }
    assert_eq!(counts.get("target").copied(), Some(3));
    assert_eq!(counts.get("a").copied(), Some(2));
    assert_eq!(counts.get("lonely").copied(), None);

    // Nothing that is not a `calls` relation leaks in, and no name is missing.
    let named: Vec<&String> = counts.keys().collect();
    assert_eq!(named.len(), 2, "{counts:?}");
}
