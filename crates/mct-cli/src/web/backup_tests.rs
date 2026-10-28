#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::web::reviews::{Kind, Status};
use crate::web::store::{FileStatus, Store};

fn project() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fs::create_dir(root.join(".mct-index")).unwrap();
    (directory, root)
}

fn populated(root: &Path) -> Store {
    let mut store = Store::open(root).unwrap();
    let identity = "a".repeat(64);
    store
        .set(
            (Kind::Duplicate, &identity, &"b".repeat(64)),
            Status::Confirmed,
            "decision reason",
            10,
        )
        .unwrap();
    store
        .add_comment(Kind::Duplicate, &identity, "persistent comment", 11)
        .unwrap();
    store
        .set_file_review(
            &identity,
            "src/main.rs",
            FileStatus::InReview,
            "persistent note",
            12,
        )
        .unwrap();
    store
}

fn rows(conn: &Connection, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
    // Fixed fixture table names only.
    let mut statement = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .unwrap();
    let count = statement.column_count();
    statement
        .query_map([], |row| (0..count).map(|column| row.get(column)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn live_wal_backup_restores_every_user_table_without_changing_source() {
    let (_directory, root) = project();
    let store = populated(&root);
    let writer = Connection::open(root.join(".mct-index").join(DATABASE)).unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    writer
        .execute("INSERT INTO meta VALUES ('wal-only', 'must survive')", [])
        .unwrap();
    assert!(
        fs::metadata(root.join(".mct-index/dashboard.sqlite3-wal"))
            .unwrap()
            .len()
            > 0
    );
    let before: Vec<_> = [
        "reviews",
        "review_history",
        "comments",
        "file_reviews",
        "meta",
        "sqlite_sequence",
    ]
    .map(|table| (table, rows(&writer, table)))
    .into_iter()
    .collect();
    let bundle = create(&root, None).unwrap();
    verify_restore(&bundle).unwrap();
    let source = completed_snapshot(&bundle).unwrap();
    let isolated = tempfile::tempdir().unwrap();
    let mut restored = Connection::open(isolated.path().join(DATABASE)).unwrap();
    copy_snapshot(&source, &mut restored, COPY_TIMEOUT).unwrap();
    for (table, expected) in before {
        assert_eq!(rows(&restored, table), expected, "restored {table}");
        assert_eq!(rows(&writer, table), expected, "source {table}");
    }
    assert!(!root.join(".mct-index/index.sqlite3").exists());
    assert_eq!(
        store.comments(Kind::Duplicate, &"a".repeat(64)).unwrap()[0].body,
        "persistent comment"
    );
}

#[test]
fn absent_source_is_never_created() {
    let (_directory, root) = project();
    assert!(create(&root, None).is_err());
    assert!(!root.join(".mct-index").join(DATABASE).exists());
    assert!(!root.join(".mct-index/backups").exists());
}

#[test]
fn future_and_older_schemas_are_refused_without_migration() {
    for version in [1, 99] {
        let (_directory, root) = project();
        drop(populated(&root));
        let connection = Connection::open(root.join(".mct-index").join(DATABASE)).unwrap();
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        let expected = rows(&connection, "file_reviews");
        assert!(create(&root, None).is_err());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        assert_eq!(rows(&connection, "file_reviews"), expected);
        assert!(!root.join(".mct-index/backups").exists());
    }
}

#[test]
fn a_different_database_with_the_same_version_is_refused() {
    let (_directory, root) = project();
    let connection = Connection::open(root.join(".mct-index").join(DATABASE)).unwrap();
    connection
        .execute_batch("CREATE TABLE other(value); PRAGMA user_version=4;")
        .unwrap();
    assert!(create(&root, None).is_err());
    assert!(!root.join(".mct-index/backups").exists());
}

#[test]
fn corrupt_source_never_publishes_a_backup() {
    let (_directory, root) = project();
    fs::write(
        root.join(".mct-index").join(DATABASE),
        b"not a SQLite database",
    )
    .unwrap();
    assert!(create(&root, None).is_err());
    assert!(!root.join(".mct-index/backups").exists());
}

#[test]
fn interrupted_bundle_and_modified_artifact_are_rejected() {
    let (_directory, root) = project();
    drop(populated(&root));
    let bundle = create(&root, None).unwrap();
    let manifest = fs::read(bundle.join(MANIFEST)).unwrap();
    fs::remove_file(bundle.join(MANIFEST)).unwrap();
    assert!(verify_restore(&bundle).is_err());
    fs::write(bundle.join(MANIFEST), manifest).unwrap();
    OpenOptions::new()
        .append(true)
        .open(bundle.join(DATABASE))
        .unwrap()
        .write_all(b"tampered")
        .unwrap();
    assert!(verify_restore(&bundle).is_err());
}

#[test]
fn future_schema_restore_is_rejected_even_with_updated_digest() {
    let (_directory, root) = project();
    drop(populated(&root));
    let bundle = create(&root, None).unwrap();
    let path = bundle.join(DATABASE);
    let old_sha = digest(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);
    let manifest = fs::read_to_string(bundle.join(MANIFEST))
        .unwrap()
        .replace(&old_sha, &digest(&path).unwrap())
        .replace("schema: 4", "schema: 99");
    fs::write(bundle.join(MANIFEST), manifest).unwrap();
    assert!(verify_restore(&bundle).is_err());
}

#[test]
fn manifest_duplicates_unknown_fields_and_wrong_schema_are_rejected() {
    let (_directory, root) = project();
    drop(populated(&root));
    let bundle = create(&root, None).unwrap();
    let original = fs::read_to_string(bundle.join(MANIFEST)).unwrap();
    for changed in [
        format!("{original}schema: 4\n"),
        original.replace("application:", "unknown:"),
        original.replace("schema: 4", "schema: 3"),
        original.replace(FORMAT, "mct-dashboard-backup/99"),
    ] {
        fs::write(bundle.join(MANIFEST), changed).unwrap();
        assert!(verify_restore(&bundle).is_err());
    }
}

#[test]
fn backups_inside_source_and_dotdot_escapes_are_refused() {
    let (_directory, root) = project();
    drop(populated(&root));
    for path in [
        root.join("saved-notes"),
        root.join(".mct-index/../saved-notes"),
    ] {
        assert!(create(&root, Some(&path)).is_err());
        assert!(!root.join("saved-notes").exists());
    }
}

#[test]
fn repeated_backups_never_replace_an_existing_bundle() {
    let (_directory, root) = project();
    drop(populated(&root));
    let first = create(&root, None).unwrap();
    let original = fs::read(first.join(MANIFEST)).unwrap();
    let second = create(&root, None).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(first.join(MANIFEST)).unwrap(), original);
    verify_restore(&first).unwrap();
    verify_restore(&second).unwrap();
}

#[test]
fn copy_deadline_aborts_without_reporting_success() {
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch("CREATE TABLE fixture(value);")
        .unwrap();
    let mut target = Connection::open_in_memory().unwrap();
    assert!(copy_snapshot(&source, &mut target, Duration::ZERO).is_err());
}

#[test]
fn pinned_wal_snapshot_is_consistent_while_another_connection_commits() {
    let (_directory, root) = project();
    drop(populated(&root));
    let source = read_only(&root.join(".mct-index").join(DATABASE)).unwrap();
    let snapshot = source.unchecked_transaction().unwrap();
    let before = rows(&snapshot, "meta"); // Establish the read snapshot.
    let writer = Connection::open(root.join(".mct-index").join(DATABASE)).unwrap();
    writer
        .execute(
            "INSERT INTO meta VALUES ('after-snapshot', 'new commit')",
            [],
        )
        .unwrap();
    let mut restored = Connection::open_in_memory().unwrap();
    copy_snapshot(&snapshot, &mut restored, COPY_TIMEOUT).unwrap();
    assert_eq!(rows(&restored, "meta"), before);
    assert_ne!(rows(&writer, "meta"), before);
    snapshot.commit().unwrap();
}

#[test]
fn busy_destination_stops_at_the_copy_deadline() {
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch("CREATE TABLE fixture(value);")
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("busy.sqlite3");
    let mut target = Connection::open(&path).unwrap();
    target.busy_timeout(Duration::ZERO).unwrap();
    let holder = Connection::open(&path).unwrap();
    holder
        .execute_batch("BEGIN EXCLUSIVE; CREATE TABLE held(value);")
        .unwrap();
    let start = Instant::now();
    assert!(copy_snapshot(&source, &mut target, Duration::from_millis(100)).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn invalid_foreign_keys_do_not_publish_a_completed_bundle() {
    let (_directory, root) = project();
    drop(populated(&root));
    let writer = Connection::open(root.join(".mct-index").join(DATABASE)).unwrap();
    writer
        .execute_batch(
            "PRAGMA foreign_keys=OFF;
        CREATE TABLE fixture_parent(id INTEGER PRIMARY KEY);
        CREATE TABLE fixture_child(parent INTEGER REFERENCES fixture_parent(id));
        INSERT INTO fixture_child VALUES (123);",
        )
        .unwrap();
    assert!(create(&root, None).is_err());
    let destination = root.join(".mct-index/backups");
    if destination.exists() {
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}

#[test]
fn failed_dashboard_migration_keeps_original_rows_and_version() {
    let (_directory, root) = project();
    drop(populated(&root));
    let path = root.join(".mct-index").join(DATABASE);
    let connection = Connection::open(&path).unwrap();
    let original = rows(&connection, "file_reviews");
    connection
        .execute_batch(
            "CREATE TABLE file_reviews_v3(sentinel TEXT);
        INSERT INTO file_reviews_v3 VALUES ('retain me'); PRAGMA user_version=3;",
        )
        .unwrap();
    assert!(Store::open(&root).is_err());
    assert_eq!(rows(&connection, "file_reviews"), original);
    assert_eq!(
        rows(&connection, "file_reviews_v3")[0][0],
        rusqlite::types::Value::Text("retain me".into())
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}

#[cfg(unix)]
#[test]
fn links_and_public_destination_permissions_are_refused() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (_directory, root) = project();
    drop(populated(&root));
    let public = root.join(".mct-index/public");
    fs::create_dir(&public).unwrap();
    fs::set_permissions(&public, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(create(&root, Some(&public)).is_err());
    let link = root.join(".mct-index/linked");
    symlink(&public, &link).unwrap();
    assert!(create(&root, Some(&link)).is_err());
    let bundle = create(&root, None).unwrap();
    let database = bundle.join(DATABASE);
    fs::remove_file(&database).unwrap();
    symlink(root.join(".mct-index").join(DATABASE), &database).unwrap();
    assert!(verify_restore(&bundle).is_err());
}

#[cfg(unix)]
#[test]
fn backup_directories_and_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, root) = project();
    drop(populated(&root));
    let bundle = create(&root, None).unwrap();
    for directory in [bundle.as_path(), bundle.parent().unwrap()] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for name in [DATABASE, MANIFEST] {
        assert_eq!(
            fs::metadata(bundle.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
