//! Manifest ownership and note-by-note publication under an OS writer lock.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{Result, VaultError, VaultReport};

/// Manifest file inside the vault: one `hash,path` row per generated note,
/// in the project's TOON table style rather than JSON (nothing outside this
/// crate reads it).
pub const MANIFEST_FILE: &str = ".manifest.toon";

/// Hex SHA-256 of a note's text.
pub fn hash_note(content: &str) -> String {
    hmac_sha256::Hash::hash(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn quote_if_needed(path: &str) -> String {
    if path.contains([',', '"', '\\']) {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

fn unquote(field: &str) -> String {
    match field
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => field.to_string(),
    }
}

/// Note path -> note hash.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub notes: BTreeMap<String, String>,
}

impl Manifest {
    pub fn render(&self) -> String {
        let mut out = format!("notes[{}]{{hash,path}}:\n", self.notes.len());
        for (path, hash) in &self.notes {
            out.push_str(&format!("  {hash},{}\n", quote_if_needed(path)));
        }
        out
    }

    /// `None` for anything that is not a manifest this crate wrote.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let header = lines.next()?;
        let count: usize = header
            .strip_prefix("notes[")?
            .strip_suffix("]{hash,path}:")?
            .parse()
            .ok()?;
        let mut notes = BTreeMap::new();
        for line in lines.filter(|l| !l.trim().is_empty()) {
            let (hash, path) = line.trim_start().split_once(',')?;
            let path = unquote(path);
            if safe_join(Path::new("vault"), &path).is_none()
                || path == MANIFEST_FILE
                || path.eq_ignore_ascii_case(".obsidian")
                || path.to_ascii_lowercase().starts_with(".obsidian/")
                || hash.len() != 64
                || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                || notes.insert(path, hash.to_string()).is_some()
            {
                return None;
            }
        }
        (notes.len() == count).then_some(Self { notes })
    }

    /// The manifest stored in `vault`, or `None` if it is missing or unreadable
    /// or invalid. Publication uses a stricter fallible read to preserve conflicts.
    pub fn load(vault: &Path) -> Option<Self> {
        let text = fs::read_to_string(vault.join(MANIFEST_FILE)).ok()?;
        Self::parse(&text)
    }
}

/// `rel` joined under `base`, only if it is a plain relative path: no root,
/// no drive prefix, no `.`/`..`. The vault only ever writes through this.
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty()
        || rel.contains(['\\', ':'])
        || rel
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return None;
    }
    let mut joined = base.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            _ => return None,
        }
    }
    joined.starts_with(base).then_some(joined)
}

fn io_error(path: &Path, source: std::io::Error) -> VaultError {
    VaultError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn is_real_dir(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.is_dir())
}

fn is_real_file(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.is_file())
}

/// `out` with `suffix` appended to its final name (`obsidian` -> `obsidian.tmp`).
fn sibling(out: &Path, suffix: &str) -> Result<PathBuf> {
    let name = out
        .file_name()
        .ok_or_else(|| VaultError::UnsafePath(out.display().to_string()))?;
    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(out.with_file_name(name))
}

/// Reject symlinks in every existing ancestor, including nested user files.
fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(VaultError::Symlink(ancestor.display().to_string()))
            }
            Ok(_) => (),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(io_error(ancestor, err)),
        }
    }
    Ok(())
}

/// How long a publication waits for another one to finish before failing.
pub const LOCK_WAIT: Duration = Duration::from_secs(120);

/// Contents of the legacy `<vault>.lock` file while this version publishes.
/// Older versions leave it empty, so a stale file with this text was left by
/// a writer of this version that stopped; it held the OS lock, which the OS
/// has released, so the file can be replaced.
const LEGACY_MARKER: &str = "mct-obsidian: guarded by <vault>.writer.lock\n";

/// Prefix of the temporary files written beside each note before the rename.
const TEMP_PREFIX: &str = ".mct-obsidian-";
const TEMP_SUFFIX: &str = ".tmp";

/// The writer lock: an OS lock on `<vault>.writer.lock`, released by the OS
/// when the process exits however it exits, plus the legacy `<vault>.lock`
/// file that older versions create exclusively, so they keep failing closed
/// while this version publishes.
struct WriterLock {
    _file: fs::File,
    legacy: PathBuf,
}

impl WriterLock {
    fn acquire(out: &Path, wait: Duration) -> Result<Self> {
        let path = sibling(out, ".writer.lock")?;
        check_ancestors(&path)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io_error(&path, e))?;
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(VaultError::Locked {
                        path: path.display().to_string(),
                        reason: format!(
                            "another publication still held it after {} s",
                            wait.as_secs_f32()
                        ),
                    })
                }
                Err(fs::TryLockError::Error(e)) => return Err(io_error(&path, e)),
            }
        }
        let legacy = sibling(out, ".lock")?;
        check_ancestors(&legacy)?;
        match create_with_marker(&legacy) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Left by a stopped writer of this version: the OS lock it
                // held is ours now, so the file is ours too.
                let text = fs::read_to_string(&legacy).map_err(|e| io_error(&legacy, e))?;
                if text != LEGACY_MARKER {
                    return Err(VaultError::Locked {
                        path: legacy.display().to_string(),
                        reason: "an older mct version is publishing, or stopped while \
                                 publishing; delete the file if no mct process is running"
                            .into(),
                    });
                }
            }
            Err(e) => return Err(io_error(&legacy, e)),
        }
        Ok(Self {
            _file: file,
            legacy,
        })
    }
}

/// Creates `path` already holding [`LEGACY_MARKER`], so a writer stopped at
/// any point leaves either no file or the marker, never an empty file that
/// would look like an older version's. Fails with `AlreadyExists` if `path`
/// exists. Where hard links are unsupported, falls back to create-then-write.
fn create_with_marker(path: &Path) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("lock path has no parent"))?;
    let mut staged = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)?;
    staged.write_all(LEGACY_MARKER.as_bytes())?;
    match fs::hard_link(staged.path(), path) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(LEGACY_MARKER.as_bytes())
        }
        linked => linked,
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // The OS lock is released when `_file` closes, after this.
        let _ = fs::remove_file(&self.legacy);
    }
}

/// Brings the vault at `out` in line with `notes`. Only manifest-owned files
/// are written or removed; every other file is left alone. Each changed note
/// is written beside its destination and renamed over it, so a reader sees
/// every note either old or new, never missing. The manifest is written
/// last: after an interruption it still describes the previous notes, and
/// the next publication accepts a note that already holds its new text.
/// Concurrent writers, in this process or another, wait for each other.
pub fn publish(out: &Path, notes: &BTreeMap<String, String>) -> Result<VaultReport> {
    publish_with(out, notes, &|from, to| fs::rename(from, to), LOCK_WAIT)
}

fn publish_with(
    out: &Path,
    notes: &BTreeMap<String, String>,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
    wait: Duration,
) -> Result<VaultReport> {
    check_ancestors(out)?;
    if out.exists() && !is_real_dir(out) {
        return Err(VaultError::NotADirectory(out.display().to_string()));
    }
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let _lock = WriterLock::acquire(out, wait)?;

    let manifest_path = out.join(MANIFEST_FILE);
    check_ancestors(&manifest_path)?;
    let old = match fs::read_to_string(&manifest_path) {
        Ok(text) => Manifest::parse(&text)
            .ok_or_else(|| VaultError::Manifest(manifest_path.display().to_string()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Manifest::default(),
        Err(e) => return Err(io_error(&manifest_path, e)),
    };
    let hashes: BTreeMap<String, String> = notes
        .iter()
        .map(|(p, t)| (p.clone(), hash_note(t)))
        .collect();
    // Notes whose file already holds the text to publish.
    let mut current = BTreeSet::new();
    // Check actual contents as well as the manifest; local edits must not be
    // silently overwritten, including when an indexed file has been removed.
    // A file that already holds its new text was written by an interrupted
    // publication, not edited.
    for (path, expected) in &old.notes {
        let file = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath(path.clone()))?;
        check_ancestors(&file)?;
        match fs::read_to_string(&file) {
            Ok(text) => {
                let found = hash_note(&text);
                if hashes.get(path) == Some(&found) {
                    current.insert(path.as_str());
                } else if found != *expected {
                    return Err(VaultError::Conflict(path.clone()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(&file, e)),
        }
    }
    for (path, hash) in &hashes {
        if old.notes.contains_key(path) {
            continue;
        }
        let file = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath(path.clone()))?;
        check_ancestors(&file)?;
        if file.exists() {
            let ours = is_real_file(&file)
                && fs::read_to_string(&file).is_ok_and(|text| hash_note(&text) == *hash);
            if !ours {
                return Err(VaultError::Conflict(path.clone()));
            }
            current.insert(path.as_str());
        }
    }
    // Validate the whole existing tree even on no-op runs, and drop
    // temporary files left by an interrupted publication.
    validate_tree(out)?;
    let removed: Vec<&String> = old
        .notes
        .keys()
        .filter(|p| !hashes.contains_key(*p))
        .collect();
    let changed: Vec<(&String, &String)> = notes
        .iter()
        .filter(|(p, _)| !current.contains(p.as_str()))
        .collect();
    if changed.is_empty() && removed.is_empty() && old.notes == hashes && manifest_path.is_file() {
        return Ok(VaultReport {
            notes_written: 0,
            notes_unchanged: notes.len(),
            notes_removed: 0,
        });
    }
    fs::create_dir_all(out).map_err(|e| io_error(out, e))?;
    for (path, text) in &changed {
        let dest = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath((*path).clone()))?;
        replace_file(&dest, text, rename)?;
    }
    for path in &removed {
        let file = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath((*path).clone()))?;
        match fs::remove_file(&file) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(&file, e)),
        }
    }
    let report = VaultReport {
        notes_written: changed.len(),
        notes_unchanged: notes.len() - changed.len(),
        notes_removed: removed.len(),
    };
    replace_file(&manifest_path, &Manifest { notes: hashes }.render(), rename)?;
    Ok(report)
}

/// Writes `text` beside `dest` and renames it over `dest`.
fn replace_file(
    dest: &Path,
    text: &str,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| VaultError::UnsafePath(dest.display().to_string()))?;
    fs::create_dir_all(dir).map_err(|e| io_error(dir, e))?;
    check_ancestors(dest)?;
    if dest.exists() && !is_real_file(dest) {
        return Err(VaultError::Conflict(dest.display().to_string()));
    }
    let mut temp = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)
        .map_err(|e| io_error(dir, e))?;
    temp.write_all(text.as_bytes())
        .map_err(|e| io_error(temp.path(), e))?;
    let temp = temp.into_temp_path();
    rename(&temp, dest).map_err(|e| io_error(dest, e))?;
    // Renamed away: nothing left for the guard to delete.
    let _ = temp.keep();
    Ok(())
}

fn is_leftover_temp(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.starts_with(TEMP_PREFIX) && name.ends_with(TEMP_SUFFIX))
}

fn validate_tree(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| io_error(dir, e))? {
        let entry = entry.map_err(|e| io_error(dir, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| io_error(&path, e))?;
        if kind.is_symlink() {
            return Err(VaultError::Symlink(path.display().to_string()));
        }
        if kind.is_dir() {
            validate_tree(&path)?;
        } else if !kind.is_file() {
            return Err(VaultError::Conflict(path.display().to_string()));
        } else if is_leftover_temp(&entry.file_name()) {
            fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn manifest_roundtrips_and_rejects_traversal_or_invalid_hashes() {
        let manifest = Manifest {
            notes: BTreeMap::from([("files/a,\"b.md".into(), hash_note("text"))]),
        };
        assert_eq!(Manifest::parse(&manifest.render()), Some(manifest));
        for path in [
            "../x",
            "/x",
            "files/../../x",
            "files\\x",
            "C:/x",
            ".obsidian/app.json",
            ".OBSIDIAN/app.json",
            ".obsidian",
            MANIFEST_FILE,
        ] {
            assert!(Manifest::parse(&format!(
                "notes[1]{{hash,path}}:\n  {},{path}\n",
                hash_note("x")
            ))
            .is_none());
        }
        assert!(Manifest::parse("notes[1]{hash,path}:\n  bad,files/x.md\n").is_none());
    }

    #[test]
    fn a_failed_publication_keeps_the_old_manifest_and_the_next_one_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &notes(&[("a.md", "a"), ("index.md", "old")])).unwrap();
        fs::create_dir(out.join(".obsidian")).unwrap();
        fs::write(out.join(".obsidian/settings.json"), "settings").unwrap();
        fs::write(out.join("personal.md"), "personal").unwrap();
        let manifest = fs::read(out.join(MANIFEST_FILE)).unwrap();
        let next = notes(&[("a.md", "a2"), ("index.md", "new")]);
        let result = publish_with(
            &out,
            &next,
            &|from, to| {
                if to.ends_with("index.md") {
                    Err(std::io::Error::other("injected rename failure"))
                } else {
                    fs::rename(from, to)
                }
            },
            LOCK_WAIT,
        );
        assert!(result.is_err());
        // Each note is old or new; the manifest still describes the old notes.
        assert_eq!(fs::read_to_string(out.join("a.md")).unwrap(), "a2");
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "old");
        assert_eq!(fs::read(out.join(MANIFEST_FILE)).unwrap(), manifest);
        let report = publish(&out, &next).unwrap();
        assert_eq!(report.notes_written, 1);
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "new");
        assert_eq!(Manifest::load(&out).unwrap().notes.len(), 2);
        assert_eq!(
            fs::read_to_string(out.join("personal.md")).unwrap(),
            "personal"
        );
        assert_eq!(
            fs::read_to_string(out.join(".obsidian/settings.json")).unwrap(),
            "settings"
        );
        let mut names: Vec<_> = fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                MANIFEST_FILE,
                ".obsidian",
                "a.md",
                "index.md",
                "personal.md"
            ]
        );
        assert!(!sibling(&out, ".lock").unwrap().exists());
    }

    #[test]
    fn a_leftover_temporary_note_is_removed_by_the_next_publication() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &notes(&[("files/a.md", "a")])).unwrap();
        let leftover = out.join(format!("files/{TEMP_PREFIX}x1{TEMP_SUFFIX}"));
        fs::write(&leftover, "half").unwrap();
        publish(&out, &notes(&[("files/a.md", "a")])).unwrap();
        assert!(!leftover.exists());
    }

    #[test]
    fn concurrent_publishers_wait_for_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &notes(&[("index.md", "0")])).unwrap();
        let (inside, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let first = {
            let out = out.clone();
            std::thread::spawn(move || {
                publish_with(
                    &out,
                    &notes(&[("index.md", "1")]),
                    &|from, to| {
                        let _ = inside.send(());
                        let _ = released.recv();
                        fs::rename(from, to)
                    },
                    LOCK_WAIT,
                )
            })
        };
        entered.recv().unwrap();
        let second = {
            let out = out.clone();
            std::thread::spawn(move || publish(&out, &notes(&[("index.md", "2")])))
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!second.is_finished(), "the second writer must wait");
        // Dropping the sender releases every later rename too.
        drop(release);
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "2");
    }

    #[test]
    fn a_writer_that_cannot_get_the_lock_in_time_fails_with_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        let held = WriterLock::acquire(&out, LOCK_WAIT).unwrap();
        let result = publish_with(
            &out,
            &notes(&[("index.md", "x")]),
            &|from, to| fs::rename(from, to),
            Duration::from_millis(50),
        );
        assert!(
            matches!(&result, Err(VaultError::Locked { reason, .. }) if reason.contains("another publication")),
            "{result:?}"
        );
        drop(held);
        publish(&out, &notes(&[("index.md", "x")])).unwrap();
    }

    #[test]
    fn a_legacy_lock_left_by_this_version_is_replaced_but_a_foreign_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        let legacy = sibling(&out, ".lock").unwrap();
        fs::write(&legacy, LEGACY_MARKER).unwrap();
        publish(&out, &notes(&[("index.md", "x")])).unwrap();
        assert!(!legacy.exists());
        fs::write(&legacy, "").unwrap();
        let result = publish(&out, &notes(&[("index.md", "y")]));
        assert!(
            matches!(&result, Err(VaultError::Locked { reason, .. }) if reason.contains("older mct version")),
            "{result:?}"
        );
        assert!(legacy.exists());
    }

    #[test]
    fn unmanaged_files_and_edits_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        let notes = BTreeMap::from([("index.md".into(), "generated".into())]);
        publish(&out, &notes).unwrap();
        fs::create_dir_all(out.join("files/custom")).unwrap();
        fs::write(out.join("files/custom/user.md"), "user").unwrap();
        publish(
            &out,
            &BTreeMap::from([("index.md".into(), "updated".into())]),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(out.join("files/custom/user.md")).unwrap(),
            "user"
        );
        fs::write(out.join("index.md"), "edited").unwrap();
        assert!(publish(&out, &notes).is_err());
        assert!(publish(&out, &BTreeMap::new()).is_err());
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "edited");
        fs::write(out.join(MANIFEST_FILE), "broken").unwrap();
        assert!(publish(&out, &notes).is_err());
    }

    #[test]
    fn unmanaged_collisions_and_existing_writer_locks_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("index.md"), "user").unwrap();
        let notes = BTreeMap::from([("index.md".into(), "generated".into())]);
        assert!(publish(&out, &notes).is_err());
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "user");
        fs::write(sibling(&out, ".lock").unwrap(), "another writer").unwrap();
        assert!(publish(&out, &notes).is_err());
        assert_eq!(
            fs::read_to_string(sibling(&out, ".lock").unwrap()).unwrap(),
            "another writer"
        );
    }

    #[cfg(unix)]
    #[test]
    fn literal_backslash_user_names_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &BTreeMap::from([("files/a.md".into(), "old".into())])).unwrap();
        fs::write(out.join("files\\a.md"), "literal backslash").unwrap();
        publish(&out, &BTreeMap::from([("files/a.md".into(), "new".into())])).unwrap();
        assert_eq!(
            fs::read_to_string(out.join("files\\a.md")).unwrap(),
            "literal backslash"
        );
    }

    // The macOS test host refuses invalid UTF-8 filenames outside the sandbox
    // too. Exercise real preservation on Linux and path identity on Unix.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_user_names_survive_publication_on_linux() {
        use std::os::unix::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &BTreeMap::from([("index.md".into(), "old".into())])).unwrap();
        let name = std::ffi::OsString::from_vec(vec![b'u', 0xff]);
        fs::write(out.join(&name), "user").unwrap();
        publish(&out, &BTreeMap::from([("index.md".into(), "new".into())])).unwrap();
        assert_eq!(fs::read_to_string(out.join(name)).unwrap(), "user");
    }

    fn notes(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect()
    }

    #[test]
    fn a_reader_never_sees_a_note_missing_during_publication() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        let paths = ["index.md", "files/a.md", "files/b.md"];
        publish(
            &out,
            &notes(&[(paths[0], "0"), (paths[1], "a"), (paths[2], "b")]),
        )
        .unwrap();
        let missing = std::cell::RefCell::new(Vec::new());
        let check = || {
            for path in paths {
                if !out.join(path).is_file() {
                    missing.borrow_mut().push(path);
                }
            }
        };
        publish_with(
            &out,
            &notes(&[(paths[0], "1"), (paths[1], "a2"), (paths[2], "b")]),
            &|from, to| {
                check();
                fs::rename(from, to)?;
                check();
                Ok(())
            },
            LOCK_WAIT,
        )
        .unwrap();
        assert_eq!(missing.into_inner(), Vec::<&str>::new());
        assert_eq!(fs::read_to_string(out.join("files/a.md")).unwrap(), "a2");
    }

    #[cfg(unix)]
    #[test]
    fn one_changed_note_leaves_the_other_files_untouched() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &notes(&[("a.md", "a"), ("b.md", "b"), ("c.md", "c")])).unwrap();
        fs::write(out.join("user.md"), "user").unwrap();
        let inode = |path: &str| fs::metadata(out.join(path)).unwrap().ino();
        let before = [inode("b.md"), inode("c.md"), inode("user.md")];
        let report = publish(
            &out,
            &notes(&[("a.md", "a2"), ("b.md", "b"), ("c.md", "c")]),
        )
        .unwrap();
        assert_eq!(report.notes_written, 1);
        assert_eq!([inode("b.md"), inode("c.md"), inode("user.md")], before);
        assert_eq!(fs::read_to_string(out.join("a.md")).unwrap(), "a2");
    }

    /// Run by [`a_stopped_writer_does_not_block_the_next_one`] in a child
    /// process: publish and stop forever while holding the writer lock.
    #[test]
    #[ignore = "child process of a_stopped_writer_does_not_block_the_next_one"]
    fn hold_the_writer_lock_child() {
        let Some(out) = std::env::var_os("MCT_VAULT_LOCK_CHILD") else {
            return;
        };
        let out = PathBuf::from(out);
        let ready = sibling(&out, ".holding").unwrap();
        let _ = publish_with(
            &out,
            &notes(&[("index.md", "child")]),
            &|_, _| {
                fs::write(&ready, "")?;
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            },
            LOCK_WAIT,
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stopped_writer_does_not_block_the_next_one() {
        for signal in ["-KILL", "-INT"] {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().canonicalize().unwrap().join("vault");
            publish(&out, &notes(&[("index.md", "old")])).unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "manifest::tests::hold_the_writer_lock_child"])
                .args(["--ignored", "--test-threads=1"])
                .env("MCT_VAULT_LOCK_CHILD", &out)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let ready = sibling(&out, ".holding").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "child never held the lock"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let status = std::process::Command::new("kill")
                .args([signal, &child.id().to_string()])
                .status()
                .unwrap();
            assert!(status.success());
            child.wait().unwrap();
            fs::remove_file(&ready).unwrap();
            let result = publish(&out, &notes(&[("index.md", "new")]));
            assert!(result.is_ok(), "after {signal}: {result:?}");
            assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "new");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_ancestors_or_inside_the_tree_are_refused() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap().join("real");
        fs::create_dir_all(real.join("nested")).unwrap();
        symlink(&real, dir.path().join("alias")).unwrap();
        let notes = BTreeMap::from([("index.md".into(), "generated".into())]);
        assert!(publish(&dir.path().join("alias/nested/vault"), &notes).is_err());
        let out = real.join("vault");
        publish(&out, &notes).unwrap();
        symlink(&real, out.join("user-link")).unwrap();
        assert!(publish(&out, &notes).is_err());
        assert_eq!(
            fs::read_to_string(out.join("index.md")).unwrap(),
            "generated"
        );
    }
}
