//! An index created before issue #267 added `rockspec_items` upgrades in place.
//!
//! Rockspecs are manifests: `index_file` re-parses them on every reindex and
//! never hash-skips them, so the migration needs no content-hash reset. These
//! tests pin that, and the read-only and old-binary edges around it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, IndexError};

const ROCKSPEC: &str = "package = 'demo'\nversion = '1.0-1'\nrockspec_format = '3.0'\n\
dependencies = { 'lua >= 5.1', 'lpeg ~> 1.0' }\n";

fn user_version(database: &std::path::Path) -> i64 {
    rusqlite::Connection::open(database)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// Rewinds a synthetic test database one migration, to the state an index had
/// before `rockspec_items` existed. Assumes `rockspec_items` is the newest
/// migration, which the final assertions of the test check rather than trust.
fn rewind_before_rockspec_items(database: &std::path::Path, latest: i64) {
    let connection = rusqlite::Connection::open(database).unwrap();
    connection
        .execute_batch("DROP TABLE rockspec_items;")
        .unwrap();
    connection
        .pragma_update(None, "user_version", latest - 1)
        .unwrap();
}

#[test]
fn an_index_from_before_rockspec_items_upgrades_and_fills_without_a_forced_reindex() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("demo-1.0-1.rockspec"), ROCKSPEC).unwrap();
    // The database lives outside the project root so it is never indexed.
    let store = tempfile::tempdir().unwrap();
    let database = store.path().join("index.db");
    let registry = LanguageRegistry::new();

    {
        let mut index = Index::open(project.path(), &database, ExcludeSet::new(&[])).unwrap();
        index.reindex(&registry, false).unwrap();
        assert_eq!(index.rockspec_report(None).unwrap().rockspecs.len(), 1);
    }
    let latest = user_version(&database);
    rewind_before_rockspec_items(&database, latest);

    // A read-only open must refuse the older schema, not fail on a missing table.
    let before = std::fs::read(&database).unwrap();
    assert!(matches!(
        Index::open_existing_read_only(project.path(), &database, ExcludeSet::new(&[])),
        Err(IndexError::IncompatibleReadOnlySchema)
    ));
    assert_eq!(std::fs::read(&database).unwrap(), before);

    // The write path migrates in place: the new table exists and is empty.
    let mut index = Index::open(project.path(), &database, ExcludeSet::new(&[])).unwrap();
    assert_eq!(
        user_version(&database),
        latest,
        "the upgrade did not reach the latest schema, or rockspec_items is no longer the newest migration"
    );
    assert!(index.rockspec_report(None).unwrap().rockspecs.is_empty());

    // An ordinary, non-forced reindex fills it from the unchanged rockspec.
    index.reindex(&registry, false).unwrap();
    let report = index.rockspec_report(None).unwrap();
    assert_eq!(report.rockspecs.len(), 1);
    assert_eq!(report.rockspecs[0].package.as_deref(), Some("demo"));

    // A rockspec has no `files` row, so there is no content hash to invalidate.
    let rows: i64 = rusqlite::Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM files WHERE relative_path LIKE '%.rockspec'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}
