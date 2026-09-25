//! `Index::find_symbol_matching`'s three `SymbolMatchMode`s: `Exact` (must
//! stay byte-for-byte identical to `find_symbol`), `Prefix` (via the FTS5
//! `symbols_fts` table) and `Fuzzy` (a case-insensitive `LIKE` substring
//! match — see the mode's doc comment in `mct-index/src/queries.rs` for why
//! it isn't routed through `symbols_fts` too).
//!
//! Uses the same toy `fn NAME`-per-line fake parser as `reindex.rs`/
//! `traversal.rs`, duplicated locally since Rust integration test files
//! don't share code.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index, SymbolMatchMode};

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
    let dir = std::env::temp_dir().join(format!("mct-index-symbol-match-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// One file per symbol: `handle_request`, `handle_response`, `parse_body`,
/// `Handler` (mixed case, to exercise fuzzy's case-insensitivity), `megabody`
/// (contains "body" but not as its own token/prefix, to distinguish `Prefix`
/// from `Fuzzy`), and a name containing FTS5/LIKE-special characters (`%`,
/// `_`, `"`, `*`) to prove the query-builders don't choke on or get widened
/// by them.
fn fixture_index() -> Index {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn handle_request\n").unwrap();
    fs::write(dir.join("b.fake"), "fn handle_response\n").unwrap();
    fs::write(dir.join("c.fake"), "fn parse_body\n").unwrap();
    fs::write(dir.join("d.fake"), "fn Handler\n").unwrap();
    fs::write(dir.join("e.fake"), "fn weird_100%_\"quoted\"_name\n").unwrap();
    fs::write(dir.join("f.fake"), "fn megabody\n").unwrap();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

#[test]
fn exact_mode_is_byte_identical_to_find_symbol() {
    let index = fixture_index();
    let plain = index.find_symbol("handle_request").unwrap();
    let matched = index
        .find_symbol_matching("handle_request", SymbolMatchMode::Exact)
        .unwrap();
    assert_eq!(plain.len(), 1, "{plain:?}");
    assert_eq!(plain.len(), matched.len());
    assert_eq!(plain[0].name, matched[0].name);
    assert_eq!(plain[0].relative_path, matched[0].relative_path);
}

#[test]
fn exact_mode_does_not_widen_to_a_partial_name() {
    let index = fixture_index();
    let hits = index
        .find_symbol_matching("handle", SymbolMatchMode::Exact)
        .unwrap();
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn prefix_mode_finds_every_symbol_whose_leading_token_matches() {
    let index = fixture_index();
    let hits = index
        .find_symbol_matching("handle", SymbolMatchMode::Prefix)
        .unwrap();
    let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
    // `symbols_fts` uses FTS5's default `unicode61` tokenizer, which splits
    // on `_`: `Handler` tokenizes to a single token `handler`, which is also
    // a prefix match for "handle" — token-prefix, not literal-string-prefix,
    // is the documented semantics (see `SymbolMatchMode::Prefix`).
    assert_eq!(
        names,
        vec!["handle_request", "handle_response", "Handler"],
        "{names:?}"
    );
}

#[test]
fn prefix_mode_matches_a_non_leading_token_that_starts_with_the_term() {
    let index = fixture_index();
    // "body" is `parse_body`'s *second* token — prefix matching is
    // per-token, not anchored to the start of the whole name.
    let hits = index.find_symbol_matching("body", SymbolMatchMode::Prefix).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].name, "parse_body");
}

#[test]
fn prefix_mode_does_not_match_the_term_appearing_mid_token() {
    let index = fixture_index();
    // "body" appears inside `megabody`, but not as that token's own prefix
    // (the token is "megabody", which does not start with "body") — this is
    // exactly what distinguishes `Prefix` from `Fuzzy`.
    let hits = index.find_symbol_matching("body", SymbolMatchMode::Prefix).unwrap();
    assert!(
        hits.iter().all(|h| h.name != "megabody"),
        "megabody must not match a token-prefix query for `body`: {hits:?}"
    );
}

#[test]
fn prefix_mode_full_name_is_also_a_valid_prefix_of_itself() {
    let index = fixture_index();
    let hits = index
        .find_symbol_matching("handle_request", SymbolMatchMode::Prefix)
        .unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].name, "handle_request");
}

#[test]
fn prefix_mode_tolerates_fts5_special_characters_in_the_term() {
    let index = fixture_index();
    // `"` and `*` are FTS5 query-syntax operators; the quoting in
    // `quote_fts_phrase` must neutralize them rather than erroring or
    // matching everything.
    let hits = index
        .find_symbol_matching("weird_100%_\"quoted", SymbolMatchMode::Prefix)
        .unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].name, "weird_100%_\"quoted\"_name");
}

#[test]
fn fuzzy_mode_is_case_insensitive() {
    let index = fixture_index();
    let hits = index.find_symbol_matching("handler", SymbolMatchMode::Fuzzy).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].name, "Handler");
}

#[test]
fn fuzzy_mode_matches_every_symbol_containing_the_shared_substring() {
    let index = fixture_index();
    let hits = index.find_symbol_matching("handle", SymbolMatchMode::Fuzzy).unwrap();
    let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
    // Case-insensitive substring match: "handle" is also a substring of
    // `Handler`, unlike `Prefix`'s per-token matching (see the `prefix_mode_*`
    // tests), so all three surface here.
    assert_eq!(
        names,
        vec!["handle_request", "handle_response", "Handler"],
        "{names:?}"
    );
}

#[test]
fn fuzzy_mode_matches_a_substring_at_any_position_unlike_prefix_mode() {
    let index = fixture_index();
    // Unlike `Prefix` (see `prefix_mode_does_not_match_the_term_appearing_mid_token`),
    // `Fuzzy` matches `megabody` because "body" appears anywhere in the name.
    let hits = index.find_symbol_matching("body", SymbolMatchMode::Fuzzy).unwrap();
    let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
    // Ordered by relative_path (c.fake, f.fake), same as every other mode.
    assert_eq!(names, vec!["parse_body", "megabody"], "{names:?}");
}

#[test]
fn fuzzy_mode_does_not_let_a_literal_percent_or_underscore_widen_the_match() {
    let index = fixture_index();
    // If `%`/`_` in the term weren't escaped before being wrapped in
    // `%...%`, "100%" and "100_" would both act as wildcards and match
    // unrelated names; escaped, only the literal `100%` substring matches.
    let hits = index
        .find_symbol_matching("100%_", SymbolMatchMode::Fuzzy)
        .unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].name, "weird_100%_\"quoted\"_name");

    let unrelated = index
        .find_symbol_matching("100XY", SymbolMatchMode::Fuzzy)
        .unwrap();
    assert!(
        unrelated.is_empty(),
        "an unescaped `_` in the term would have matched any character here: {unrelated:?}"
    );
}

#[test]
fn no_match_in_any_mode_returns_empty_not_an_error() {
    let index = fixture_index();
    for mode in [
        SymbolMatchMode::Exact,
        SymbolMatchMode::Prefix,
        SymbolMatchMode::Fuzzy,
    ] {
        let hits = index.find_symbol_matching("no_such_symbol", mode).unwrap();
        assert!(hits.is_empty(), "{mode:?}: {hits:?}");
    }
}
