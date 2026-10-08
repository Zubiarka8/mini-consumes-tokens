use std::collections::HashMap;

use rusqlite::Connection;

use crate::Result;

#[derive(Debug, Clone, Default)]
pub struct SymbolHit {
    /// The symbol's row id: an exact, snapshot-local identity — the key a
    /// resolved relation ([`RelationHit::target_id`]) points at. Not stable
    /// across a reindex of its file; source-stable consumers use the
    /// `(language, relative_path, kind, parent, name, line)` tuple instead.
    pub id: i64,
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
    /// Writer-declared depth of this symbol (e.g. 1..6 for a Markdown ATX
    /// heading), when the parser that produced it populates
    /// `SymbolRecord::level`. `None` for every other language.
    pub level: Option<u32>,
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
    /// Writer-declared depth of this symbol — see [`SymbolHit::level`].
    pub level: Option<u32>,
}

/// How a relation's target resolved against the index — see
/// `mct_core::RelationTarget` for the evidence a parser supplies and the
/// `relation_candidates` view (schema.rs) for the rule applied to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    /// Exactly one candidate definition: [`RelationHit::target_id`].
    Resolved,
    /// Several candidates remain, or one that the evidence can't prove (a
    /// call through a receiver of unknown type); none is picked.
    Ambiguous,
    /// The parser proved the target lies outside the repository.
    External,
    /// No candidate, unsupported semantics, or a row indexed before
    /// resolution existed. Not evidence that the target is external.
    #[default]
    Unresolved,
}

impl Resolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Ambiguous => "ambiguous",
            Self::External => "external",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Debug, Clone, Default)]
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
    /// Row id of the relation, for [`relation_candidates`].
    pub relation_id: i64,
    /// Exact row id of `from_symbol` — the relation's source is always known.
    pub from_symbol_id: i64,
    pub resolution: Resolution,
    /// The one definition `to_name` denotes here; `Some` only when
    /// `resolution` is [`Resolution::Resolved`].
    pub target_id: Option<i64>,
    /// How many definitions remain candidates (0 when unresolved/external).
    pub candidate_count: usize,
    /// Identity of the first [`SHOWN_CANDIDATES`] candidates, by path then
    /// line: the target itself when resolved. Enough for output to say
    /// *which* definition a hit reaches without a lookup per hit; the full
    /// set is [`relation_candidates`].
    pub candidates: Vec<CandidateRef>,
}

/// How many candidates a [`RelationHit`] carries.
pub const SHOWN_CANDIDATES: usize = 3;

/// Where one candidate definition of a relation is declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateRef {
    pub relative_path: String,
    pub line: u32,
    pub parent: Option<String>,
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
pub(crate) type BoundValues = Vec<Box<dyn rusqlite::ToSql>>;

/// Direction of a relation query.
#[derive(Debug, Clone, Copy)]
pub enum RelationDirection {
    Calls,
    Callers,
    References,
}

/// A rendered page, with an exact total only when it is known.
#[derive(Debug)]
pub struct RelationPage {
    pub hits: Vec<RelationHit>,
    pub total: Option<usize>,
    pub has_more: bool,
    pub offset: usize,
}

impl RelationDirection {
    fn predicate(self) -> &'static str {
        match self {
            Self::Calls => "caller.name = ?1 AND r.kind = 'calls'",
            Self::Callers => "r.to_name = ?1 AND r.kind = 'calls'",
            Self::References => "r.to_name = ?1",
        }
    }
}

/// Appends the scope predicates to `sql` and their values to `bound`.
///
/// Only the static predicate fragments are concatenated into the SQL string;
/// every user-supplied value stays bound through a `?N` placeholder, where `N`
/// is derived from the number of already-bound values, never from input.
pub(crate) fn push_scope(sql: &mut String, bound: &mut BoundValues, scope: ResolvedScope<'_>) {
    if let Some(path) = scope.path {
        if scope.path_is_file {
            sql.push_str(&format!(" AND f.relative_path = ?{}", bound.len() + 1));
            bound.push(Box::new(path.to_string()));
        } else {
            sql.push_str(&format!(
                " AND f.relative_path LIKE ?{} ESCAPE '\\'",
                bound.len() + 1
            ));
            bound.push(Box::new(directory_pattern(path)));
        }
    }
    if let Some(language) = scope.language {
        sql.push_str(&format!(" AND f.language = ?{}", bound.len() + 1));
        bound.push(Box::new(language.to_string()));
    }
}

/// Directory names are literal, even when they contain SQL LIKE metacharacters.
fn directory_pattern(path: &str) -> String {
    let escaped = path
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}/%")
}

/// Reads a [`SymbolHit`] from columns 0..=9 of a row selected as
/// `s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent,
/// s.end_line, s.level, s.id`.
pub(crate) fn symbol_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolHit> {
    Ok(SymbolHit {
        name: row.get(0)?,
        kind: row.get(1)?,
        language: row.get(2)?,
        relative_path: row.get(3)?,
        line: row.get(4)?,
        column: row.get(5)?,
        parent: row.get(6)?,
        end_line: row.get(7)?,
        level: row.get(8)?,
        id: row.get(9)?,
    })
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
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name = ?1",
    );
    let mut bound: BoundValues = vec![Box::new(name.to_string())];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), symbol_hit)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Every symbol defined in any of `paths` (exact relative paths), ordered by
/// path then line, in one query — the paths travel as one bound JSON array
/// read back through `json_each`, never interpolated into the SQL.
pub fn symbols_in_files(conn: &Connection, paths: &[&str]) -> Result<Vec<SymbolHit>> {
    let paths = serde_json::to_string(paths).unwrap_or_else(|_| "[]".to_string());
    let mut stmt = conn.prepare_cached(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE f.relative_path IN (SELECT value FROM json_each(?1))
         ORDER BY f.relative_path, s.line",
    )?;
    let rows = stmt
        .query_map([paths], symbol_hit)?
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
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
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
        .query_map(params.as_slice(), symbol_hit)?
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
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name LIKE ?1 ESCAPE '\\'",
    );
    let mut bound: BoundValues = vec![Box::new(pattern)];
    push_scope(&mut sql, &mut bound, scope);
    sql.push_str(" ORDER BY f.relative_path, s.line");

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), symbol_hit)?
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
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.end_line, s.parent, s.level
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE ",
    );
    sql.push_str(if is_file {
        "f.relative_path = ?1"
    } else {
        "f.relative_path LIKE ?1 ESCAPE '\\'"
    });

    let mut bound: BoundValues = vec![Box::new(if is_file {
        path.to_string()
    } else {
        directory_pattern(path)
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
                level: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// The three relation queries differ only in their WHERE predicate; the
/// SELECT/JOIN prefix and the ORDER BY are shared, with any scope predicates
/// spliced in between.
///
/// The last two columns are the candidate count and, for exactly one
/// candidate, its id — correlated lookups into `relation_candidates`, so a
/// relation's resolution is always computed against the current symbols.
const RELATION_SELECT: &str =
    "SELECT r.kind, caller.name, r.to_name, f.language, f.relative_path, r.line, r.column,
            r.id, r.from_symbol_id, r.external,
            (SELECT COUNT(*) FROM relation_candidates c WHERE c.relation_id = r.id),
            (SELECT MIN(c.symbol_id) FROM relation_candidates c WHERE c.relation_id = r.id),
            r.member
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
    query_relations(conn, "r.to_name = ?1", symbol.to_string(), scope)
}

pub fn find_calls_scoped(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "caller.name = ?1 AND r.kind = 'calls'",
        function.to_string(),
        scope,
    )
}

pub fn find_callers_scoped(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "r.to_name = ?1 AND r.kind = 'calls'",
        function.to_string(),
        scope,
    )
}

/// Every non-call relation made *by* `symbol` — what it imports, extends,
/// implements or otherwise references. The outgoing counterpart of
/// [`find_references_scoped`] minus the calls [`find_calls_scoped`] covers.
pub fn find_dependencies_scoped(
    conn: &Connection,
    symbol: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "caller.name = ?1 AND r.kind <> 'calls'",
        symbol.to_string(),
        scope,
    )
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
    param: impl rusqlite::ToSql + 'static,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    query_relations_page(conn, predicate, param, scope, None, 0)
}

/// A count as an SQLite integer, saturating instead of wrapping.
fn sql_count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// [`query_relations`] restricted to `limit` rows after skipping `offset`
/// (`None` = every row). The order is total — path, line, column, then the
/// relation id — so a page is always the exact slice of the full list, and
/// only the rows of the page pay for the candidate preview below.
fn query_relations_page(
    conn: &Connection,
    predicate: &str,
    param: impl rusqlite::ToSql + 'static,
    scope: ResolvedScope<'_>,
    limit: Option<usize>,
    offset: usize,
) -> Result<Vec<RelationHit>> {
    let (sql, bound) = relation_selection(predicate, param, scope, None);
    query_relation_rows(conn, sql, bound, limit, offset)
}

fn relation_selection(
    predicate: &str,
    param: impl rusqlite::ToSql + 'static,
    scope: ResolvedScope<'_>,
    start: Option<&[i64]>,
) -> (String, BoundValues) {
    let mut sql = String::from(RELATION_SELECT);
    sql.push_str(predicate);
    let mut bound: BoundValues = vec![Box::new(param)];
    push_scope(&mut sql, &mut bound, scope);
    if let Some(start) = start {
        // This is the SQL equivalent of traversal::reaches_start: unknown
        // and external relations stay; proven unrelated targets do not.
        sql.push_str(" AND (r.external = 1 OR NOT EXISTS (SELECT 1 FROM relation_candidates c WHERE c.relation_id = r.id)");
        if !start.is_empty() {
            sql.push_str(&format!(" OR EXISTS (SELECT 1 FROM relation_candidates c WHERE c.relation_id = r.id AND c.symbol_id IN (SELECT CAST(value AS INTEGER) FROM json_each(?{})))", bound.len() + 1));
            let ids = start
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            bound.push(Box::new(format!("[{ids}]")));
        }
        sql.push(')');
    }
    (sql, bound)
}

pub(crate) fn direct_relation_page(
    conn: &Connection,
    direction: RelationDirection,
    name: &str,
    scope: ResolvedScope<'_>,
    start: Option<&[i64]>,
    limit: usize,
    offset: usize,
) -> Result<RelationPage> {
    let (sql, bound) = relation_selection(direction.predicate(), name.to_string(), scope, start);
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM ({sql})"),
        params.as_slice(),
        |row| row.get(0),
    )?;
    let total = usize::try_from(total).unwrap_or(usize::MAX);
    let hits = query_relation_rows(conn, sql, bound, Some(limit), offset)?;
    Ok(RelationPage {
        has_more: offset.saturating_add(hits.len()) < total,
        hits,
        total: Some(total),
        offset,
    })
}

fn query_relation_rows(
    conn: &Connection,
    mut sql: String,
    mut bound: BoundValues,
    limit: Option<usize>,
    offset: usize,
) -> Result<Vec<RelationHit>> {
    sql.push_str("\n         ORDER BY f.relative_path, r.line, r.column, r.id");
    if let Some(limit) = limit {
        sql.push_str(&format!("\n         LIMIT ?{}", bound.len() + 1));
        bound.push(Box::new(sql_count(limit)));
        sql.push_str(&format!(" OFFSET ?{}", bound.len() + 1));
        bound.push(Box::new(sql_count(offset)));
    }

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            let external: bool = row.get(9)?;
            let count: i64 = row.get(10)?;
            let only: Option<i64> = row.get(11)?;
            let member: bool = row.get(12)?;
            let (resolution, target_id) = match (external, count) {
                (true, _) => (Resolution::External, None),
                (false, 0) => (Resolution::Unresolved, None),
                (false, 1) if !member => (Resolution::Resolved, only),
                (false, _) => (Resolution::Ambiguous, None),
            };
            Ok(RelationHit {
                kind: row.get(0)?,
                from_symbol: row.get(1)?,
                to_name: row.get(2)?,
                language: row.get(3)?,
                relative_path: row.get(4)?,
                line: row.get(5)?,
                column: row.get(6)?,
                depth: 1,
                relation_id: row.get(7)?,
                from_symbol_id: row.get(8)?,
                resolution,
                target_id,
                candidate_count: count.max(0) as usize,
                candidates: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<RelationHit>>>()?;
    let mut rows = rows;
    let mut preview = conn.prepare_cached(
        "SELECT f.relative_path, s.line, s.parent
         FROM relation_candidates c
         JOIN symbols s ON s.id = c.symbol_id
         JOIN files f ON f.id = s.file_id
         WHERE c.relation_id = ?1
         ORDER BY f.relative_path, s.line
         LIMIT ?2",
    )?;
    for hit in rows.iter_mut().filter(|h| h.candidate_count > 0) {
        hit.candidates = preview
            .query_map(
                rusqlite::params![hit.relation_id, SHOWN_CANDIDATES as i64],
                |row| {
                    Ok(CandidateRef {
                        relative_path: row.get(0)?,
                        line: row.get(1)?,
                        parent: row.get(2)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?;
    }
    Ok(rows)
}

/// Calls made by the one symbol row `symbol_id` — the identity-based hop of
/// a forward walk, which can't wander into a same-named definition.
/// At most `limit` of them (`None` = all), in the usual order.
pub(crate) fn calls_from_symbol(
    conn: &Connection,
    symbol_id: i64,
    scope: ResolvedScope<'_>,
    limit: Option<usize>,
) -> Result<Vec<RelationHit>> {
    query_relations_page(
        conn,
        "r.from_symbol_id = ?1 AND r.kind = 'calls'",
        symbol_id,
        scope,
        limit,
        0,
    )
}

/// The `limit` calls made by the function named `function` after skipping
/// `offset` — a slice of exactly what [`find_calls_scoped`] returns.
pub(crate) fn find_calls_page(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
    limit: usize,
    offset: usize,
) -> Result<Vec<RelationHit>> {
    query_relations_page(
        conn,
        "caller.name = ?1 AND r.kind = 'calls'",
        function.to_string(),
        scope,
        Some(limit),
        offset,
    )
}

/// A slice of [`find_callers_scoped`], as [`find_calls_page`] is of calls.
pub(crate) fn find_callers_page(
    conn: &Connection,
    function: &str,
    scope: ResolvedScope<'_>,
    limit: usize,
    offset: usize,
) -> Result<Vec<RelationHit>> {
    query_relations_page(
        conn,
        "r.to_name = ?1 AND r.kind = 'calls'",
        function.to_string(),
        scope,
        Some(limit),
        offset,
    )
}

/// A slice of [`find_references_scoped`], as [`find_calls_page`] is of calls.
pub(crate) fn find_references_page(
    conn: &Connection,
    symbol: &str,
    scope: ResolvedScope<'_>,
    limit: usize,
    offset: usize,
) -> Result<Vec<RelationHit>> {
    query_relations_page(
        conn,
        "r.to_name = ?1",
        symbol.to_string(),
        scope,
        Some(limit),
        offset,
    )
}

/// Non-call relations made by the one symbol row `symbol_id` — what it
/// imports, extends, implements or otherwise references.
pub fn dependencies_of_symbol(conn: &Connection, symbol_id: i64) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "r.from_symbol_id = ?1 AND r.kind <> 'calls'",
        symbol_id,
        ResolvedScope::default(),
    )
}

/// Relations that may denote the one symbol row `symbol_id` — resolved to it,
/// or ambiguous with it among the candidates — optionally only `calls`. A
/// relation resolved to another definition, or from a language that can't
/// name it, is not one of them.
pub(crate) fn relations_reaching_symbol(
    conn: &Connection,
    symbol_id: i64,
    calls_only: bool,
    scope: ResolvedScope<'_>,
    limit: Option<usize>,
) -> Result<Vec<RelationHit>> {
    let predicate = if calls_only {
        "r.kind = 'calls' AND r.id IN (SELECT relation_id FROM relation_candidates WHERE symbol_id = ?1)"
    } else {
        "r.id IN (SELECT relation_id FROM relation_candidates WHERE symbol_id = ?1)"
    };
    query_relations_page(conn, predicate, symbol_id, scope, limit, 0)
}

/// Every symbol row named `name` within `scope` — the start nodes of a walk.
pub(crate) fn symbol_ids_named(
    conn: &Connection,
    name: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<i64>> {
    Ok(find_symbol_scoped(conn, name, scope)?
        .into_iter()
        .map(|hit| hit.id)
        .collect())
}

/// The symbol rows with the given ids, in id order; unknown ids are skipped.
pub fn symbols_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<SymbolHit>> {
    let ids = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
    let mut stmt = conn.prepare_cached(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.id IN (SELECT value FROM json_each(?1))
         ORDER BY s.id",
    )?;
    let rows = stmt
        .query_map([ids], symbol_hit)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Every candidate definition of relation `relation_id`, ordered by path then
/// line: the one target when resolved, all of them when ambiguous, none when
/// unresolved/external.
pub fn relation_candidates(conn: &Connection, relation_id: i64) -> Result<Vec<SymbolHit>> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id
         FROM relation_candidates c
         JOIN symbols s ON s.id = c.symbol_id
         JOIN files f ON f.id = s.file_id
         WHERE c.relation_id = ?1
         ORDER BY f.relative_path, s.line",
    )?;
    let rows = stmt
        .query_map([relation_id], symbol_hit)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
