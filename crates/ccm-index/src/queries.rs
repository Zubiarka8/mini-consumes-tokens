use rusqlite::{params, Connection};

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

/// All symbol names are search parameters bound via placeholders (`?1`), never
/// interpolated into SQL — see `ccm-index` security notes in the project spec.
pub fn find_symbol(conn: &Connection, name: &str) -> Result<Vec<SymbolHit>> {
    let mut stmt = conn.prepare(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name = ?1
         ORDER BY f.relative_path, s.line",
    )?;
    let rows = stmt
        .query_map(params![name], |row| {
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
/// `path` is matched as an exact file when its last segment contains a `.`
/// (e.g. `src/lib.rs`), or as a directory/crate prefix otherwise (e.g. `src`
/// matches every file under `src/`). `kind` and `language` narrow the result
/// with an exact match, combined with AND when both are given.
pub fn list_symbols(
    conn: &Connection,
    path: &str,
    kind: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<SymbolListEntry>> {
    let is_file = path.rsplit('/').next().unwrap_or(path).contains('.');

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

    let mut bound: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(if is_file {
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

    let mut stmt = conn.prepare(&sql)?;
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

pub fn find_references(conn: &Connection, symbol: &str) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "SELECT r.kind, caller.name, r.to_name, f.language, f.relative_path, r.line, r.column
         FROM relations r
         JOIN symbols caller ON caller.id = r.from_symbol_id
         JOIN files f ON f.id = caller.file_id
         WHERE r.to_name = ?1
         ORDER BY f.relative_path, r.line",
        symbol,
    )
}

pub fn find_calls(conn: &Connection, function: &str) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "SELECT r.kind, caller.name, r.to_name, f.language, f.relative_path, r.line, r.column
         FROM relations r
         JOIN symbols caller ON caller.id = r.from_symbol_id
         JOIN files f ON f.id = caller.file_id
         WHERE caller.name = ?1 AND r.kind = 'calls'
         ORDER BY f.relative_path, r.line",
        function,
    )
}

pub fn find_callers(conn: &Connection, function: &str) -> Result<Vec<RelationHit>> {
    query_relations(
        conn,
        "SELECT r.kind, caller.name, r.to_name, f.language, f.relative_path, r.line, r.column
         FROM relations r
         JOIN symbols caller ON caller.id = r.from_symbol_id
         JOIN files f ON f.id = caller.file_id
         WHERE r.to_name = ?1 AND r.kind = 'calls'
         ORDER BY f.relative_path, r.line",
        function,
    )
}

fn query_relations(conn: &Connection, sql: &str, param: &str) -> Result<Vec<RelationHit>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![param], |row| {
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
