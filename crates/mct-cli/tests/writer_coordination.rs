//! Real-process ownership regressions. Fixtures never open a production store.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::{coordination::DatabaseAccess, ExcludeSet, Index};
use rusqlite::Connection;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const NOW: Duration = Duration::ZERO;

struct Worker {
    child: Child,
    _output: BufReader<std::process::ChildStdout>,
}
impl Worker {
    fn spawn(path: &Path, mode: &str) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "subprocess_worker", "--nocapture"])
            .env("MCT_LOCK_TEST_PATH", path)
            .env("MCT_LOCK_TEST_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let mut output = BufReader::new(output);
        let mut line = String::new();
        while output.read_line(&mut line).unwrap() != 0 {
            if line.trim() == "READY" {
                return Self {
                    child,
                    _output: output,
                };
            }
            line.clear();
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("child did not acquire its lock");
    }
    fn finish(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"go\n")
            .unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn subprocess_worker() {
    let Some(path) = std::env::var_os("MCT_LOCK_TEST_PATH") else {
        return;
    };
    let path = Path::new(&path);
    let mode = std::env::var("MCT_LOCK_TEST_MODE").unwrap();
    let access = DatabaseAccess::shared(path, NOW).unwrap();
    let _writer = if mode == "writer" {
        Some(access.writer(NOW).unwrap())
    } else {
        None
    };
    let mut index = if mode == "index" {
        Some(
            Index::open(
                path.parent().unwrap().parent().unwrap(),
                path,
                ExcludeSet::default(),
            )
            .unwrap(),
        )
    } else {
        None
    };
    let _reader = if mode == "reader" {
        Some(Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap())
    } else {
        None
    };
    println!("READY");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    if let Some(index) = index.as_mut() {
        match index.reindex(&mct_languages::build_registry(), false) {
            Ok(_) => (),
            Err(mct_index::IndexError::Coordination(message))
                if message.contains("coordination timeout") =>
            {
                println!("CONTENDED")
            }
            Err(error) => panic!("unexpected indexing failure: {error}"),
        }
    }
}

#[test]
fn same_database_writer_contends_but_readers_and_other_projects_continue() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let path = a.path().join("state.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; CREATE TABLE sample(value); INSERT INTO sample VALUES(7)",
    )
    .unwrap();
    let mut holder = Worker::spawn(&path, "writer");
    let access = DatabaseAccess::shared(&path, NOW).unwrap();
    let start = Instant::now();
    let error = access.writer(Duration::from_millis(30)).unwrap_err();
    assert!(error.to_string().contains("coordination timeout"));
    assert!(start.elapsed() < Duration::from_secs(2));
    let other = DatabaseAccess::shared(&b.path().join("state.sqlite3"), NOW).unwrap();
    let _other = other.writer(NOW).unwrap();
    let reader =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT value FROM sample", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert!(DatabaseAccess::maintenance(&path, NOW).is_err());
    holder.finish();
    assert!(access.writer(NOW).is_ok());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sample", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn killed_process_releases_os_ownership_without_deleting_lock_files() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("state.sqlite3");
    let mut holder = Worker::spawn(&path, "writer");
    let access = DatabaseAccess::shared(&path, NOW).unwrap();
    assert!(access.writer(NOW).is_err());
    holder.child.kill().unwrap();
    holder.child.wait().unwrap();
    assert!(access.writer(NOW).is_ok());
    drop(access);
    assert!(DatabaseAccess::maintenance(&path, NOW).is_ok());
    assert!(root.path().join("state.sqlite3.access.lock").exists());
    assert!(root.path().join("state.sqlite3.writer.lock").exists());
}

#[test]
fn connected_process_prevents_migration_and_replacement() {
    use rusqlite_migration::{Migrations, M};
    let root = TempDir::new().unwrap();
    let path = root.path().join("state.sqlite3");
    let migrations = Migrations::new(vec![M::up("CREATE TABLE sample(value)")]);
    let (conn, access) = DatabaseAccess::open(&path, &migrations).unwrap();
    drop(conn);
    drop(access);
    let mut reader = Worker::spawn(&path, "reader");
    let future = Migrations::new(vec![
        M::up("CREATE TABLE sample(value)"),
        M::up("ALTER TABLE sample ADD COLUMN second"),
    ]);
    let error = DatabaseAccess::open(&path, &future).unwrap_err();
    assert!(error.to_string().contains("coordination timeout"));
    let check = Connection::open(&path).unwrap();
    assert_eq!(
        check
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        1
    );
    drop(check);
    reader.finish();
    let (conn, access) = DatabaseAccess::open(&path, &future).unwrap();
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        2
    );
    drop(conn);
    drop(access);
    assert!(DatabaseAccess::open(&path, &migrations).is_err());
}

#[test]
fn legacy_sqlite_writer_can_bypass_advisory_protocol_explicitly() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("state.sqlite3");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch("CREATE TABLE legacy(value)").unwrap();
    let mut holder = Worker::spawn(&path, "writer");
    // Intentionally nonparticipating: evidence of the documented rollout limit.
    raw.execute("INSERT INTO legacy VALUES(1)", []).unwrap();
    assert_eq!(
        raw.query_row("SELECT count(*) FROM legacy", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    holder.finish();
}

#[test]
fn concurrent_index_processes_converge_and_read_only_queries_work() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir(root.path().join(".mct-index")).unwrap();
    std::fs::write(
        root.path().join("sample.rs"),
        "pub fn coordinated_symbol() {}\n",
    )
    .unwrap();
    let path = root.path().join(".mct-index/index.sqlite3");
    drop(Index::open(root.path(), &path, ExcludeSet::default()).unwrap());
    let mut one = Worker::spawn(&path, "index");
    let mut two = Worker::spawn(&path, "index");
    one.child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"go\n")
        .unwrap();
    two.finish();
    assert!(one.child.wait().unwrap().success());
    let index = Index::open_existing_read_only(root.path(), &path, ExcludeSet::default()).unwrap();
    assert_eq!(index.find_symbol("coordinated_symbol").unwrap().len(), 1);
    let mut writable = Index::open(root.path(), &path, ExcludeSet::default()).unwrap();
    let mut holder = Worker::spawn(&path, "writer");
    std::fs::write(
        root.path().join("sample.rs"),
        "pub fn replacement_symbol() {}\n",
    )
    .unwrap();
    let error = writable
        .reindex(&mct_languages::build_registry(), false)
        .unwrap_err();
    assert!(error.to_string().contains("coordination timeout"));
    assert_eq!(index.find_symbol("coordinated_symbol").unwrap().len(), 1);
    assert!(index.find_symbol("replacement_symbol").unwrap().is_empty());
    assert!(DatabaseAccess::maintenance(&path, NOW).is_err());
    holder.finish();
    writable.set_post_reindex_hook(Box::new(|index| index.clear_embeddings()));
    let report = writable
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(
        report.warnings.is_empty(),
        "writer guard must release before hooks"
    );
    assert_eq!(index.find_symbol("replacement_symbol").unwrap().len(), 1);
}

#[test]
fn future_index_and_dashboard_schemas_fail_closed() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir(root.path().join(".mct-index")).unwrap();
    let path = root.path().join(".mct-index/index.sqlite3");
    drop(Index::open(root.path(), &path, ExcludeSet::default()).unwrap());
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch("PRAGMA user_version=999; PRAGMA journal_mode=DELETE")
        .unwrap();
    drop(raw);
    assert!(Index::open(root.path(), &path, ExcludeSet::default()).is_err());
    assert!(Index::open_existing_read_only(root.path(), &path, ExcludeSet::default()).is_err());
    let raw = Connection::open(&path).unwrap();
    assert_eq!(
        raw.pragma_query_value::<String, _>(None, "journal_mode", |r| r.get(0))
            .unwrap(),
        "delete"
    );
    let dashboard = Connection::open(root.path().join(".mct-index/dashboard.sqlite3")).unwrap();
    dashboard.execute_batch("PRAGMA user_version=999").unwrap();
    assert!(
        mct_index::review_state::ReviewSnapshot::load(&root.path().canonicalize().unwrap())
            .err()
            .unwrap()
            .to_string()
            .contains("newer than this reader")
    );
}

#[cfg(unix)]
#[test]
fn canonical_parent_aliases_share_locks_and_linked_files_are_rejected() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let root = TempDir::new().unwrap();
    std::fs::create_dir(root.path().join("physical")).unwrap();
    symlink(root.path().join("physical"), root.path().join("alias")).unwrap();
    let path = root.path().join("physical/state.sqlite3");
    let access = DatabaseAccess::shared(&path, NOW).unwrap();
    let _writer = access.writer(NOW).unwrap();
    let alias = DatabaseAccess::shared(&root.path().join("alias/state.sqlite3"), NOW).unwrap();
    assert!(alias.writer(NOW).is_err());
    let unsafe_path = root.path().join("linked.sqlite3");
    symlink(&path, &unsafe_path).unwrap();
    assert!(DatabaseAccess::shared(&unsafe_path, NOW).is_err());
    let writer_file = root.path().join("physical/state.sqlite3.writer.lock");
    assert_eq!(
        std::fs::metadata(&writer_file)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    std::fs::set_permissions(writer_file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(access.writer(NOW).is_err());
}

#[test]
fn uncontended_lock_latency_measurement() {
    let root = TempDir::new().unwrap();
    let access = DatabaseAccess::shared(&root.path().join("state.sqlite3"), NOW).unwrap();
    drop(access.writer(NOW).unwrap());
    let mut samples = Vec::new();
    for _ in 0..1000 {
        let start = Instant::now();
        drop(access.writer(Duration::from_secs(1)).unwrap());
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    println!(
        "uncontended writer acquisition+release ns: p50={} p95={} max={} (1000 samples)",
        samples[500], samples[950], samples[999]
    );
}
