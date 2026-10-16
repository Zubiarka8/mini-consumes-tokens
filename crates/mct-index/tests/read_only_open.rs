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
