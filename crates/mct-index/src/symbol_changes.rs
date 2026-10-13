//! Opt-in accounting of which symbols a reindex added or removed.
//!
//! The counters in [`ReindexReport`] are per file (`files_parsed`,
//! `files_removed`), so deleting one function from a file that is otherwise
//! unchanged only shows as `files_parsed = 1`. When tracking is on, `TEMP`
//! triggers on the `symbols` table record every insert and delete of the
//! current run, wherever it comes from (a rewritten file, a deleted file, a
//! rename sweep, a cascade), and [`finish`] nets them out: a symbol that was
//! deleted and re-inserted unchanged cancels out, so only real additions and
//! removals are reported.
//!
//! `TEMP` objects live in the connection, not in `index.sqlite3`: there is no
//! schema migration and nothing is persisted. Tracking is off unless a caller
//! such as the MCP server's watcher asks for it, so the initial index of a
//! large project pays nothing.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::indexer::ReindexReport;
use crate::Result;

/// At most this many removed symbols are named in a report; the count is
/// always exact.
const MAX_LISTED: usize = 20;

const TRIGGER_NAMES: [&str; 3] = [
    "symbol_change_file_gone",
    "symbol_change_added",
    "symbol_change_removed",
];

/// Starts recording symbol changes on `conn`. Idempotent.
pub(crate) fn enable(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS symbol_change_log (
             op     TEXT NOT NULL,
             path   TEXT NOT NULL,
             kind   TEXT NOT NULL,
             name   TEXT NOT NULL,
             parent TEXT
         );
         CREATE TEMP TABLE IF NOT EXISTS symbol_change_paths (
             file_id INTEGER PRIMARY KEY,
             path    TEXT NOT NULL
         );
         -- A deleted file's symbols go by cascade after its row is gone, so
         -- remember the path while it can still be read.
         CREATE TEMP TRIGGER IF NOT EXISTS symbol_change_file_gone
         BEFORE DELETE ON main.files
         BEGIN
             INSERT OR REPLACE INTO symbol_change_paths VALUES (OLD.id, OLD.relative_path);
         END;
         CREATE TEMP TRIGGER IF NOT EXISTS symbol_change_added
         AFTER INSERT ON main.symbols
         BEGIN
             INSERT INTO symbol_change_log
             SELECT '+',
                    COALESCE((SELECT relative_path FROM main.files WHERE id = NEW.file_id), '?'),
                    NEW.kind, NEW.name, NEW.parent;
         END;
         CREATE TEMP TRIGGER IF NOT EXISTS symbol_change_removed
         AFTER DELETE ON main.symbols
         BEGIN
             INSERT INTO symbol_change_log
             SELECT '-',
                    COALESCE((SELECT relative_path FROM main.files WHERE id = OLD.file_id),
                             (SELECT path FROM symbol_change_paths WHERE file_id = OLD.file_id),
                             '?'),
                    OLD.kind, OLD.name, OLD.parent;
         END;",
    )?;
    Ok(())
}

/// Stops recording and drops what was recorded. Idempotent.
pub(crate) fn disable(conn: &Connection) -> Result<()> {
    for name in TRIGGER_NAMES {
        conn.execute_batch(&format!("DROP TRIGGER IF EXISTS temp.{name};"))?;
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.symbol_change_log;
         DROP TABLE IF EXISTS temp.symbol_change_paths;",
    )?;
    Ok(())
}

fn enabled(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM temp.sqlite_master WHERE type = 'table' AND name = 'symbol_change_log'",
        [],
        |_| Ok(()),
    )
    .is_ok()
}

/// Forgets whatever an earlier run recorded. A no-op when tracking is off.
pub(crate) fn begin(conn: &Connection) -> Result<()> {
    if enabled(conn) {
        conn.execute_batch(
            "DELETE FROM temp.symbol_change_log; DELETE FROM temp.symbol_change_paths;",
        )?;
    }
    Ok(())
}

/// Nets out the recorded inserts and deletes into `report`. A no-op when
/// tracking is off.
pub(crate) fn finish(conn: &Connection, report: &mut ReindexReport) -> Result<()> {
    if !enabled(conn) {
        return Ok(());
    }
    let mut stmt = conn
        .prepare("SELECT op, path, kind, name, COALESCE(parent, '') FROM temp.symbol_change_log")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            (
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ),
        ))
    })?;
    // (path, kind, name, parent) -> inserts minus deletes.
    let mut balance: HashMap<(String, String, String, String), i64> = HashMap::new();
    for row in rows {
        let (op, key) = row?;
        *balance.entry(key).or_default() += if op == "+" { 1 } else { -1 };
    }
    drop(stmt);

    let mut removed: Vec<String> = Vec::new();
    for ((path, kind, name, _), net) in balance {
        if net > 0 {
            report.symbols_added += net as usize;
        } else if net < 0 {
            let count = net.unsigned_abs() as usize;
            report.symbols_removed += count;
            removed.extend(std::iter::repeat_n(
                format!("{kind} {name} ({path})"),
                count,
            ));
        }
    }
    removed.sort();
    removed.truncate(MAX_LISTED);
    report.removed_symbols = removed;
    Ok(())
}
