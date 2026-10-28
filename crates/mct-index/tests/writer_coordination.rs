//! Coordination gates must serialize writers without excluding readers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::{coordination::DatabaseAccess, ExcludeSet, Index, IndexError};
use std::time::Duration;

#[test]
fn writers_exclude_each_other_and_release_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("index.sqlite3");
    let first = DatabaseAccess::shared(&db, Duration::ZERO).unwrap();
    let second = DatabaseAccess::shared(&db, Duration::ZERO).unwrap();
    let writer = first.writer(Duration::ZERO).unwrap();
    let error = second.writer(Duration::ZERO).unwrap_err();
    assert!(
        error.to_string().contains("coordination timeout"),
        "{error}"
    );
    drop(writer);
    assert!(second.writer(Duration::ZERO).is_ok());
}

#[test]
fn maintenance_requires_readers_to_close_and_blocks_new_readers() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("index.sqlite3");
    let reader = DatabaseAccess::shared(&db, Duration::ZERO).unwrap();
    assert!(DatabaseAccess::maintenance(&db, Duration::ZERO).is_err());
    drop(reader);
    let maintenance = DatabaseAccess::maintenance(&db, Duration::ZERO).unwrap();
    assert!(DatabaseAccess::shared(&db, Duration::ZERO).is_err());
    drop(maintenance);
    assert!(DatabaseAccess::shared(&db, Duration::ZERO).is_ok());
}

#[test]
fn separate_databases_do_not_share_writer_locks() {
    let root = tempfile::tempdir().unwrap();
    let first = DatabaseAccess::shared(&root.path().join("first.sqlite3"), Duration::ZERO).unwrap();
    let second =
        DatabaseAccess::shared(&root.path().join("second.sqlite3"), Duration::ZERO).unwrap();
    let _writer = first.writer(Duration::ZERO).unwrap();
    assert!(second.writer(Duration::ZERO).is_ok());
}

#[test]
fn index_mutations_fail_on_contention_without_publishing_then_recover() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("index.sqlite3");
    let mut index = Index::open(root.path(), &db, ExcludeSet::default()).unwrap();
    let access = DatabaseAccess::shared(&db, Duration::ZERO).unwrap();
    let writer = access.writer(Duration::ZERO).unwrap();
    let generation = index.published_generation().unwrap();
    let registry = mct_core::LanguageRegistry::new();
    assert!(matches!(
        index.reindex(&registry, false),
        Err(IndexError::Coordination(_))
    ));
    assert_eq!(index.published_generation().unwrap(), generation);
    drop(writer);
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.published_generation().unwrap(), generation + 1);
}

#[cfg(unix)]
#[test]
fn linked_coordination_files_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("index.sqlite3");
    let target = root.path().join("target");
    std::fs::write(&target, "fixture").unwrap();
    let access = DatabaseAccess::shared(&db, Duration::ZERO).unwrap();
    let lock = root.path().join("index.sqlite3.writer.lock");
    std::os::unix::fs::symlink(&target, &lock).unwrap();
    assert!(access.writer(Duration::ZERO).is_err());
    std::fs::remove_file(&lock).unwrap();
    std::fs::hard_link(&target, &lock).unwrap();
    assert!(access.writer(Duration::ZERO).is_err());
}
