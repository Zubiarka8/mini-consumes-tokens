//! Consistent, manually requested user-state snapshots. No production cutover.
//!
//! A bundle is complete only after its manifest is durably published. The
//! directory is exclusively created and private before SQLite writes into it.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, ensure, Context};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};

use super::{reviews, store};

const DATABASE: &str = "dashboard.sqlite3";
const MANIFEST: &str = "manifest.toon";
const FORMAT: &str = "mct-dashboard-backup/1";
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const COPY_TIMEOUT: Duration = Duration::from_secs(60);

fn temporary_directory(parent: Option<&Path>, prefix: &str) -> anyhow::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    Ok(match parent {
        Some(parent) => builder.tempdir_in(parent)?,
        None => builder.tempdir()?,
    })
}

pub(super) fn private_directory(path: &Path) -> anyhow::Result<()> {
    // A nonexistent child lets the shared guard validate directory ancestors.
    reviews::safe_path(&path.join(".backup-path-check"))?;
    ensure!(path.is_dir(), "backup destination is not a directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            fs::metadata(path)?.permissions().mode() & 0o077 == 0,
            "backup directory must have owner-only permissions"
        );
    }
    Ok(())
}

pub(super) fn private_file(path: &Path) -> anyhow::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn read_only(path: &Path) -> anyhow::Result<Connection> {
    reviews::safe_path(path)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        reviews::safe_path(Path::new(&sidecar))?;
    }
    ensure!(path.is_file(), "dashboard database does not exist");
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(250))?;
    conn.pragma_update(None, "trusted_schema", false)?;
    conn.pragma_update(None, "query_only", true)?;
    Ok(conn)
}

fn validate(conn: &Connection) -> anyhow::Result<i64> {
    let schema = store::validate_backup_schema(conn)?;
    let mut check = conn.prepare("PRAGMA integrity_check")?;
    let mut rows = check.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        let result: String = row.get(0)?;
        // Do not echo corruption diagnostics that can contain stored values.
        ensure!(result == "ok", "dashboard integrity check failed");
        count += 1;
    }
    ensure!(
        count == 1,
        "dashboard integrity check returned no valid result"
    );
    let mut foreign = conn.prepare("PRAGMA foreign_key_check")?;
    ensure!(
        foreign.query([])?.next()?.is_none(),
        "dashboard foreign-key check failed"
    );
    Ok(schema)
}

fn copy_snapshot(
    source: &Connection,
    target: &mut Connection,
    timeout: Duration,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let backup = Backup::new(source, target)?;
    loop {
        ensure!(
            started.elapsed() < timeout,
            "dashboard backup copy timed out"
        );
        match backup.step(128)? {
            StepResult::Done => return Ok(()),
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => {
                std::thread::sleep(Duration::from_millis(25));
            }
            _ => bail!("unsupported SQLite backup state"),
        }
    }
}

fn digest(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = hmac_sha256::Hash::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn sync_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    // Rust's portable file API does not expose Windows directory flushing.
    // File flushes still run; directory durability needs native acceptance.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// No index open, dashboard migration/import or source mutation is performed.
pub(crate) fn create(root: &Path, destination: Option<&Path>) -> anyhow::Result<PathBuf> {
    ensure!(
        root.join(".mct-index").join(DATABASE).is_file(),
        "dashboard database does not exist"
    );
    // Preserve the manual command's fail-before-destination behavior.
    let source_path = root.join(".mct-index").join(DATABASE);
    let _source_access = mct_index::coordination::DatabaseAccess::shared(
        &source_path,
        mct_index::coordination::DEFAULT_TIMEOUT,
    )?;
    validate(&read_only(&source_path)?)?;
    let destination = prepare_destination(root, destination)?;
    let access = mct_index::coordination::DatabaseAccess::shared(
        &destination.join("management"),
        COPY_TIMEOUT,
    )?;
    let _guard = access.writer(COPY_TIMEOUT)?;
    create_managed(root, &destination, None)
}

/// Caller holds the destination management guard; avoid nested acquisition.
pub(super) fn create_managed(
    root: &Path,
    destination: &Path,
    owner: Option<(&str, u64)>,
) -> anyhow::Result<PathBuf> {
    let root = std::path::absolute(root)?;
    let source_path = root.join(".mct-index").join(DATABASE);
    reviews::safe_path(&source_path)?;
    ensure!(source_path.is_file(), "dashboard database does not exist");
    let _access = mct_index::coordination::DatabaseAccess::shared(
        &source_path,
        mct_index::coordination::DEFAULT_TIMEOUT,
    )?;
    let source = read_only(&source_path)?;
    // The read transaction pins schema validation and all copy steps to one
    // snapshot even if another process commits or attempts a migration.
    let snapshot = source.unchecked_transaction()?;
    store::validate_backup_schema(&snapshot)?;
    let pages =
        u64::try_from(snapshot.pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))?)?;
    let page_size =
        u64::try_from(snapshot.pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))?)?;
    ensure!(
        pages.checked_mul(page_size).is_some_and(|n| n <= MAX_BYTES),
        "dashboard exceeds the 256 MiB manual-backup limit"
    );
    let destination = prepare_destination(root.as_path(), Some(destination))?;
    let created = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let bundle = temporary_directory(Some(&destination), &format!("dashboard-{created}-"))?;
    let database = bundle.path().join(DATABASE);
    private_file(&database)?;
    let mut target = Connection::open(&database)?;
    target.pragma_update(None, "synchronous", "FULL")?;
    copy_snapshot(&snapshot, &mut target, COPY_TIMEOUT)?;
    #[cfg(test)]
    test_hook("snapshot")?;
    snapshot.commit()?;
    // A standalone artifact must not need an adjacent WAL at restore time.
    let mode: String = target.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    if mode != "delete" {
        target.pragma_update(None, "journal_mode", "DELETE")?;
    }
    target.close().map_err(|(_, error)| error)?;
    let reopened = read_only(&database)?;
    let schema = validate(&reopened)?;
    drop(reopened);
    let bytes = fs::metadata(&database)?.len();
    ensure!(bytes <= MAX_BYTES, "backup artifact exceeds its size limit");
    let sha = digest(&database)?;
    File::open(&database)?.sync_all()?;
    let completed = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    ensure!(
        completed >= created,
        "system clock moved backwards during backup"
    );
    if let Some((owner, attempt)) = owner {
        let mut receipt = private_file(&bundle.path().join("managed.toon"))?;
        receipt.write_all(
            format!(
                "format: mct-managed-backup/1\nsource: {owner}\nattempt: {attempt}\norder: {}\n",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            )
            .as_bytes(),
        )?;
        receipt.sync_all()?;
    }
    let manifest = format!(
        "format: {FORMAT}\ncreated_unix: {created}\ncompleted_unix: {completed}\nschema: {schema}\napplication: {}\nsqlite: {}\nbytes: {bytes}\nsha256: {sha}\n",
        env!("CARGO_PKG_VERSION"), rusqlite::version(),
    );
    let mut staging = tempfile::NamedTempFile::new_in(bundle.path())?;
    staging.write_all(manifest.as_bytes())?;
    staging.as_file().sync_all()?;
    staging.persist_noclobber(bundle.path().join(".manifest.pending"))?;
    #[cfg(test)]
    test_hook("verification")?;
    verify_restore_with_manifest(bundle.path(), ".manifest.pending")?;
    #[cfg(test)]
    test_hook("before-publication")?;
    fs::rename(
        bundle.path().join(".manifest.pending"),
        bundle.path().join(MANIFEST),
    )?;
    sync_directory(bundle.path())?;
    sync_directory(&destination)?;
    #[cfg(test)]
    test_hook("after-publication")?;
    Ok(bundle.keep())
}

pub(super) fn completed_snapshot(bundle: &Path) -> anyhow::Result<Connection> {
    snapshot_with_manifest(bundle, MANIFEST)
}

fn snapshot_with_manifest(bundle: &Path, manifest_name: &str) -> anyhow::Result<Connection> {
    private_directory(bundle)?;
    let manifest_path = bundle.join(manifest_name);
    reviews::safe_path(&manifest_path)?;
    ensure!(
        fs::metadata(&manifest_path)?.len() <= 2048,
        "backup manifest is oversized"
    );
    let mut manifest = String::new();
    File::open(manifest_path)?
        .take(2049)
        .read_to_string(&mut manifest)?;
    ensure!(manifest.len() <= 2048, "backup manifest is oversized");
    let mut fields = BTreeMap::new();
    for line in manifest.lines() {
        let (key, value) = line.split_once(": ").context("invalid backup manifest")?;
        ensure!(
            fields.insert(key, value).is_none(),
            "duplicate backup manifest field"
        );
    }
    ensure!(fields.len() == 8, "invalid backup manifest fields");
    for key in [
        "format",
        "created_unix",
        "completed_unix",
        "schema",
        "application",
        "sqlite",
        "bytes",
        "sha256",
    ] {
        ensure!(fields.contains_key(key), "missing backup manifest field");
    }
    ensure!(fields["format"] == FORMAT, "unsupported backup format");
    let created: u64 = fields["created_unix"].parse()?;
    let completed: u64 = fields["completed_unix"].parse()?;
    ensure!(completed >= created, "invalid backup timestamps");
    let database = bundle.join(DATABASE);
    reviews::safe_path(&database)?;
    // A completed bundle is standalone. Refuse sidecars instead of replaying
    // unexpected transactions into an artifact after its digest was published.
    for suffix in ["-wal", "-shm", "-journal"] {
        ensure!(
            !bundle.join(format!("{DATABASE}{suffix}")).exists(),
            "completed backup unexpectedly has a SQLite sidecar"
        );
    }
    let bytes: u64 = fields["bytes"].parse()?;
    ensure!(
        bytes <= MAX_BYTES && bytes == fs::metadata(&database)?.len(),
        "backup size mismatch"
    );
    ensure!(
        digest(&database)? == fields["sha256"],
        "backup digest mismatch"
    );
    let source = read_only(&database)?;
    let schema: i64 = fields["schema"].parse()?;
    ensure!(validate(&source)? == schema, "backup schema mismatch");
    Ok(source)
}

/// Rehearse restore into a private temporary DB; never replace project data.
pub(crate) fn verify_restore(bundle: &Path) -> anyhow::Result<()> {
    verify_restore_with_manifest(bundle, MANIFEST)
}

fn verify_restore_with_manifest(bundle: &Path, manifest: &str) -> anyhow::Result<()> {
    let source = snapshot_with_manifest(&std::path::absolute(bundle)?, manifest)?;
    let restored = temporary_directory(None, "mct-restore-check-")?;
    let path = restored.path().join(DATABASE);
    private_file(&path)?;
    let mut target = Connection::open(&path)?;
    copy_snapshot(&source, &mut target, COPY_TIMEOUT)?;
    validate(&target)?;
    compare_tables(&source, &target)?;
    target.close().map_err(|(_, error)| error)?;
    validate(&Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)?;
    Ok(())
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;

/// Validate and create the one authorized destination, excluding source paths.
pub(super) fn prepare_destination(
    root: &Path,
    destination: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let destination = std::path::absolute(destination.unwrap_or(&root.join(".mct-index/backups")))?;
    reviews::safe_path(&destination.join(".backup-path-check"))?;
    // Canonicalize the existing parent before creating any directory so `..`
    // cannot bypass the rule excluding backups from indexed source paths.
    let resolved = if destination.exists() {
        destination.canonicalize()?
    } else {
        let parent = destination
            .parent()
            .context("backup destination needs a parent")?;
        parent.canonicalize()?.join(
            destination
                .file_name()
                .context("invalid backup destination")?,
        )
    };
    let physical_root = root.canonicalize()?;
    ensure!(
        !resolved.starts_with(&physical_root)
            || resolved.starts_with(physical_root.join(".mct-index")),
        "backups inside the project must be under .mct-index"
    );
    if !destination.exists() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&destination)?;
        sync_directory(
            destination
                .parent()
                .context("backup destination needs a parent")?,
        )?;
    }
    private_directory(&destination)?;
    Ok(destination)
}

/// Export into a new isolated directory; never overwrite any existing path.
pub(crate) fn restore_into(bundle: &Path, destination: &Path) -> anyhow::Result<()> {
    let source = completed_snapshot(bundle)?;
    let destination = std::path::absolute(destination)?;
    reviews::safe_path(&destination)?;
    ensure!(!destination.exists(), "restore destination must not exist");
    let parent = destination
        .parent()
        .context("restore destination needs a parent")?;
    let restored = temporary_directory(Some(parent), "mct-isolated-restore-")?;
    let database = restored.path().join(DATABASE);
    private_file(&database)?;
    let mut target = Connection::open(&database)?;
    copy_snapshot(&source, &mut target, COPY_TIMEOUT)?;
    validate(&target)?;
    compare_tables(&source, &target)?;
    target.close().map_err(|(_, e)| e)?;
    File::open(database)?.sync_all()?;
    sync_directory(restored.path())?;
    // create_dir is the no-clobber reservation; known file moves avoid replacing
    // an existing destination on platforms where directory rename overwrites it.
    fs::create_dir(&destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700))?;
    }
    fs::rename(restored.path().join(DATABASE), destination.join(DATABASE))?;
    sync_directory(&destination)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(test)]
thread_local! {
    pub(super) static FAULT: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

#[cfg(test)]
fn test_hook(stage: &str) -> anyhow::Result<()> {
    if FAULT.with(|f| f.get() == stage) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "injected storage exhaustion",
        )
        .into());
    }
    if std::env::var("MCT_BACKUP_TEST_PAUSE").ok().as_deref() == Some(stage) {
        println!("BACKUP_TEST_READY");
        std::io::stdout().flush()?;
        let root = std::env::var_os("MCT_BACKUP_TEST_ROOT").context("test barrier needs root")?;
        let access = mct_index::coordination::DatabaseAccess::shared(
            &Path::new(&root).join(".mct-index/test-barrier"),
            Duration::from_secs(60),
        )?;
        let _released_by_parent = access.writer(Duration::from_secs(60))?;
    }
    Ok(())
}

fn compare_tables(source: &Connection, target: &Connection) -> anyhow::Result<()> {
    // Compare every application table, preserving SQLite types and duplicates.
    let mut tables =
        source.prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?;
    for table in tables.query_map([], |r| r.get::<_, String>(0))? {
        let quoted = table?.replace('"', "\"\"");
        let query = format!("SELECT * FROM \"{quoted}\" ORDER BY rowid");
        let mut left = source.prepare(&query)?;
        let mut right = target.prepare(&query)?;
        ensure!(
            left.column_count() == right.column_count(),
            "restored columns mismatch"
        );
        let columns = left.column_count();
        let mut left_rows = left.query([])?;
        let mut right_rows = right.query([])?;
        loop {
            match (left_rows.next()?, right_rows.next()?) {
                (None, None) => break,
                (Some(left), Some(right)) => {
                    for i in 0..columns {
                        ensure!(
                            left.get_ref(i)? == right.get_ref(i)?,
                            "restored table content mismatch"
                        );
                    }
                }
                _ => bail!("restored row count mismatch"),
            }
        }
    }
    Ok(())
}
