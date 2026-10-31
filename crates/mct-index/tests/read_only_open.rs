#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::{ExcludeSet, Index, IndexError};

#[test]
fn read_only_open_never_creates_or_migrates_an_index() {
    let root = std::env::temp_dir().join(format!("mct-read-only-schema-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("test-index.db");
    let _ = std::fs::remove_file(&database);
    assert!(Index::open_existing_read_only(&root, &database, ExcludeSet::new(&[])).is_err());
    assert!(!database.exists());
    // This is a synthetic test database, never the user's index.
    let connection = rusqlite::Connection::open(&database).unwrap();
    for version in [0, 1, 9999] {
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        let before = std::fs::read(&database).unwrap();
        let result = Index::open_existing_read_only(&root, &database, ExcludeSet::new(&[]));
        assert!(matches!(
            result,
            Err(IndexError::IncompatibleReadOnlySchema)
        ));
        assert_eq!(std::fs::read(&database).unwrap(), before);
    }
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

/// A binary that predates a migration (issue #267 added `rockspec_items`) meets
/// a database whose `user_version` is ahead of every migration it knows. That
/// is the same state as a current binary meeting a version one past its own
/// latest, which is how it is reproduced here: no old build is available to
/// run. The write path must refuse explicitly and leave the files untouched.
#[test]
fn the_write_path_refuses_a_schema_newer_than_the_binary_and_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("index.db");
    let latest: i64 = {
        drop(Index::open(root.path(), &database, ExcludeSet::new(&[])).unwrap());
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    };
    {
        // This is a synthetic test database, never the user's index.
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .pragma_update(None, "user_version", latest + 1)
            .unwrap();
    }

    let listing = |dir: &std::path::Path| {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&path).unwrap_or_default(),
                )
            })
            .collect();
        files.sort();
        files
    };
    let before = listing(root.path());

    let result = Index::open(root.path(), &database, ExcludeSet::new(&[]));
    let Err(error) = result else {
        panic!("a database ahead of this binary's schema was opened for writing");
    };
    assert!(
        matches!(error, IndexError::Coordination(_)),
        "unexpected error: {error}"
    );
    assert!(!error.to_string().is_empty());

    assert_eq!(
        listing(root.path()),
        before,
        "the failed open changed files"
    );
    let connection = rusqlite::Connection::open(&database).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, latest + 1, "the schema version was rewritten");
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'rockspec_items'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 1, "the LuaRocks table was dropped or recreated");
}
