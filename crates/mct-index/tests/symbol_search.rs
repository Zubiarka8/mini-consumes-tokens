//! `Index::search_symbols`: the BM25-ranked FTS5 search over each symbol's
//! name split into words (issue #58). Covers recall that `find_symbol`'s
//! exact/prefix/fuzzy modes miss, ranking (an exact name always first), and
//! keeping `symbol_words_fts` in sync as files are added, modified and
//! removed across reindexes.
//!
//! Uses the same toy `fn NAME`-per-line fake parser as `symbol_match.rs`,
//! duplicated locally since Rust integration test files don't share code.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index, QueryScope, SymbolMatchMode};

struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
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
                level: None,
            });
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-symbol-search-test-{}", uuid_like()));
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

fn names(index: &Index, query: &str) -> Vec<String> {
    index
        .search_symbols(query, QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|hit| hit.name)
        .collect()
}

fn indexed(files: &[(&str, &str)]) -> (std::path::PathBuf, Index) {
    let dir = tempdir();
    for (path, contents) in files {
        fs::write(dir.join(path), contents).unwrap();
    }
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    (dir, index)
}

fn fixture() -> (std::path::PathBuf, Index) {
    indexed(&[
        ("a.fake", "fn parseRequestBody\nfn parse_request\n"),
        ("b.fake", "fn ParseRequest\nfn request_parser\n"),
        ("c.fake", "fn HTTPServer\nfn serve_http_request\n"),
        ("d.fake", "fn unrelated\n"),
    ])
}

#[test]
fn split_word_queries_find_symbols_find_symbol_misses() {
    let (_dir, index) = fixture();
    for mode in [
        SymbolMatchMode::Exact,
        SymbolMatchMode::Prefix,
        SymbolMatchMode::Fuzzy,
    ] {
        assert!(
            index
                .find_symbol_matching("parse request body", mode)
                .unwrap()
                .is_empty(),
            "{mode:?} unexpectedly matched a space-separated query"
        );
    }
    assert_eq!(names(&index, "parse request body"), ["parseRequestBody"]);
    assert_eq!(names(&index, "parse_request_body"), ["parseRequestBody"]);
    assert_eq!(names(&index, "http server"), ["HTTPServer"]);
}

#[test]
fn partial_words_prefix_match_each_name_word() {
    let (_dir, index) = fixture();
    let hits = names(&index, "pars req");
    for expected in [
        "parseRequestBody",
        "parse_request",
        "ParseRequest",
        "request_parser",
    ] {
        assert!(
            hits.contains(&expected.to_string()),
            "{expected} missing from {hits:?}"
        );
    }
    assert!(!hits.contains(&"unrelated".to_string()));
    // A run-together query still reaches a multi-word name via its glued form.
    assert_eq!(names(&index, "httpserv"), ["HTTPServer"]);
}

#[test]
fn an_exact_case_insensitive_name_ranks_first() {
    let (_dir, index) = fixture();
    assert_eq!(
        names(&index, "ParseRequest").first().map(String::as_str),
        Some("ParseRequest")
    );
    assert_eq!(
        names(&index, "parse_request").first().map(String::as_str),
        Some("parse_request")
    );
    assert_eq!(
        names(&index, "parserequest").first().map(String::as_str),
        Some("ParseRequest")
    );
}

#[test]
fn same_words_in_another_style_rank_before_a_mere_superset() {
    let (_dir, index) = fixture();
    let hits = names(&index, "parse request");
    let pos = |n: &str| hits.iter().position(|h| h == n).unwrap();
    assert!(pos("parse_request") < pos("parseRequestBody"));
    assert!(pos("ParseRequest") < pos("parseRequestBody"));
    assert!(pos("parseRequestBody") < pos("request_parser"));
}

#[test]
fn all_words_first_falls_back_to_any_word() {
    let (_dir, index) = fixture();
    // No name holds both words, so any-word matching kicks in.
    let hits = names(&index, "unrelated server");
    assert!(hits.contains(&"unrelated".to_string()));
    assert!(hits.contains(&"HTTPServer".to_string()));
}

#[test]
fn malformed_or_operator_laden_queries_never_error() {
    let (_dir, index) = fixture();
    for query in [
        "",
        "   ",
        "\"",
        "*",
        "(",
        "NEAR(",
        "a OR",
        "parse:request",
        "^http",
        "--",
        "\u{0}",
    ] {
        index
            .search_symbols(query, QueryScope::default())
            .unwrap_or_else(|e| panic!("query {query:?} errored: {e}"));
    }
    assert!(names(&index, "\"*()").is_empty());
    assert_eq!(
        names(&index, "parse OR body").first().map(String::as_str),
        Some("parseRequestBody")
    );
}

#[test]
fn scope_narrows_by_path_and_language() {
    let (_dir, index) = fixture();
    let scoped = index
        .search_symbols(
            "parse request",
            QueryScope {
                path: Some("b.fake"),
                language: None,
            },
        )
        .unwrap();
    assert!(scoped.iter().all(|h| h.relative_path == "b.fake"));
    assert!(!scoped.is_empty());
    let other_language = index
        .search_symbols(
            "parse request",
            QueryScope {
                path: None,
                language: Some("rust"),
            },
        )
        .unwrap();
    assert!(other_language.is_empty());
}

#[test]
fn fts_stays_in_sync_across_add_modify_and_remove() {
    let (dir, mut index) = indexed(&[("a.fake", "fn parseRequestBody\n")]);
    assert_eq!(names(&index, "request body"), ["parseRequestBody"]);

    // Add a file.
    fs::write(dir.join("b.fake"), "fn renderResponseBody\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(names(&index, "response body"), ["renderResponseBody"]);

    // Modify: the old name must disappear, the new one appear.
    fs::write(dir.join("a.fake"), "fn decodeRequestHeader\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(names(&index, "parse").is_empty());
    assert_eq!(names(&index, "request header"), ["decodeRequestHeader"]);

    // Remove a file.
    fs::remove_file(dir.join("b.fake")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(names(&index, "render response").is_empty());

    // A forced full reindex leaves no duplicate rows behind.
    index.reindex(&registry(), true).unwrap();
    assert_eq!(names(&index, "request header"), ["decodeRequestHeader"]);
}

#[test]
fn an_on_disk_index_is_searchable_after_reopening() {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn parseRequestBody\n").unwrap();
    let db = dir.join(".mct-index").join("index.sqlite3");
    {
        let mut index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
        index.reindex(&registry(), false).unwrap();
    }
    let index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
    assert_eq!(names(&index, "request body"), ["parseRequestBody"]);
}
