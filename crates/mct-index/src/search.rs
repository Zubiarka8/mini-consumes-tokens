//! Lexical symbol search: identifier word-splitting plus a BM25-ranked FTS5
//! match over the split words, so `parse request` (or `parseRequest`, or
//! `parse_req`) finds `parseRequestBody`/`parse_request_body` — names
//! `find_symbol`'s exact/prefix/fuzzy modes all miss.
//!
//! The split words live in the `symbol_words_fts` table (see `schema.rs`),
//! kept in sync with `symbols` by triggers that call the `mct_split_words`
//! SQL function this module registers on every connection *before*
//! migrations run — the migration's backfill and every later write go
//! through the exact same [`split_identifier`] this module queries with.

use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;

use crate::queries::{push_scope, BoundValues, ResolvedScope, SymbolHit};
use crate::Result;

/// Name of the scalar SQL function the `symbol_words_fts` triggers call.
pub(crate) const SPLIT_WORDS_FUNCTION: &str = "mct_split_words";

/// Upper bound on the terms taken from one query — every term is one more
/// FTS5 prefix scan, and no real identifier search needs more than a few.
const MAX_QUERY_TERMS: usize = 16;

/// Splits an identifier into lowercase words at camelCase, PascalCase,
/// snake_case, kebab-case, whitespace/punctuation and acronym boundaries:
/// `parseRequestBody` → `parse request body`, `HTTPServer` → `http server`,
/// `IOError` → `io error`, `HTTP2Server` → `http2 server`. Digits stay
/// attached to the word they follow (`base64Encode` → `base64 encode`).
/// Pure and total: any input, including an empty or all-punctuation one,
/// yields a (possibly empty) list, never a panic.
pub fn split_identifier(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            flush(&mut current, &mut words);
            continue;
        }
        if c.is_uppercase() && !current.is_empty() {
            let prev = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
            let next = chars.get(i + 1).copied();
            let after_lower_or_digit = prev.is_some_and(|p| p.is_lowercase() || p.is_numeric());
            let acronym_end =
                prev.is_some_and(char::is_uppercase) && next.is_some_and(char::is_lowercase);
            if after_lower_or_digit || acronym_end {
                flush(&mut current, &mut words);
            }
        }
        current.extend(c.to_lowercase());
    }
    flush(&mut current, &mut words);
    words
}

fn flush(current: &mut String, words: &mut Vec<String>) {
    if !current.is_empty() {
        words.push(std::mem::take(current));
    }
}

/// What gets indexed for `name`: its split words, plus — when it splits into
/// more than one — the words glued back together, so a query typed as one
/// run-together token (`httpserver`, `parserequ`) still prefix-matches.
pub fn search_words(name: &str) -> String {
    let words = split_identifier(name);
    let mut out = words.join(" ");
    if words.len() > 1 {
        out.push(' ');
        out.push_str(&words.concat());
    }
    out
}

/// Registers [`SPLIT_WORDS_FUNCTION`] on `conn`. Must run before
/// `schema::migrations()` (the migration backfills through it) and on every
/// connection that writes `symbols` (the sync triggers call it).
pub(crate) fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        SPLIT_WORDS_FUNCTION,
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let name: Option<String> = ctx.get(0)?;
            Ok(search_words(name.as_deref().unwrap_or_default()))
        },
    )
}

/// Builds the FTS5 `MATCH` expression for `query`, or `None` when it holds no
/// searchable word. Every term comes out of [`split_identifier`] (so it is
/// purely alphanumeric) and is still quoted as an FTS5 phrase — user input
/// can never inject an FTS5 operator (`NEAR`, `OR`, `:`, `^`, `(`...), and a
/// malformed query is simply one with fewer terms, never a syntax error.
fn fts_expression(query: &str, joiner: &str) -> Option<String> {
    let terms: Vec<String> = split_identifier(query)
        .into_iter()
        .take(MAX_QUERY_TERMS)
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(joiner))
}

/// How well a hit's name matches the query, before BM25 is consulted:
/// lower is better. Guarantees an exact name always ranks first.
fn match_tier(name: &str, query_lower: &str, query_compact: &str) -> u8 {
    let name_lower = name.to_lowercase();
    let name_compact = split_identifier(name).concat();
    if name_lower == query_lower {
        0
    } else if !query_compact.is_empty() && name_compact == query_compact {
        1
    } else if name_lower.starts_with(query_lower)
        || (!query_compact.is_empty() && name_compact.starts_with(query_compact))
    {
        2
    } else {
        3
    }
}

/// Every symbol whose split name matches `query`, narrowed to `scope`, best
/// match first: exact (case-insensitive) name, then the same words in any
/// casing/separator style, then a name prefix, then the rest — each tier
/// ordered by BM25, ties broken by path and line. Every query word must
/// prefix-match one of the name's words; when that finds nothing and the
/// query has several words, any-word matching is tried instead.
pub fn search_symbols(
    conn: &Connection,
    query: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<SymbolHit>> {
    let Some(all_terms) = fts_expression(query, " ") else {
        return Ok(Vec::new());
    };
    let mut ranked = run_match(conn, &all_terms, scope)?;
    if ranked.is_empty() {
        if let Some(any_term) = fts_expression(query, " OR ").filter(|e| *e != all_terms) {
            ranked = run_match(conn, &any_term, scope)?;
        }
    }

    let query_lower = query.trim().to_lowercase();
    let query_compact = split_identifier(query).concat();
    let mut tiered: Vec<(u8, f64, SymbolHit)> = ranked
        .into_iter()
        .map(|(score, hit)| {
            (
                match_tier(&hit.name, &query_lower, &query_compact),
                score,
                hit,
            )
        })
        .collect();
    tiered.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.2.relative_path.cmp(&b.2.relative_path))
            .then_with(|| a.2.line.cmp(&b.2.line))
    });
    Ok(tiered.into_iter().map(|(_, _, hit)| hit).collect())
}

fn run_match(
    conn: &Connection,
    fts_query: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<(f64, SymbolHit)>> {
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level,
                bm25(symbol_words_fts)
         FROM symbol_words_fts
         JOIN symbols s ON s.id = symbol_words_fts.rowid
         JOIN files f ON f.id = s.file_id
         WHERE symbol_words_fts MATCH ?1",
    );
    let mut bound: BoundValues = vec![Box::new(fts_query.to_string())];
    push_scope(&mut sql, &mut bound, scope);

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, f64>(9)?,
                SymbolHit {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    language: row.get(2)?,
                    relative_path: row.get(3)?,
                    line: row.get(4)?,
                    column: row.get(5)?,
                    parent: row.get(6)?,
                    end_line: row.get(7)?,
                    level: row.get(8)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(name: &str) -> String {
        split_identifier(name).join(" ")
    }

    #[test]
    fn splits_every_identifier_convention() {
        assert_eq!(split("parseRequestBody"), "parse request body");
        assert_eq!(split("ParseRequestBody"), "parse request body");
        assert_eq!(split("parse_request_body"), "parse request body");
        assert_eq!(split("parse-request-body"), "parse request body");
        assert_eq!(split("PARSE_REQUEST_BODY"), "parse request body");
        assert_eq!(split("parse request  body"), "parse request body");
    }

    #[test]
    fn splits_acronyms_and_digits() {
        assert_eq!(split("HTTPServer"), "http server");
        assert_eq!(split("IOError"), "io error");
        assert_eq!(split("getHTTPResponse"), "get http response");
        assert_eq!(split("HTTP2Server"), "http2 server");
        assert_eq!(split("base64Encode"), "base64 encode");
        assert_eq!(split("utf8"), "utf8");
    }

    #[test]
    fn degenerate_input_yields_no_words_not_a_panic() {
        assert!(split_identifier("").is_empty());
        assert!(split_identifier("__").is_empty());
        assert!(split_identifier("+=").is_empty());
        assert_eq!(split("ÄrgerÖl"), "ärger öl");
    }

    #[test]
    fn indexed_words_include_the_glued_form_only_for_multi_word_names() {
        assert_eq!(search_words("HTTPServer"), "http server httpserver");
        assert_eq!(search_words("main"), "main");
        assert_eq!(search_words(""), "");
    }

    #[test]
    fn fts_expression_neutralises_operators() {
        assert_eq!(
            fts_expression("a OR b NEAR(c) \"d\" e:f ^g *", " ").as_deref(),
            Some("\"a\"* \"or\"* \"b\"* \"near\"* \"c\"* \"d\"* \"e\"* \"f\"* \"g\"*")
        );
        assert_eq!(fts_expression("\"*():^", " "), None);
    }

    #[test]
    fn tiers_put_exact_before_compact_before_prefix() {
        let q = "parserequest";
        assert_eq!(match_tier("ParseRequest", q, "parserequest"), 0);
        assert_eq!(
            match_tier("parse_request", "parse request", "parserequest"),
            1
        );
        assert_eq!(
            match_tier("parse_request_body", "parse request", "parserequest"),
            2
        );
        assert_eq!(
            match_tier("request_parser", "parse request", "parserequest"),
            3
        );
    }
}
