//! Migration 13 (ADR-026): a file's `module` symbol is dropped from a
//! relation's candidates only when the same file declares a same-named,
//! same-parent symbol that is not a module.
//!
//! The rule's safety argument is that such a symbol satisfies every predicate
//! the module satisfied, so no relation can lose its last candidate. The
//! enumeration below checks it against the real view instead of trusting the
//! argument: every combination of the relation attributes the view reads
//! (kind, qualifier, target path, language, `module`, `target_module`,
//! `member`, `target_kind`) against every placement of the module and of a
//! would-be alternative (kind, parent, file, language).

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection};

use super::migrations;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Candidates = HashMap<i64, BTreeSet<i64>>;

const MODULE_ID: i64 = 20;
const ALTERNATIVE_ID: i64 = 21;
const FIRST_RELATION_ID: i64 = 1_000;

/// A migrated in-memory database with a second view, `old_relation_candidates`,
/// that is the current one without the migration-13 clause.
fn connection() -> Result<Connection, Box<dyn std::error::Error>> {
    let mut conn = Connection::open_in_memory()?;
    crate::search::register_functions(&conn)?;
    migrations().to_latest(&mut conn)?;
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE name = 'relation_candidates'",
        [],
        |row| row.get(0),
    )?;
    let old = sql.replace(shadowed_module_predicate!(), "");
    assert_ne!(old, sql, "the view must contain the migration-13 clause");
    conn.execute_batch(&old.replacen(
        "CREATE VIEW relation_candidates",
        "CREATE VIEW old_relation_candidates",
        1,
    ))?;
    conn.execute_batch(
        "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
             (1, 'lib/Order.php', 'php', 'h', 0),
             (2, 'other/X.php', 'php', 'h', 0),
             (3, 'other/Y.java', 'java', 'h', 0);
         INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
             VALUES (10, 2, 'caller', 'function', 1, 1, 1);",
    )?;
    Ok(conn)
}

fn candidates(conn: &Connection, view: &str) -> Result<Candidates, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "SELECT relation_id, symbol_id FROM {view} WHERE relation_id >= {FIRST_RELATION_ID}"
    ))?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
    let mut map = Candidates::new();
    for row in rows {
        let (relation, symbol) = row?;
        map.entry(relation).or_default().insert(symbol);
    }
    Ok(map)
}

/// The module of `lib/Order.php` named `Zorp`, carrying the namespace alias
/// `ns` that `target_module` scopes look for, plus an optional alternative.
fn place_symbols(
    conn: &Connection,
    module_parent: Option<&str>,
    alternative: Option<(&str, Option<&str>, i64)>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len)
         VALUES (?1, 1, 'Zorp', 'module', ?2, 1, 1, 1)",
        params![MODULE_ID, module_parent],
    )?;
    conn.execute(
        "INSERT INTO symbol_aliases (symbol_id, alias) VALUES (?1, 'ns')",
        params![MODULE_ID],
    )?;
    if let Some((kind, parent, file)) = alternative {
        conn.execute(
            "INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len)
             VALUES (?1, ?2, 'Zorp', ?3, ?4, 3, 1, 1)",
            params![ALTERNATIVE_ID, file, kind, parent],
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_relation(
    conn: &Connection,
    id: i64,
    kind: &str,
    qualifier: Option<&str>,
    target_path: Option<&str>,
    target_language: Option<&str>,
    module: Option<&str>,
    target_module: Option<&str>,
    member: bool,
    target_kind: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                targets_parsed, external, qualifier, target_path,
                                target_language, module, target_module, member, target_kind)
         VALUES (?1, 10, ?2, 'Zorp', 1, 1, 1, 1, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            kind,
            qualifier,
            target_path,
            target_language,
            module,
            target_module,
            member,
            target_kind
        ],
    )?;
    Ok(())
}

/// Every relation shape the view can tell apart.
fn insert_every_relation_shape(conn: &Connection) -> rusqlite::Result<i64> {
    let mut id = FIRST_RELATION_ID;
    for kind in ["references", "implements", "calls"] {
        for qualifier in [None, Some("Q"), Some("Z")] {
            for target_path in [None, Some("lib/Order.php"), Some("other/X.php")] {
                for target_language in [None, Some("php"), Some("java")] {
                    for module in [None, Some("lib"), Some("Q")] {
                        for target_module in [None, Some("ns"), Some("zz")] {
                            for member in [false, true] {
                                for target_kind in [None, Some("module"), Some("class")] {
                                    insert_relation(
                                        conn,
                                        id,
                                        kind,
                                        qualifier,
                                        target_path,
                                        target_language,
                                        module,
                                        target_module,
                                        member,
                                        target_kind,
                                    )?;
                                    id += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(id - FIRST_RELATION_ID)
}

#[test]
fn no_relation_loses_its_last_candidate_and_only_a_shadowed_module_is_dropped() -> TestResult {
    let mut conn = connection()?;
    let mut with_candidates = 0usize;
    let mut changed = 0usize;
    let mut shapes = 0i64;

    let parents = [None, Some("Q"), Some("Q<T>"), Some("Z")];
    let mut placements = vec![None];
    for kind in ["class", "function", "method"] {
        for parent in parents {
            for file in [1, 2, 3] {
                placements.push(Some((kind, parent, file)));
            }
        }
    }

    for module_parent in [None, Some("Q"), Some("Q<T>")] {
        for alternative in &placements {
            let tx = conn.transaction()?;
            place_symbols(&tx, module_parent, *alternative)?;
            shapes = insert_every_relation_shape(&tx)?;
            let old = candidates(&tx, "old_relation_candidates")?;
            let new = candidates(&tx, "relation_candidates")?;
            for (relation, before) in &old {
                with_candidates += 1;
                let after = new.get(relation).cloned().unwrap_or_default();
                assert!(
                    !after.is_empty(),
                    "relation {relation} lost every candidate: module parent {module_parent:?}, \
                     alternative {alternative:?}, before {before:?}"
                );
                if &after != before {
                    changed += 1;
                    let removed: BTreeSet<i64> = before.difference(&after).copied().collect();
                    assert_eq!(
                        removed,
                        BTreeSet::from([MODULE_ID]),
                        "only the module may be dropped (relation {relation}, \
                         module parent {module_parent:?}, alternative {alternative:?})"
                    );
                    assert!(
                        after.contains(&ALTERNATIVE_ID),
                        "the module may be dropped only for the alternative that stays \
                         (relation {relation}, module parent {module_parent:?}, \
                         alternative {alternative:?})"
                    );
                }
            }
            // `tx` is dropped here: the next placement starts from a clean file set.
        }
    }
    // Guard the enumeration itself: it must keep exercising both outcomes.
    assert!(shapes > 2_000, "{shapes} relation shapes");
    assert!(
        with_candidates > 10_000,
        "{with_candidates} relations had candidates"
    );
    assert!(changed > 0, "the rule never applied");
    assert!(changed < with_candidates, "the rule applied everywhere");
    Ok(())
}

fn resolve_one(
    conn: &Connection,
    module_parent: Option<&str>,
    alternative: Option<(&str, Option<&str>, i64)>,
    relation: impl FnOnce(&Connection, i64) -> rusqlite::Result<()>,
) -> Result<BTreeSet<i64>, Box<dyn std::error::Error>> {
    place_symbols(conn, module_parent, alternative)?;
    relation(conn, FIRST_RELATION_ID)?;
    Ok(candidates(conn, "relation_candidates")?
        .remove(&FIRST_RELATION_ID)
        .unwrap_or_default())
}

#[test]
fn an_unpinned_reference_resolves_to_the_type_declared_in_the_modules_file() -> TestResult {
    let conn = connection()?;
    let got = resolve_one(&conn, None, Some(("class", None, 1)), |conn, id| {
        insert_relation(
            conn,
            id,
            "references",
            None,
            None,
            None,
            None,
            None,
            false,
            None,
        )
    })?;
    assert_eq!(got, BTreeSet::from([ALTERNATIVE_ID]));
    Ok(())
}

#[test]
fn a_reference_whose_only_candidate_is_a_module_still_resolves() -> TestResult {
    let conn = connection()?;
    let got = resolve_one(&conn, None, None, |conn, id| {
        insert_relation(
            conn,
            id,
            "references",
            None,
            None,
            None,
            None,
            None,
            false,
            None,
        )
    })?;
    assert_eq!(got, BTreeSet::from([MODULE_ID]));
    Ok(())
}

#[test]
fn a_same_named_symbol_in_another_file_stays_an_ambiguous_candidate() -> TestResult {
    let conn = connection()?;
    let got = resolve_one(&conn, None, Some(("function", None, 2)), |conn, id| {
        insert_relation(
            conn,
            id,
            "references",
            None,
            None,
            None,
            None,
            None,
            false,
            None,
        )
    })?;
    assert_eq!(got, BTreeSet::from([MODULE_ID, ALTERNATIVE_ID]));
    Ok(())
}

/// The case a same-file, same-name rule would have broken: the module is
/// nested under `Q` and the relation is qualified by `Q`, while the
/// same-named type has another parent and does not satisfy the qualifier.
#[test]
fn a_nested_module_keeps_its_qualified_relation_when_the_type_has_another_parent() -> TestResult {
    let conn = connection()?;
    let got = resolve_one(&conn, Some("Q"), Some(("class", None, 1)), |conn, id| {
        insert_relation(
            conn,
            id,
            "references",
            Some("Q"),
            None,
            None,
            None,
            None,
            false,
            None,
        )
    })?;
    assert_eq!(got, BTreeSet::from([MODULE_ID]));
    Ok(())
}

#[test]
fn a_pinned_or_imported_module_is_untouched() -> TestResult {
    let conn = connection()?;
    place_symbols(&conn, None, Some(("class", None, 1)))?;
    insert_relation(
        &conn,
        1_000,
        "references",
        None,
        None,
        None,
        None,
        None,
        false,
        Some("module"),
    )?;
    insert_relation(
        &conn, 1_001, "imports", None, None, None, None, None, false, None,
    )?;
    let got = candidates(&conn, "relation_candidates")?;
    assert_eq!(got.get(&1_000), Some(&BTreeSet::from([MODULE_ID])));
    assert_eq!(
        got.get(&1_001),
        Some(&BTreeSet::from([MODULE_ID, ALTERNATIVE_ID]))
    );
    Ok(())
}
