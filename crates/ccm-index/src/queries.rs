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
}

/// All symbol names are search parameters bound via placeholders (`?1`), never
/// interpolated into SQL — see `ccm-index` security notes in the project spec.
pub fn find_symbol(conn: &Connection, name: &str) -> Result<Vec<SymbolHit>> {
    let mut stmt = conn.prepare(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent
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
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
