//! Turns matching join points into `calls` relations (ADR-038).
//!
//! A *use* of a value and a *declaration* of the same kind and value are one
//! link: the symbol that uses the endpoint calls the symbol that declares it.
//! Name resolution is not involved. The relation carries the declarer's file,
//! language and parent as evidence, so it resolves to exactly that symbol.
//! Several declarers of one value give one relation each, and the query layer
//! reports them all as ambiguous (see `RelationHit`).
//!
//! The pass is idempotent: it compares the relations it wants with the ones it
//! wrote earlier and changes only the difference, so a run that moved nothing
//! leaves the database untouched.

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::error::Result;

/// The `relations.join_kind` of a relation this pass wrote for HTTP.
const HTTP: &str = "http";

/// One relation the pass wants, which is also its identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Link {
    from_symbol_id: i64,
    to_name: String,
    qualifier: Option<String>,
    target_path: String,
    target_language: String,
    target_kind: String,
    line: u32,
    column: u32,
    byte_len: u32,
}

/// Brings the join relations up to date with the stored join points.
pub(crate) fn resolve(conn: &Connection) -> Result<()> {
    let wanted: HashSet<Link> = wanted(conn)?.into_iter().collect();
    let mut present: HashSet<Link> = HashSet::new();
    let mut delete = conn.prepare_cached("DELETE FROM relations WHERE id = ?1")?;
    for (id, link) in stored(conn)? {
        // Keep one row per wanted link; drop the rest.
        if !(wanted.contains(&link) && present.insert(link)) {
            delete.execute([id])?;
        }
    }
    let mut insert = conn.prepare_cached(
        "INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len,
                                qualifier, target_path, target_language, external,
                                targets_parsed, target_kind, member, not_field, join_kind)
         VALUES (?1, 'calls', ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 1, ?9, 0, 0, ?10)",
    )?;
    for link in wanted.iter().filter(|link| !present.contains(*link)) {
        insert.execute(params![
            link.from_symbol_id,
            link.to_name,
            link.line,
            link.column,
            link.byte_len,
            link.qualifier,
            link.target_path,
            link.target_language,
            link.target_kind,
            HTTP,
        ])?;
    }
    Ok(())
}

fn wanted(conn: &Connection) -> Result<Vec<Link>> {
    let mut stmt = conn.prepare_cached(
        "SELECT u.symbol_id, s.name, s.parent, f.relative_path, f.language, s.kind,
                u.line, u.column, u.byte_len
         FROM join_points u
         JOIN join_points d ON d.kind = u.kind AND d.value = u.value AND d.role = 'declares'
         JOIN symbols s ON s.id = d.symbol_id
         JOIN files f ON f.id = s.file_id
         WHERE u.role = 'uses' AND u.kind = 'http'",
    )?;
    let links = stmt
        .query_map([], |row| {
            Ok(Link {
                from_symbol_id: row.get(0)?,
                to_name: row.get(1)?,
                qualifier: row.get(2)?,
                target_path: row.get(3)?,
                target_language: row.get(4)?,
                target_kind: row.get(5)?,
                line: row.get(6)?,
                column: row.get(7)?,
                byte_len: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(links)
}

fn stored(conn: &Connection) -> Result<Vec<(i64, Link)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, from_symbol_id, to_name, qualifier, target_path, target_language,
                target_kind, line, column, byte_len
         FROM relations WHERE join_kind = 'http'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                Link {
                    from_symbol_id: row.get(1)?,
                    to_name: row.get(2)?,
                    qualifier: row.get(3)?,
                    target_path: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    target_language: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    target_kind: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    line: row.get(7)?,
                    column: row.get(8)?,
                    byte_len: row.get(9)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
