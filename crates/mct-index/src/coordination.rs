//! Cooperative database ownership. Persistent lock files must never be removed
//! while clients run. This does not constrain old binaries or external tools.
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{bail, ensure, Context};
use rusqlite::Connection;
use rusqlite_migration::Migrations;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// An OS lock released by closing its private file handle, including on crash.
/// Do not clone this handle or unlink its file.
#[derive(Debug)]
pub struct Guard {
    _file: File,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // Explicit unlock also releases a transient inherited Unix descriptor
        // between another thread's fork and exec. Close remains the fallback.
        let _ = self._file.unlock();
    }
}

/// Shared lifetime barrier. Declare after the SQLite connection so it drops last.
#[derive(Debug)]
pub struct DatabaseAccess {
    path: PathBuf,
    _barrier: Guard,
}

fn safe_file(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "refusing linked or nonregular coordination/database file"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                ensure!(
                    meta.nlink() == 1,
                    "refusing hard-linked coordination/database file"
                );
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    meta.file_attributes() & 0x400 == 0,
                    "refusing reparse-point coordination/database file"
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn database_path(path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let parent = absolute
        .parent()
        .context("database needs a parent directory")?
        .canonicalize()?;
    let result = parent.join(absolute.file_name().context("database needs a file name")?);
    safe_file(&result)?;
    Ok(result)
}

fn acquire(path: &Path, suffix: &str, shared: bool, timeout: Duration) -> anyhow::Result<Guard> {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    let lock_path = PathBuf::from(name);
    safe_file(&lock_path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&lock_path)
        .context("cannot open database coordination file")?;
    safe_file(&lock_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let opened = file.metadata()?;
        let named = fs::symlink_metadata(&lock_path)?;
        ensure!(
            opened.dev() == named.dev() && opened.ino() == named.ino(),
            "coordination file changed during open"
        );
        ensure!(
            opened.permissions().mode() & 0o077 == 0,
            "coordination file must be private (mode 0600)"
        );
    }
    let start = Instant::now();
    loop {
        let result = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match result {
            Ok(()) => return Ok(Guard { _file: file }),
            Err(TryLockError::WouldBlock) => {
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    bail!("database coordination timeout: another process holds {} access; retry after its operation finishes; close compatible clients before migration/maintenance; never delete lock files", if suffix == ".writer.lock" { "writer" } else { "database" });
                }
                std::thread::sleep(remaining.min(Duration::from_millis(10)));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error).context("OS database locking failed")
            }
        }
    }
}

impl DatabaseAccess {
    /// Shared access does not exclude ordinary writers or other readers.
    /// The parent must exist. The database itself is not created here.
    pub fn shared(path: &Path, timeout: Duration) -> anyhow::Result<Self> {
        let path = database_path(path)?;
        let barrier = acquire(&path, ".access.lock", true, timeout)?;
        Ok(Self {
            path,
            _barrier: barrier,
        })
    }

    /// Serializes application-level write operations, not read requests.
    pub fn writer(&self, timeout: Duration) -> anyhow::Result<Guard> {
        safe_file(&self.path)?;
        acquire(&self.path, ".writer.lock", false, timeout)
    }

    /// Prerequisite for offline migration/replacement, never a cutover itself.
    /// Caller must hold no shared access or writer guard for this database.
    pub fn maintenance(path: &Path, timeout: Duration) -> anyhow::Result<Guard> {
        acquire(&database_path(path)?, ".access.lock", false, timeout)
    }

    /// Opens with schema preflight and migrates only with exclusive access.
    /// Returned connection must close before the returned access guard.
    pub fn open(path: &Path, migrations: &Migrations<'_>) -> anyhow::Result<(Connection, Self)> {
        Self::open_with_setup(path, migrations, |_| Ok(()))
    }

    /// Like open, registering connection-local SQL functions before migrations.
    pub fn open_with_setup(
        path: &Path,
        migrations: &Migrations<'_>,
        setup: fn(&Connection) -> rusqlite::Result<()>,
    ) -> anyhow::Result<(Connection, Self)> {
        for _ in 0..3 {
            let access = Self::shared(path, DEFAULT_TIMEOUT)?;
            let writer = access.writer(DEFAULT_TIMEOUT)?;
            let conn = Connection::open(&access.path)?;
            conn.busy_timeout(DEFAULT_TIMEOUT)?;
            setup(&conn)?;
            // Refuse future schemas before any journal-setting mutation.
            if migrations.pending_migrations(&conn)? == 0 {
                conn.pragma_update(None, "journal_mode", "WAL")?;
                conn.pragma_update(None, "foreign_keys", "ON")?;
                drop(writer);
                return Ok((conn, access));
            }
            let physical = access.path.clone();
            drop(conn);
            drop(writer);
            drop(access);
            let maintenance = Self::maintenance(&physical, DEFAULT_TIMEOUT)?;
            let mut conn = Connection::open(&physical)?;
            conn.busy_timeout(DEFAULT_TIMEOUT)?;
            setup(&conn)?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            migrations.to_latest(&mut conn)?;
            drop(conn);
            drop(maintenance);
            // No portable atomic downgrade: reacquire shared access and recheck.
        }
        bail!("database schema changed repeatedly during coordinated open")
    }
}
