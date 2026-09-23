use std::collections::HashMap;

use rusqlite::Connection;

use crate::Result;

#[derive(Debug, Clone)]
pub struct SymbolHit {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub relative_path: String,
    pub line: u32,
    pub column: u32,
    pub parent: Option<String>,
    /// 1-based end line of the symbol's node, when the parser that produced
    /// it populates `Location::end_line`. `None` for rows indexed before
    /// that column existed and not yet re-indexed, or for a parser that
    /// hasn't been updated to populate it.
    pub end_line: Option<u32>,
}

/// One entry in a [`list_symbols`] result: a symbol definition's name, kind,
/// location and enclosing file/language — everything needed to discover
/// "what does this file/crate contain" without already knowing a name.
#[derive(Debug, Clone)]
pub struct SymbolListEntry {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub relative_path: String,
    pub line: u32,
    pub end_line: Option<u32>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RelationHit {
    pub kind: String,
    /// Name of the symbol that defines the relation (e.g. the caller for
    /// `find_calls`, the callee for `find_callers`).
    pub from_symbol: String,
    pub to_name: String,
    pub language: String,
    pub relative_path: String,
    pub line: u32,
    pub column: u32,
    /// How many relation-graph hops this hit is from the originally queried
    /// symbol. Every hit produced by a single-hop query in this module is a
    /// direct relation, so `1`; a multi-hop traversal (see `crate::traversal`)
    /// overwrites this with the hop number at which it found the hit.
    pub depth: u32,
}

/// Optional narrowing applied to a lookup: `path` is matched exactly when it
/// names a file and as a directory prefix otherwise (same semantics as
/// `list_symbols`), `language` is an exact match on the file's language id.
#[derive(Debug, Clone, Copy, Default)]
pub struct QueryScope<'a> {
    pub path: Option<&'a str>,
    pub language: Option<&'a str>,
}

impl QueryScope<'_> {
    /// True when no narrowing at all is requested — the query then produces
    /// byte-for-byte the SQL (and therefore the results) of the unscoped
    /// method it backs.
    pub fn is_empty(&self) -> bool {
        self.path.is_none() && self.language.is_none()
    }
}

/// A [`QueryScope`] whose `path` has already been resolved to
/// exact-file-vs-directory-prefix by the caller, which is the only layer that
/// knows the project root (see `Index::resolve_is_file`). Keeping the decision
/// out of this module is what stops `.claude`-style dotted directories from
/// being mistaken for files (see §B5 of `investigacion.md`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedScope<'a> {
    pub path: Option<&'a str>,
    /// Only consulted when `path` is `Some`.
    pub path_is_file: bool,
    pub language: Option<&'a str>,
}

/// Boxed values bound to a statement's `?N` placeholders, in placeholder order.
type BoundValues = Vec<Box<dyn rusqlite::ToSql>>;

/// Appends the scope predicates to `sql` and their values to `bound`.
///
/// Only the static predicate fragments are concatenated into the SQL string;
/// every user-supplied value stays bound through a `?N` placeholder, where `N`
/// is derived from the number of already-bound values, never from input.
fn push_scope(sql: &mut String, bound: &mut BoundValues, scope: ResolvedScope<'_>) {
    if let Some(path) = scope.path {
        if scope.path_is_file {
            sql.push_str(&format!(" AND f.relative_path = ?{}", bound.len() + 1));
            bound.push(Box::new(path.to_string()));
        } else {
            sql.push_str(&format!(" AND f.relative_path LIKE ?{}", bound.len() + 1));
            bound.push(Box::new(format!("{path}/%")));
        }
    }
    if let Some(language) = scope.language {
        sql.push_str(&format!(" AND f.language = ?{}", bound.len() + 1));
        bound.push(Box::new(language.to_string()));
    }
}

/// Symbol definitions named `name`, narrowed to `scope`.
///
/// All symbol names are search parameters bound via placeholders (`?1`), never
/// interpolated into SQL — see `mct-index` security notes in the project spec.
/// With an empty scope the built SQL is identical to the pre-scope query, so
/// the unscoped lookup is unchanged, hit for hit.
pub fn find_symbol_scoped(
    conn: &Connection,
    name: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<SymbolHit>> {
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name = ?1",
    );
    let mut bound: BoundValues = vec![Box::new(name.to_string())];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(SymbolHit {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                relative_path: row.get(3)?,
                line: row.get(4)?,
                column: row.get(5)?,
                parent: row.get(6)?,
                end_line: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// How [`find_symbol_matching`] compares `name` against indexed symbol names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymbolMatchMode {
    /// `s.name = name`, byte for byte — [`find_symbol`]'s original behavior.
    #[default]
    Exact,
    /// Every symbol with a *token* starting with `name`, via an FTS5 prefix
    /// query (`"name"*`) against `symbols_fts` — the table schema.rs already
    /// builds and maintains on every write but that, before this, no query
    /// read.
    ///
    /// "Token" because `symbols_fts` is built with FTS5's default
    /// `unicode61` tokenizer, which splits a name on non-alphanumeric
    /// characters: `name = "body"` matches `parse_body` (its second token is
    /// exactly `body`) as well as `body_parser`, but not `megabody` (one
    /// token, `megabody`, which does not *start with* `body`). This is a
    /// prefix match per word, not an arbitrary-position substring match —
    /// for the latter, see `Fuzzy`.
    Prefix,
    /// Every symbol whose name contains `name` anywhere, case-insensitively.
    ///
    /// This is a substring match via `LIKE`, not true edit-distance fuzzy
    /// matching: `symbols_fts` is built with FTS5's default `unicode61`
    /// tokenizer, which supports trailing-wildcard prefix queries
    /// (`Prefix` above) but not infix/leading wildcards — that needs the
    /// `trigram` tokenizer, which means rebuilding `symbols_fts`, a schema
    /// change out of scope here (see `CLAUDE.md`'s cross-cutting-change
    /// rule). `LIKE` gets the same "I don't remember the exact/full name"
    /// use case without touching the schema.
    Fuzzy,
}

/// [`find_symbol`], widened to [`SymbolMatchMode::Prefix`] or
/// [`SymbolMatchMode::Fuzzy`]. `SymbolMatchMode::Exact` delegates to
/// [`find_symbol`] verbatim — same SQL, same result order.
pub fn find_symbol_matching(
    conn: &Connection,
    name: &str,
    mode: SymbolMatchMode,
) -> Result<Vec<SymbolHit>> {
    find_symbol_matching_scoped(conn, name, mode, ResolvedScope::default())
}

/// [`find_symbol_matching`], additionally narrowed to `scope` — see
/// [`find_symbol_scoped`]. With an empty scope, identical hit for hit.
pub fn find_symbol_matching_scoped(
    conn: &Connection,
    name: &str,
    mode: SymbolMatchMode,
    scope: ResolvedScope<'_>,
) -> Result<Vec<SymbolHit>> {
    match mode {
        SymbolMatchMode::Exact => find_symbol_scoped(conn, name, scope),
        SymbolMatchMode::Prefix => {
            let fts_query = format!("{}*", quote_fts_phrase(name));
            find_symbol_fts(conn, &fts_query, scope)
        }
        SymbolMatchMode::Fuzzy => find_symbol_like(conn, name, scope),
    }
}

/// Quotes `term` as a single FTS5 phrase (`"term"`, with embedded `"`
/// doubled per FTS5's escaping rule) so it is matched literally — including
/// any character FTS5's query syntax would otherwise treat as an operator
/// (`*`, `:`, `(`, `-`, whitespace, ...). Combined with a trailing `*`
/// outside the quotes, `"term"*` is FTS5's documented syntax for "phrase,
/// prefix-matched".
fn quote_fts_phrase(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn find_symbol_fts(
    conn: &Connection,
    fts_query: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<SymbolHit>> {
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line
         FROM symbols_fts
         JOIN symbols s ON s.id = symbols_fts.rowid
         JOIN files f ON f.id = s.file_id
         WHERE symbols_fts MATCH ?1",
    );
    let mut bound: BoundValues = vec![Box::new(fts_query.to_string())];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(SymbolHit {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                relative_path: row.get(3)?,
                line: row.get(4)?,
                column: row.get(5)?,
                parent: row.get(6)?,
                end_line: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Case-insensitive substring match on `symbols.name` directly — see
/// [`SymbolMatchMode::Fuzzy`] for why this bypasses `symbols_fts`. `term`'s
/// own `%`/`_`/`\` are escaped before being wrapped in `%...%`, so a name
/// containing a literal percent or underscore can't widen the match.
fn find_symbol_like(
    conn: &Connection,
    term: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<SymbolHit>> {
    let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name LIKE ?1 ESCAPE '\\'",
    );
    let mut bound: BoundValues = vec![Box::new(pattern)];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(SymbolHit {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                relative_path: row.get(3)?,
                line: row.get(4)?,
                column: row.get(5)?,
                parent: row.get(6)?,
                end_line: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Lists symbol definitions discovered under `path`, with no name known in
/// advance — the discovery step ahead of `find_symbol`/`find_references`/
/// `find_calls`/`find_callers`/`impact_analysis` in the explore -> locate ->
/// navigate -> read pipeline.
///
/// `path` is matched as an exact file when `is_file`, or as a directory/crate
/// prefix otherwise (e.g. `src` matches every file under `src/`). The caller
/// decides which, because only it knows the project root — see
/// `Index::resolve_is_file`. `kind` and `language` narrow the result with an
/// exact match, combined with AND when both are given.
pub fn list_symbols(
    conn: &Connection,
    path: &str,
    is_file: bool,
    kind: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<SymbolListEntry>> {
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.end_line, s.parent
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE ",
    );
    sql.push_str(if is_file {
        "f.relative_path = ?1"
    } else {
        "f.relative_path LIKE ?1"
    });

    let mut bound: BoundValues = vec![Box::new(if is_file {
        path.to_string()
    } else {
        format!("{path}/%")
    })];
    if let Some(k) = kind {
        sql.push_str(&format!(" AND s.kind = ?{}", bound.len() + 1));
        bound.push(Box::new(k.to_string()));
    }
    if let Some(l) = language {
        sql.push_str(&format!(" AND f.language = ?{}", bound.len() + 1));
        bound.push(Box::new(l.to_string()));
    }
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(SymbolListEntry {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                relative_path: row.get(3)?,
                line: row.get(4)?,
                end_line: row.get(5)?,
                parent: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// The three relation queries differ only in their WHERE predicate; the
/// SELECT/JOIN prefix and the ORDER BY are shared, with any scope predicates
/// spliced in between.
const RELATION_SELECT: &str =
    "SELECT r.kind, caller.name, r.to_name, f.language, f.relative_path, r.line, r.column
         FROM relations r
         JOIN symbols caller ON caller.id = r.from_symbol_id
         JOIN files f ON f.id = caller.file_id
         WHERE ";

/// Every relation kind pointing at `symbol`, narrowed to `scope`.
/// `ResolvedScope::default()` is the unscoped query, row for row.
pub fn find_references_scoped(
    conn: &Connection,
    symbol: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(conn, "r.to_name = ?1", symbol, scope)
}

pub fn find_calls_scoped(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "caller.name = ?1 AND r.kind = 'calls'",
        function,
        scope,
    )
}

pub fn find_callers_scoped(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(conn, "r.to_name = ?1 AND r.kind = 'calls'", function, scope)
}

/// Direct-caller counts for every called name, in one query — the whole
/// fan-in ranking `get_project_overview` needs, instead of one
/// `find_callers` per candidate symbol.
pub fn fan_in_counts(conn: &Connection) -> Result<HashMap<String, usize>> {
    let mut stmt = conn.prepare_cached(
        "SELECT to_name, COUNT(*) FROM relations WHERE kind = 'calls' GROUP BY to_name",
    )?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((name, count.max(0) as usize))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (name, count) = row?;
        counts.insert(name, count);
    }
    Ok(counts)
}

/// Total reference counts for every referenced name, across *every* relation
/// kind (calls/imports/extends/implements/plain references) — the whole-
/// project fan-in `find_dead_code` needs to tell "zero references anywhere"
/// from "referenced", in one query instead of one `find_references` per
/// candidate symbol. Unlike [`fan_in_counts`] (which is `calls`-only, for
/// `get_project_overview`'s ranking), this counts every relation kind since a
/// symbol referenced only via an import or a trait `impl` is not dead code.
pub fn reference_counts(conn: &Connection) -> Result<HashMap<String, usize>> {
    let mut stmt =
        conn.prepare_cached("SELECT to_name, COUNT(*) FROM relations GROUP BY to_name")?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((name, count.max(0) as usize))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (name, count) = row?;
        counts.insert(name, count);
    }
    Ok(counts)
}

/// `predicate` is a static SQL fragment chosen by the caller (never built from
/// input) whose `?1` placeholder binds `param`; scope values bind to `?2`
/// onwards.
fn query_relations(
    conn: &Connection,
    predicate: &str,
    param: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    let mut sql = String::from(RELATION_SELECT);
    sql.push_str(predicate);
    let mut bound: BoundValues = vec![Box::new(param.to_string())];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str("\n         ORDER BY f.relative_path, r.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(RelationHit {
                kind: row.get(0)?,
                from_symbol: row.get(1)?,
                to_name: row.get(2)?,
                language: row.get(3)?,
                relative_path: row.get(4)?,
                line: row.get(5)?,
                column: row.get(6)?,
                depth: 1,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
