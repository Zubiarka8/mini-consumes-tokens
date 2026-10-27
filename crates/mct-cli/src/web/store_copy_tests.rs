//! Migration and independent-connection copy-label guarantees.
use super::*;

fn project() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join(".mct-index"))?;
    Ok(root)
}

fn label(key: char) -> FileReview {
    FileReview {
        path: "src/a.rs".into(),
        member_key: key.to_string().repeat(64),
        member_base: "b".repeat(64),
        code_hash: Some("c".repeat(64)),
        status: FileStatus::Accepted,
        note: "original note".into(),
        updated_at: 7,
    }
}

#[test]
fn copy_labels_v3_migration_preserves_every_legacy_row_and_reopens_v4() -> anyhow::Result<()> {
    let root = project()?;
    let path = root.path().join(".mct-index/dashboard.sqlite3");
    let mut conn = Connection::open(&path)?;
    Migrations::new(vec![M::up(SCHEMA), M::up(SCHEMA_FILE_REVIEWS), M::up(
        "ALTER TABLE reviews ADD COLUMN fingerprint_version INTEGER NOT NULL DEFAULT 1 CHECK(fingerprint_version IN (1, 2));"
    )]).to_latest(&mut conn)?;
    let identity = "a".repeat(64);
    for (path, status, note) in [
        ("src/a.rs", "accepted", "keep\nnotes"),
        ("src/b.rs", "rejected", "remove"),
        ("src/c.rs", "pending", "keep note"),
    ] {
        conn.execute(
            "INSERT INTO file_reviews VALUES (?1, ?2, ?3, ?4, 17)",
            params![identity, path, status, note],
        )?;
    }
    conn.execute(
        "INSERT INTO meta VALUES (?1, '9')",
        [file_revision_key(&identity, "src/a.rs")],
    )?;
    drop(conn);
    for _ in 0..2 {
        let store = Store::open(&root.path().canonicalize()?)?;
        let rows = store.file_reviews(&identity)?;
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (&rows[0].note, rows[0].status, rows[0].updated_at),
            (&"keep\nnotes".to_string(), FileStatus::Accepted, 17)
        );
        assert_eq!(
            (&rows[1].note, rows[1].status),
            (&"remove".to_string(), FileStatus::Rejected)
        );
        assert_eq!(
            (&rows[2].note, rows[2].status),
            (&"keep note".to_string(), FileStatus::Pending)
        );
        assert!(rows
            .iter()
            .all(|row| row.member_key.is_empty() && row.code_hash.is_none()));
        assert_eq!(store.file_revision(&identity, "src/a.rs")?, 9);
        let version: i64 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        assert_eq!(version, 4);
    }
    Ok(())
}

#[test]
fn copy_labels_fallback_overrides_and_two_connections_preserve_revisions() -> anyhow::Result<()> {
    let root = project()?;
    let identity = "a".repeat(64);
    let mut first = Store::open(&root.path().canonicalize()?)?;
    let mut second = Store::open(&root.path().canonicalize()?)?;
    first.set_file_review(&identity, "src/a.rs", FileStatus::Rejected, "legacy", 3)?;
    let mut a = label('1');
    let b = label('2');
    assert_eq!(
        second.member_revision(&identity, &a.path, &a.member_key)?,
        1
    );
    first.set_member_review(&identity, &a, 1)?;
    let conflict = second
        .set_member_review(&identity, &a, 1)
        .err()
        .context("expected a stale copy conflict")?;
    assert!(conflict.downcast_ref::<Conflict>().is_some());
    // Saving a different copy does not conflict, even within the same path.
    second.set_member_review(&identity, &b, 1)?;
    assert_eq!(first.file_reviews(&identity)?.len(), 3);
    a.status = FileStatus::Pending;
    a.note.clear();
    first.set_member_review(&identity, &a, 2)?;
    let rows = second.file_reviews(&identity)?;
    assert_eq!(
        rows.len(),
        3,
        "legacy row and explicit pending override survive"
    );
    assert!(rows
        .iter()
        .any(|row| row.member_key.is_empty() && row.note == "legacy"));
    assert!(rows.iter().any(|row| row.member_key == a.member_key
        && row.status == FileStatus::Pending
        && row.note.is_empty()));
    assert!(second
        .set_file_review_checked(&identity, &a.path, FileStatus::Accepted, "old client", 8, 1)
        .is_err());
    assert_eq!(first.member_revision(&identity, &a.path, &a.member_key)?, 3);
    assert!(first.decisions()?.is_empty());
    Ok(())
}

#[test]
fn copy_labels_clear_recreate_prevents_aba_and_quota_failure_is_atomic() -> anyhow::Result<()> {
    let root = project()?;
    let identity = "a".repeat(64);
    let mut store = Store::open(&root.path().canonicalize()?)?;
    let mut row = label('1');
    store.set_member_review(&identity, &row, 0)?;
    row.status = FileStatus::Pending;
    row.note.clear();
    store.set_member_review(&identity, &row, 1)?;
    assert!(store.file_reviews(&identity)?.is_empty());
    row.status = FileStatus::Rejected;
    assert!(store.set_member_review(&identity, &row, 0).is_err());
    store.set_member_review(&identity, &row, 2)?;
    assert_eq!(
        store.member_revision(&identity, &row.path, &row.member_key)?,
        3
    );
    store.conn.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x < 19999) INSERT INTO file_reviews(identity, path, status, note, updated_at) SELECT ?1, 'quota/' || x, 'accepted', '', 0 FROM n", [&identity])?;
    let extra = label('2');
    let before = store.decision_revision(Kind::Duplicate, &identity)?;
    assert!(store.set_member_review(&identity, &extra, 0).is_err());
    assert_eq!(
        store.member_revision(&identity, &extra.path, &extra.member_key)?,
        0
    );
    assert_eq!(store.decision_revision(Kind::Duplicate, &identity)?, before);
    assert_eq!(store.file_reviews(&identity)?.len(), 20000);
    store.set_member_review(&identity, &row, 3)?;
    Ok(())
}

#[test]
fn copy_labels_future_store_version_is_rejected_without_data_loss() -> anyhow::Result<()> {
    let root = project()?;
    let identity = "a".repeat(64);
    let mut store = Store::open(&root.path().canonicalize()?)?;
    store.set_file_review(&identity, "src/a.rs", FileStatus::Accepted, "retained", 7)?;
    store.conn.pragma_update(None, "user_version", 5)?;
    drop(store);
    assert!(Store::open(&root.path().canonicalize()?).is_err());
    let conn = Connection::open(root.path().join(".mct-index/dashboard.sqlite3"))?;
    let note: String = conn.query_row("SELECT note FROM file_reviews", [], |row| row.get(0))?;
    assert_eq!(note, "retained");
    Ok(())
}

#[test]
fn copy_labels_two_process_style_openers_can_race_migration() -> anyhow::Result<()> {
    let root = project()?;
    let path = root.path().canonicalize()?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let mut tasks = Vec::new();
        for at in 0..2 {
            let path = &path;
            let barrier = barrier.clone();
            tasks.push(scope.spawn(move || -> anyhow::Result<()> {
                barrier.wait();
                let mut store = Store::open(path)?;
                store.set_file_review(
                    &"a".repeat(64),
                    &format!("src/{at}.rs"),
                    FileStatus::Accepted,
                    "concurrent",
                    1,
                )
            }));
        }
        for task in tasks {
            task.join()
                .map_err(|_| anyhow::anyhow!("store opener thread failed"))??;
        }
        Ok(())
    })?;
    assert_eq!(Store::open(&path)?.file_reviews(&"a".repeat(64))?.len(), 2);
    Ok(())
}

#[test]
fn live_wal_snapshot_restores_all_user_tables_and_revisions() -> anyhow::Result<()> {
    fn contents(conn: &Connection) -> anyhow::Result<Vec<Vec<Vec<rusqlite::types::Value>>>> {
        let mut tables = Vec::new();
        for table in [
            "reviews",
            "review_history",
            "file_reviews",
            "comments",
            "meta",
            "sqlite_sequence",
        ] {
            let mut statement = conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
            let columns = statement.column_count();
            tables.push(
                statement
                    .query_map([], |row| {
                        (0..columns)
                            .map(|column| row.get(column))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        Ok(tables)
    }
    let source = project()?;
    let source_root = source.path().canonicalize()?;
    let path = source_root.join(".mct-index/dashboard.sqlite3");
    let mut store = Store::open(&source_root)?;
    store.conn.pragma_update(None, "wal_autocheckpoint", 0)?;
    store
        .conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let main_before = fs::read(&path)?;
    let identity = "a".repeat(64);
    store.set(
        (Kind::Duplicate, &identity, &"f".repeat(64)),
        Status::Confirmed,
        "group reason",
        10,
    )?;
    store.set_member_review(&identity, &label('1'), 0)?;
    store.add_comment(Kind::Duplicate, &identity, "saved discussion", 11)?;
    assert_eq!(
        fs::read(&path)?,
        main_before,
        "committed data must still be in WAL"
    );
    assert!(fs::metadata(path.with_extension("sqlite3-wal"))?.len() > 32);
    let expected = contents(&store.conn)?;

    let restored = project()?;
    let restored_root = restored.path().canonicalize()?;
    let backup = restored_root.join(".mct-index/dashboard.sqlite3");
    // A separate live SQLite connection reads committed WAL frames consistently.
    let reader = Connection::open(&path)?;
    reader.execute(
        "VACUUM main INTO ?1",
        [backup.to_str().context("UTF-8 backup path")?],
    )?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&backup)?
        .sync_all()?;
    // A later commit is outside this snapshot and must not appear after restore.
    store.add_comment(Kind::Duplicate, &identity, "after snapshot", 12)?;
    assert_ne!(contents(&store.conn)?, expected);
    assert!(
        reader
            .execute(
                "VACUUM main INTO ?1",
                [backup.to_str().context("UTF-8 backup path")?]
            )
            .is_err(),
        "a nonempty backup must never be overwritten"
    );

    let restored_store = Store::open(&restored_root)?;
    assert_eq!(contents(&restored_store.conn)?, expected);
    assert_eq!(
        restored_store.member_revision(&identity, "src/a.rs", &"1".repeat(64))?,
        1
    );
    let integrity: String =
        restored_store
            .conn
            .pragma_query_value(None, "integrity_check", |row| row.get(0))?;
    assert_eq!(integrity, "ok");
    assert!(!restored_store
        .conn
        .prepare("PRAGMA foreign_key_check")?
        .exists([])?);
    assert_eq!(
        restored_store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
        4
    );
    Ok(())
}

#[test]
fn every_explicit_copy_transition_is_independent_of_the_finding_decision() -> anyhow::Result<()> {
    let root = project()?;
    let mut store = Store::open(&root.path().canonicalize()?)?;
    let identity = "d".repeat(64);
    let fp = "f".repeat(64);
    store.set(
        (Kind::Duplicate, &identity, &fp),
        Status::FalsePositive,
        "group",
        1,
    )?;
    let statuses = [
        FileStatus::Pending,
        FileStatus::Accepted,
        FileStatus::Rejected,
        FileStatus::InReview,
        FileStatus::Ignored,
    ];
    let mut row = label('a');
    let mut count = 0;
    for from in statuses {
        for to in statuses {
            for status in [from, to] {
                row.status = status;
                row.note = format!("independent {count}");
                let revision = store.member_revision(&identity, &row.path, &row.member_key)?;
                store.set_member_review(&identity, &row, revision)?;
                let saved = store.file_reviews(&identity)?;
                assert_eq!(saved.len(), 1);
                assert_eq!(saved[0].status, status);
                assert_eq!(saved[0].note, row.note);
                assert!(matches!(
                    store.state(Kind::Duplicate, &identity, &fp)?,
                    State::Decided {
                        status: Status::FalsePositive,
                        ..
                    }
                ));
                count += 1;
            }
        }
    }
    assert_eq!(count, 50);
    assert_eq!(store.history(Kind::Duplicate, &identity)?.len(), 1);
    Ok(())
}

#[test]
fn finding_detail_snapshot_keeps_old_revision_with_old_decision_and_comments() -> anyhow::Result<()>
{
    let root = project()?;
    let reader = Store::open(&root.path().canonicalize()?)?;
    let mut writer = Store::open(&root.path().canonicalize()?)?;
    let (identity, fp) = ("d".repeat(64), "f".repeat(64));
    writer.set((Kind::Dead, &identity, &fp), Status::Ignored, "old", 1)?;
    let snapshot = reader.read_snapshot()?;
    let old_revision = reader.decision_revision(Kind::Dead, &identity)?;
    writer.set_checked(
        (Kind::Dead, &identity, &fp),
        Status::Confirmed,
        "new",
        2,
        old_revision,
    )?;
    writer.add_comment(Kind::Dead, &identity, "concurrent comment", 3)?;
    assert_eq!(
        reader.decision_revision(Kind::Dead, &identity)?,
        old_revision
    );
    assert_eq!(
        reader
            .saved_decision(Kind::Dead, &identity)?
            .map(|r| r.status),
        Some(Status::Ignored)
    );
    assert_eq!(reader.history(Kind::Dead, &identity)?.len(), 1);
    assert!(reader.comments(Kind::Dead, &identity)?.is_empty());
    drop(snapshot);
    assert_eq!(
        reader
            .saved_decision(Kind::Dead, &identity)?
            .map(|r| r.status),
        Some(Status::Confirmed)
    );
    assert_eq!(reader.comments(Kind::Dead, &identity)?.len(), 1);
    let stale = writer.set_checked(
        (Kind::Dead, &identity, &fp),
        Status::Ignored,
        "stale",
        4,
        old_revision,
    );
    assert!(stale.is_err());
    Ok(())
}

#[test]
fn failed_history_insert_rolls_back_decision_revision_and_metadata() -> anyhow::Result<()> {
    let root = project()?;
    let mut store = Store::open(&root.path().canonicalize()?)?;
    let (identity, fp) = ("d".repeat(64), "f".repeat(64));
    store.set((Kind::Dead, &identity, &fp), Status::Ignored, "before", 1)?;
    let revision = store.decision_revision(Kind::Dead, &identity)?;
    let token = store.change_token()?;
    store.conn.execute_batch(
        "CREATE TEMP TRIGGER fail_history BEFORE INSERT ON review_history
        BEGIN SELECT RAISE(ABORT, 'injected history failure'); END;",
    )?;
    let result = store.set_checked(
        (Kind::Dead, &identity, &fp),
        Status::Confirmed,
        "after",
        2,
        revision,
    );
    assert!(result.is_err());
    assert_eq!(store.decision_revision(Kind::Dead, &identity)?, revision);
    assert_eq!(store.change_token()?, token);
    assert_eq!(store.history(Kind::Dead, &identity)?.len(), 1);
    assert_eq!(
        store
            .saved_decision(Kind::Dead, &identity)?
            .map(|r| (r.status, r.reason)),
        Some((Status::Ignored, "before".into()))
    );
    store.conn.execute_batch("DROP TRIGGER fail_history;")?;
    store.set_checked(
        (Kind::Dead, &identity, &fp),
        Status::Confirmed,
        "retry",
        3,
        revision,
    )?;
    assert_eq!(store.history(Kind::Dead, &identity)?.len(), 2);
    Ok(())
}

#[test]
fn explicit_finding_transitions_preserve_comments_and_measure_bounded_reads() -> anyhow::Result<()>
{
    use std::time::Instant;
    let root = project()?;
    let mut store = Store::open(&root.path().canonicalize()?)?;
    let (identity, fp) = ("d".repeat(64), "f".repeat(64));
    store.add_comment(Kind::Dead, &identity, "retained rationale", 1)?;
    let statuses = [
        Status::Pending,
        Status::Confirmed,
        Status::FalsePositive,
        Status::Ignored,
    ];
    let mut writes = Vec::new();
    let mut reads = Vec::new();
    let pages_before: i64 = store
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))?;
    for from in statuses {
        for to in statuses {
            for status in [from, to] {
                let revision = store.decision_revision(Kind::Dead, &identity)?;
                let started = Instant::now();
                store.set_checked(
                    (Kind::Dead, &identity, &fp),
                    status,
                    "reviewed",
                    2,
                    revision,
                )?;
                writes.push(started.elapsed().as_secs_f64() * 1_000_000.0);
                let started = Instant::now();
                let snapshot = store.read_snapshot()?;
                let saved = store.saved_decision(Kind::Dead, &identity)?;
                let state = store.state(Kind::Dead, &identity, &fp)?;
                let _revision = store.decision_revision(Kind::Dead, &identity)?;
                let _history = store.history(Kind::Dead, &identity)?;
                let comments = store.comments(Kind::Dead, &identity)?;
                drop(snapshot);
                reads.push(started.elapsed().as_secs_f64() * 1_000_000.0);
                if status == Status::Pending {
                    assert!(matches!(state, State::Pending));
                } else {
                    assert_eq!(saved.map(|r| r.status), Some(status));
                    assert!(matches!(state, State::Decided { status: s, .. } if s == status));
                }
                assert_eq!(comments.len(), 1);
                assert_eq!(comments[0].body, "retained rationale");
            }
        }
    }
    let pages_after: i64 = store
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = store.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    assert_eq!(writes.len(), 32);
    assert_eq!(reads.len(), 32);
    writes.sort_by(f64::total_cmp);
    reads.sort_by(f64::total_cmp);
    eprintln!("phase6_store_fixture:\n  samples: 32\n  write_median_us: {:.3}\n  write_p95_us: {:.3}\n  detail_median_us: {:.3}\n  detail_p95_us: {:.3}\n  logical_growth_bytes: {}",
        (writes[15] + writes[16]) / 2.0, writes[30],
        (reads[15] + reads[16]) / 2.0, reads[30], pages_after.saturating_sub(pages_before) * page_size);
    Ok(())
}

#[test]
fn comment_compare_and_swap_is_atomic_across_connections_and_rolls_back() -> anyhow::Result<()> {
    let root = project()?;
    let physical = root.path().canonicalize()?;
    let mut a = Store::open(&physical)?;
    let mut b = Store::open(&physical)?;
    let id = "a".repeat(64);
    let comment = a.add_comment(Kind::Dead, &id, "original", 1)?;
    let seen = b.decision_revision(Kind::Dead, &id)?;
    assert!(a.update_comment(comment, (Kind::Dead, &id, seen), "newer", 2)?);
    let current = a.decision_revision(Kind::Dead, &id)?;
    for result in [
        b.update_comment(comment, (Kind::Dead, &id, seen), "stale", 3),
        b.delete_comment(comment, (Kind::Dead, &id, seen)),
    ] {
        assert!(result.is_err_and(|e| e.downcast_ref::<Conflict>().is_some()));
    }
    assert!(!b.delete_comment(comment, (Kind::Duplicate, &id, current))?);
    assert_eq!(b.comments(Kind::Dead, &id)?[0].body, "newer");
    assert_eq!(b.decision_revision(Kind::Dead, &id)?, current);
    a.conn.execute_batch("CREATE TEMP TRIGGER fail_comment_revision BEFORE INSERT ON meta BEGIN SELECT RAISE(ABORT, 'injected revision failure'); END;")?;
    assert!(a
        .update_comment(comment, (Kind::Dead, &id, current), "rollback", 4)
        .is_err());
    assert!(a
        .delete_comment(comment, (Kind::Dead, &id, current))
        .is_err());
    assert_eq!(b.comments(Kind::Dead, &id)?[0].body, "newer");
    assert_eq!(b.decision_revision(Kind::Dead, &id)?, current);
    a.conn
        .execute_batch("DROP TRIGGER fail_comment_revision;")?;
    assert!(a.delete_comment(comment, (Kind::Dead, &id, current))?);
    assert_eq!(b.decision_revision(Kind::Dead, &id)?, current + 1);
    Ok(())
}

#[test]
fn concurrent_comment_appends_preserve_both_rows_and_aggregate_revisions() -> anyhow::Result<()> {
    let root = project()?;
    let physical = root.path().canonicalize()?;
    let a = Store::open(&physical)?;
    let b = Store::open(&physical)?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = [a, b]
        .into_iter()
        .enumerate()
        .map(|(n, mut store)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || -> anyhow::Result<i64> {
                barrier.wait();
                store.add_comment(Kind::Dead, &"a".repeat(64), &format!("note {n}"), 1)
            })
        })
        .collect();
    let mut ids = Vec::new();
    for thread in threads {
        ids.push(
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("comment test worker failed"))??,
        );
    }
    assert_ne!(ids[0], ids[1]);
    let reader = Store::open(&physical)?;
    let mut bodies: Vec<_> = reader
        .comments(Kind::Dead, &"a".repeat(64))?
        .into_iter()
        .map(|c| c.body)
        .collect();
    bodies.sort();
    assert_eq!(bodies, ["note 0", "note 1"]);
    assert_eq!(reader.decision_revision(Kind::Dead, &"a".repeat(64))?, 2);
    Ok(())
}
