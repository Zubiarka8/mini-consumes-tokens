#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::web::store::Store;

fn fixture() -> (tempfile::TempDir, PathBuf, Policy) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join(".mct-index")).unwrap();
    drop(Store::open(&root).unwrap());
    let policy = Policy {
        enabled: true,
        destination: None,
        interval_seconds: 60,
        max_count: 3,
        max_age_seconds: 0,
        max_bytes: 1024 * 1024 * 1024,
        min_verified: 2,
        verify_seconds: 60,
    };
    (dir, root, policy)
}
fn dest(root: &Path) -> PathBuf {
    root.join(".mct-index/backups")
}
fn entries(root: &Path) -> Vec<Entry> {
    catalog(&dest(root), &source_id(root).unwrap(), true).unwrap()
}

#[test]
fn due_restart_missed_schedule_and_disabled_are_deterministic() {
    let (_dir, root, mut p) = fixture();
    p.enabled = false;
    run_at(&root, &p, false, 100).unwrap();
    assert!(!dest(&root).exists());
    p.enabled = true;
    run_at(&root, &p, false, 100).unwrap();
    run_at(&root, &p, false, 100).unwrap();
    run_at(&root, &p, false, 159).unwrap();
    assert_eq!(entries(&root).len(), 1);
    run_at(&root, &p, false, 10000).unwrap();
    assert_eq!(entries(&root).len(), 2);
    assert_eq!(
        State::read(&dest(&root), &source_id(&root).unwrap())
            .unwrap()
            .success,
        10000
    );
    assert!(run_at(&root, &p, false, 9999).is_err());
}

#[test]
fn failed_creation_backoff_preserves_previous_backups() {
    let (_dir, root, p) = fixture();
    run_at(&root, &p, false, 100).unwrap();
    fs::rename(
        root.join(".mct-index/dashboard.sqlite3"),
        root.join("saved.sqlite3"),
    )
    .unwrap();
    assert!(run_at(&root, &p, false, 160).is_err());
    assert_eq!(entries(&root).iter().filter(|e| e.valid).count(), 1);
    let state = State::read(&dest(&root), &source_id(&root).unwrap()).unwrap();
    assert_eq!(state.retry, 460);
    run_at(&root, &p, false, 200).unwrap();
    assert_eq!(
        State::read(&dest(&root), &source_id(&root).unwrap())
            .unwrap()
            .failures,
        1
    );
    assert!(run_at(&root, &p, false, 460).is_err());
    assert_eq!(
        State::read(&dest(&root), &source_id(&root).unwrap())
            .unwrap()
            .retry,
        1060
    );
}

#[test]
fn retention_protects_minimum_preserved_and_unknown_files() {
    let (_dir, root, mut p) = fixture();
    for now in [100, 160, 220] {
        run_at(&root, &p, false, now).unwrap();
    }
    let old = entries(&root);
    fs::write(old[0].path.join("preserve"), "operator pinned").unwrap();
    fs::write(old[1].path.join("user-file"), "do not delete").unwrap();
    p.max_count = 2;
    p.max_bytes = 1;
    assert!(run_at(&root, &p, false, 280).is_err());
    assert_eq!(entries(&root).len(), 3);
    assert!(old[0].path.join("preserve").exists());
    assert!(old[1].path.join("user-file").exists());
    assert!(!retain(&dest(&root), &source_id(&root).unwrap(), &p, 280).unwrap());
    assert!(entries(&root).iter().filter(|e| e.valid).count() >= p.min_verified);
}

#[test]
fn retention_count_and_age_remove_only_old_owned_verified_points() {
    let (_dir, root, mut p) = fixture();
    for now in [100, 160, 220, 280, 340] {
        run_at(&root, &p, false, now).unwrap();
    }
    assert_eq!(entries(&root).len(), 3);
    // Manual bundles have no receipt, so retention must never delete them.
    let manual = backup::create(&root, None).unwrap();
    p.max_count = 2;
    p.max_age_seconds = 1;
    assert!(!retain(&dest(&root), &source_id(&root).unwrap(), &p, u64::MAX / 2).unwrap());
    assert!(manual.exists());
    assert_eq!(
        entries(&root).iter().filter(|e| e.owned && e.valid).count(),
        2
    );
}

#[test]
fn incomplete_corrupt_and_missing_are_not_recovery_points() {
    let (_dir, root, p) = fixture();
    run_at(&root, &p, false, 100).unwrap();
    let point = entries(&root).remove(0).path;
    fs::write(point.join("dashboard.sqlite3"), "broken").unwrap();
    let incomplete = dest(&root).join("dashboard-incomplete");
    fs::create_dir(&incomplete).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&incomplete, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(entries(&root).iter().filter(|e| e.valid).count(), 0);
    assert!(run_at(&root, &p, true, 160).is_err());
    assert!(point.exists() && incomplete.exists());
}

#[test]
fn published_before_state_commit_is_adopted_without_duplicate() {
    let (_dir, root, p) = fixture();
    let destination = backup::prepare_destination(&root, None).unwrap();
    let owner = source_id(&root).unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    State {
        attempt: now,
        phase: "running".into(),
        ..State::default()
    }
    .write(&destination, &owner)
    .unwrap();
    backup::create_managed(&root, &destination, Some((&owner, now))).unwrap();
    run_at(&root, &p, false, now + 1).unwrap();
    assert_eq!(entries(&root).len(), 1);
    assert_eq!(State::read(&destination, &owner).unwrap().success, now + 1);
}

#[test]
fn periodic_verification_failure_does_not_run_retention() {
    let (_dir, root, mut p) = fixture();
    p.interval_seconds = 10000;
    run_at(&root, &p, false, 100).unwrap();
    let point = entries(&root).remove(0).path;
    fs::write(point.join("dashboard.sqlite3"), "corrupt").unwrap();
    assert!(run_at(&root, &p, false, 160).is_err());
    assert!(point.exists());
    assert_eq!(
        State::read(&dest(&root), &source_id(&root).unwrap())
            .unwrap()
            .verified,
        100
    );
}

#[test]
fn isolated_restore_preserves_rows_and_refuses_existing_destination() {
    let (_dir, root, p) = fixture();
    let conn = rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3")).unwrap();
    conn.execute("INSERT INTO meta VALUES ('restore-test','preserved')", [])
        .unwrap();
    run_at(&root, &p, false, 100).unwrap();
    let point = entries(&root).remove(0).path;
    let restore = root.join("isolated");
    backup::restore_into(&point, &restore).unwrap();
    let conn = rusqlite::Connection::open(restore.join("dashboard.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT value FROM meta WHERE key='restore-test'", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "preserved"
    );
    assert!(backup::restore_into(&point, &restore).is_err());
}

#[test]
fn destination_lock_and_source_identity_are_enforced() {
    let (_dir, root, p) = fixture();
    run_at(&root, &p, false, 100).unwrap();
    let access = mct_index::coordination::DatabaseAccess::shared(
        &dest(&root).join("management"),
        Duration::ZERO,
    )
    .unwrap();
    let guard = access.writer(Duration::ZERO).unwrap();
    assert!(run_at(&root, &p, false, 160).is_err());
    drop(guard);
    drop(access);
    let (_other, other, mut p2) = fixture();
    p2.destination = Some(dest(&root));
    assert!(run_at(&other, &p2, false, 160).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_bundle_and_destination_are_rejected() {
    use std::os::unix::fs::symlink;
    let (_dir, root, mut p) = fixture();
    run_at(&root, &p, false, 100).unwrap();
    symlink(
        root.join(".mct-index"),
        dest(&root).join("dashboard-linked"),
    )
    .unwrap();
    assert!(run_at(&root, &p, true, 160).is_err());
    symlink(dest(&root), root.join("linked")).unwrap();
    p.destination = Some(root.join("linked"));
    assert!(run_at(&root, &p, false, 160).is_err());
}

#[test]
fn policy_rejects_zero_minimum_and_aggressive_intervals() {
    let (_dir, _root, mut p) = fixture();
    p.min_verified = 0;
    assert!(validate_policy(&p).is_err());
    p.min_verified = 2;
    p.interval_seconds = 1;
    assert!(validate_policy(&p).is_err());
}

#[test]
fn injected_disk_full_and_verification_failure_retain_previous_points() {
    let (_dir, root, p) = fixture();
    run_at(&root, &p, false, 100).unwrap();
    let first = entries(&root).remove(0).path;
    for (stage, now) in [("snapshot", 160), ("verification", 460)] {
        backup::FAULT.with(|f| f.set(stage));
        let result = run_at(&root, &p, false, now);
        backup::FAULT.with(|f| f.set(""));
        assert!(result.is_err());
        backup::verify_restore(&first).unwrap();
        assert_eq!(entries(&root).iter().filter(|e| e.valid).count(), 1);
    }
}

#[test]
fn subprocess_backup_helper() {
    let Some(root) = std::env::var_os("MCT_BACKUP_TEST_ROOT") else {
        return;
    };
    let p = Policy {
        enabled: true,
        destination: None,
        interval_seconds: 86400,
        max_count: 7,
        max_age_seconds: 0,
        max_bytes: 1073741824,
        min_verified: 2,
        verify_seconds: 604800,
    };
    run(Path::new(&root), &p, false).unwrap();
}

struct Child {
    child: std::process::Child,
    output: std::io::BufReader<std::process::ChildStdout>,
    barrier: Option<mct_index::coordination::Guard>,
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn child(root: &Path, pause: Option<&str>) -> Child {
    use std::process::{Command, Stdio};
    let barrier = pause.map(|_| {
        let access = mct_index::coordination::DatabaseAccess::shared(
            &root.join(".mct-index/test-barrier"),
            Duration::ZERO,
        )
        .unwrap();
        access.writer(Duration::ZERO).unwrap()
    });
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "web::backup_manager::tests::subprocess_backup_helper",
        "--nocapture",
    ])
    .env("MCT_BACKUP_TEST_ROOT", root)
    .env_remove("MCT_BACKUP_TEST_PAUSE")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    if let Some(stage) = pause {
        cmd.env("MCT_BACKUP_TEST_PAUSE", stage);
    }
    let mut child = cmd.spawn().unwrap();
    let output = std::io::BufReader::new(child.stdout.take().unwrap());
    Child {
        child,
        output,
        barrier,
    }
}
fn ready(child: &mut Child) {
    use std::io::BufRead;
    loop {
        let mut line = String::new();
        assert!(
            child.output.read_line(&mut line).unwrap() > 0,
            "child exited before barrier"
        );
        if line.contains("BACKUP_TEST_READY") {
            return;
        }
    }
}

#[test]
fn two_process_schedulers_and_retention_share_one_owner() {
    let (_dir, root, _p) = fixture();
    let mut first = child(&root, Some("before-publication"));
    ready(&mut first);
    let mut contender = child(&root, None);
    assert!(!contender.child.wait().unwrap().success());
    drop(first.barrier.take());
    assert!(first.child.wait().unwrap().success());
    let mut next = child(&root, None);
    assert!(next.child.wait().unwrap().success());
    assert_eq!(entries(&root).iter().filter(|e| e.valid).count(), 1);
}

#[test]
fn process_termination_before_and_after_publication_is_recoverable() {
    for stage in ["before-publication", "after-publication"] {
        let (_dir, root, p) = fixture();
        let mut worker = child(&root, Some(stage));
        ready(&mut worker);
        worker.child.kill().unwrap();
        worker.child.wait().unwrap();
        let before = entries(&root).iter().filter(|e| e.valid).count();
        assert_eq!(before, usize::from(stage == "after-publication"));
        let mut restarted = child(&root, None);
        assert!(restarted.child.wait().unwrap().success());
        assert_eq!(entries(&root).iter().filter(|e| e.valid).count(), 1);
        run(&root, &p, true).unwrap();
    }
}

#[test]
fn benchmark_backup_lifecycle_fixture() {
    use std::time::Instant;
    let (_dir, root, p) = fixture();
    let conn = rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3")).unwrap();
    // Approximately 1 MiB persistent user metadata, fixed deterministic rows.
    for i in 0..256 {
        conn.execute(
            "INSERT INTO meta VALUES (?1,?2)",
            rusqlite::params![format!("fixture-{i}"), "x".repeat(4096)],
        )
        .unwrap();
    }
    let destination = backup::prepare_destination(&root, None).unwrap();
    let mut backup_us = Vec::new();
    let mut verify_us = Vec::new();
    let mut tick_us = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let point = backup::create(&root, None).unwrap();
        backup_us.push(start.elapsed().as_micros());
        let start = Instant::now();
        backup::verify_restore(&point).unwrap();
        verify_us.push(start.elapsed().as_micros());
    }
    run_at(&root, &p, false, 100).unwrap();
    for _ in 0..20 {
        let start = Instant::now();
        run_at(&root, &p, false, 101).unwrap();
        tick_us.push(start.elapsed().as_micros());
    }
    let start = Instant::now();
    retain(&destination, &source_id(&root).unwrap(), &p, 101).unwrap();
    let retain_us = start.elapsed().as_micros();
    backup_us.sort();
    verify_us.sort();
    tick_us.sort();
    println!("BACKUP_BENCH backup_p50_us={} verify_p50_us={} idle_tick_p50_us={} retention_us={} bundle_bytes={}",backup_us[2],verify_us[2],tick_us[10],retain_us,entries(&root)[0].bytes);
}

#[test]
fn inaccessible_destination_and_wrong_schema_fail_without_prior_data_loss() {
    let (_dir, root, mut p) = fixture();
    let destination = root.join("not-a-directory");
    fs::write(&destination, "keep").unwrap();
    p.destination = Some(destination.clone());
    assert!(run_at(&root, &p, false, 100).is_err());
    assert_eq!(fs::read_to_string(destination).unwrap(), "keep");
    p.destination = None;
    run_at(&root, &p, false, 100).unwrap();
    let point = entries(&root).remove(0).path;
    let conn = rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3")).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(run_at(&root, &p, false, 160).is_err());
    backup::verify_restore(&point).unwrap();
}

#[test]
fn mcp_and_indexing_remain_correct_during_backup_workload() {
    use mct_index::{ExcludeSet, Index};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    };
    use std::time::Instant;
    let (_dir, root, _p) = fixture();
    let conn = rusqlite::Connection::open(root.join(".mct-index/dashboard.sqlite3")).unwrap();
    for n in 0..256 {
        conn.execute(
            "INSERT INTO meta VALUES (?1,?2)",
            rusqlite::params![format!("load-{n}"), "x".repeat(4096)],
        )
        .unwrap();
    }
    for n in 0..100 {
        fs::write(
            root.join(format!("source{n}.rs")),
            format!("pub fn backup_measured_{n}() -> usize {{ {n} }}\n"),
        )
        .unwrap();
    }
    let db = root.join(".mct-index/index.sqlite3");
    let registry = mct_languages::build_registry();
    let mut index = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let server = mct_mcp_server::server::MctServer::new(
        Index::open(&root, &db, ExcludeSet::default()).unwrap(),
        mct_languages::build_registry(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut args = serde_json::Map::new();
    args.insert(
        "name".into(),
        serde_json::Value::String("backup_measured_50".into()),
    );
    let query = || {
        let result = runtime
            .block_on(server.call_tool_by_name("find_symbol", Some(args.clone())))
            .unwrap();
        let serialized = serde_json::to_value(result).unwrap();
        assert_ne!(
            serialized.get("isError").and_then(|v| v.as_bool()),
            Some(true)
        );
        let text = crate::result_text(&serialized);
        assert!(text.contains("backup_measured_50"));
        text
    };
    let expected = query();
    let mut measure = |label: &str| {
        let mut queries = Vec::new();
        let mut updates = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            for _ in 0..10 {
                assert_eq!(query(), expected);
            }
            queries.push(start.elapsed().as_micros());
            let start = Instant::now();
            index.reindex(&registry, false).unwrap();
            updates.push(start.elapsed().as_micros());
        }
        queries.sort();
        updates.sort();
        println!("LOAD_BENCH {label} ten_mcp_calls_p50_us={} ten_mcp_calls_p95_us={} unchanged_index_p50_us={} unchanged_index_p95_us={}",queries[10],queries[19],updates[10],updates[19]);
    };
    measure("without_backup");
    let barrier = Arc::new(Barrier::new(2));
    let stop = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let barrier2 = Arc::clone(&barrier);
        let stop2 = Arc::clone(&stop);
        let root = &root;
        let handle = scope.spawn(move || {
            barrier2.wait();
            let mut completed = 0;
            loop {
                backup::create(root, None).unwrap();
                completed += 1;
                if stop2.load(Ordering::Acquire) {
                    return completed;
                }
            }
        });
        barrier.wait();
        measure("with_backup");
        stop.store(true, Ordering::Release);
        assert!(handle.join().unwrap() >= 1);
    });
}

#[test]
fn active_wal_writes_remain_atomic_inside_snapshots() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    };
    let (_dir, root, _p) = fixture();
    let db = root.join(".mct-index/dashboard.sqlite3");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("INSERT INTO meta VALUES ('counter-a','0'),('counter-b','0');")
        .unwrap();
    drop(conn);
    let stop = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let stop2 = Arc::clone(&stop);
        let barrier2 = Arc::clone(&barrier);
        let writer = scope.spawn(move || {
            let mut conn = rusqlite::Connection::open(db).unwrap();
            conn.busy_timeout(Duration::from_secs(5)).unwrap();
            barrier2.wait();
            let mut count = 0;
            while !stop2.load(Ordering::Acquire) {
                let tx = conn.transaction().unwrap();
                for key in ["counter-a", "counter-b"] {
                    tx.execute(
                        "UPDATE meta SET value=?1 WHERE key=?2",
                        rusqlite::params![count.to_string(), key],
                    )
                    .unwrap();
                }
                tx.commit().unwrap();
                count += 1;
            }
            count
        });
        barrier.wait();
        for _ in 0..3 {
            let point = backup::create(&root, None).unwrap();
            let conn = backup::completed_snapshot(&point).unwrap();
            let a: String = conn
                .query_row("SELECT value FROM meta WHERE key='counter-a'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            let b: String = conn
                .query_row("SELECT value FROM meta WHERE key='counter-b'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(a, b);
        }
        stop.store(true, Ordering::Release);
        assert!(writer.join().unwrap() > 0);
    });
}

#[test]
fn recovered_publication_applies_missed_retention_and_malformed_receipts_are_protected() {
    let (_dir, root, p) = fixture();
    for now in [100, 160, 220] {
        run_at(&root, &p, false, now).unwrap();
    }
    let destination = dest(&root);
    let owner = source_id(&root).unwrap();
    let mut state = State::read(&destination, &owner).unwrap();
    state.attempt = 280;
    state.phase = "running".into();
    state.write(&destination, &owner).unwrap();
    backup::create_managed(&root, &destination, Some((&owner, 280))).unwrap();
    assert_eq!(entries(&root).len(), 4);
    run_at(&root, &p, false, 281).unwrap();
    assert_eq!(entries(&root).len(), 3);
    let point = entries(&root).remove(0).path;
    let receipt = fs::read_to_string(point.join(RECEIPT))
        .unwrap()
        .replace("attempt: 160", "attempt: broken");
    fs::write(point.join(RECEIPT), receipt).unwrap();
    assert!(!receipt_matches(&point, &owner));
    assert!(point.exists());
}
