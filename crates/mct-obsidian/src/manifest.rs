//! Manifest ownership and staged publication with rename rollback.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

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

struct WriterLock(PathBuf);
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Stage a complete vault beside the destination. Only manifest-owned files
/// may disappear; every other regular file is preserved. All validation and
/// copying happen before either rename. Concurrent writers fail closed.
pub fn publish(out: &Path, notes: &BTreeMap<String, String>) -> Result<VaultReport> {
    publish_with(out, notes, &|from, to| fs::rename(from, to))
}

fn publish_with(
    out: &Path,
    notes: &BTreeMap<String, String>,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
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
    let lock_path = sibling(out, ".lock")?;
    let lock_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| io_error(&lock_path, e))?;
    let _lock = WriterLock(lock_path);
    drop(lock_file);

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
    // Check actual contents as well as the manifest; local edits must not be
    // silently overwritten, including when an indexed file has been removed.
    for (path, expected) in &old.notes {
        let file = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath(path.clone()))?;
        check_ancestors(&file)?;
        match fs::read_to_string(&file) {
            Ok(text) if hash_note(&text) != *expected => {
                return Err(VaultError::Conflict(path.clone()))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(&file, e)),
        }
    }
    for path in notes.keys() {
        let file = safe_join(out, path).ok_or_else(|| VaultError::UnsafePath(path.clone()))?;
        check_ancestors(&file)?;
        if file.exists() && !old.notes.contains_key(path) {
            return Err(VaultError::Conflict(path.clone()));
        }
    }
    let reusable = |path: &str| {
        old.notes.get(path) == hashes.get(path)
            && safe_join(out, path).is_some_and(|file| is_real_file(&file))
    };
    let removed = old
        .notes
        .keys()
        .filter(|p| !hashes.contains_key(*p))
        .count();
    let changed = notes.keys().filter(|p| !reusable(p)).count();
    // Validate the whole existing tree even on no-op runs.
    validate_tree(out)?;
    if changed == 0 && removed == 0 && manifest_path.is_file() {
        return Ok(VaultReport {
            notes_written: 0,
            notes_unchanged: notes.len(),
            notes_removed: 0,
        });
    }
    let stage = tempfile::Builder::new()
        .prefix(".mct-obsidian-")
        .tempdir_in(parent)
        .map_err(|e| io_error(parent, e))?;
    let next = stage.path().join("next");
    fs::create_dir(&next).map_err(|e| io_error(&next, e))?;
    if out.exists() {
        copy_unmanaged(out, out, &next, &old)?;
    }
    for (path, text) in notes {
        let dest = safe_join(&next, path).ok_or_else(|| VaultError::UnsafePath(path.clone()))?;
        if dest.exists() {
            return Err(VaultError::Conflict(path.clone()));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
        }
        if reusable(path) {
            // Copy rather than hard-link: an editor of the old tree must not
            // mutate the staged note while publication is in progress.
            let source = out.join(path);
            fs::copy(&source, &dest).map_err(|e| io_error(&dest, e))?;
            let modified = fs::metadata(&source)
                .and_then(|m| m.modified())
                .map_err(|e| io_error(&source, e))?;
            fs::File::options()
                .write(true)
                .open(&dest)
                .and_then(|f| f.set_modified(modified))
                .map_err(|e| io_error(&dest, e))?;
        } else {
            fs::write(&dest, text).map_err(|e| io_error(&dest, e))?;
        }
    }
    let file = next.join(MANIFEST_FILE);
    fs::write(&file, Manifest { notes: hashes }.render()).map_err(|e| io_error(&file, e))?;
    let previous = stage.path().join("previous");
    let had_old = out.exists();
    if had_old {
        rename(out, &previous).map_err(|e| io_error(out, e))?;
    }
    if let Err(err) = rename(&next, out) {
        if had_old {
            if let Err(rollback) = rename(&previous, out) {
                let recovery = stage.keep();
                return Err(io_error(&recovery, std::io::Error::other(format!("publication failed: {err}; rollback failed: {rollback}; previous vault retained here"))));
            }
        }
        return Err(io_error(out, err));
    }
    Ok(VaultReport {
        notes_written: changed,
        notes_unchanged: notes.len() - changed,
        notes_removed: removed,
    })
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
        }
    }
    Ok(())
}

fn manifest_key(relative: &Path) -> Option<String> {
    relative
        .iter()
        .map(|part| part.to_str())
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn copy_unmanaged(base: &Path, dir: &Path, next: &Path, old: &Manifest) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| io_error(dir, e))? {
        let entry = entry.map_err(|e| io_error(dir, e))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(base)
            .map_err(|_| VaultError::UnsafePath(path.display().to_string()))?;
        // Compare actual path components. On Unix a literal backslash is
        // part of a user filename, not a separator; lossy conversion could
        // otherwise mistake an unmanaged file for a generated note.
        let key = manifest_key(relative);
        if key.as_deref() == Some(MANIFEST_FILE)
            || key.as_ref().is_some_and(|key| old.notes.contains_key(key))
        {
            continue;
        }
        let dest = next.join(relative);
        let kind = entry.file_type().map_err(|e| io_error(&path, e))?;
        if kind.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| io_error(&dest, e))?;
            copy_unmanaged(base, &path, next, old)?;
        } else if kind.is_file() {
            fs::copy(&path, &dest).map_err(|e| io_error(&dest, e))?;
        } else {
            return Err(VaultError::Symlink(path.display().to_string()));
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
    fn failed_publication_rolls_back_the_complete_old_tree() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &BTreeMap::from([("index.md".into(), "old".into())])).unwrap();
        fs::create_dir(out.join(".obsidian")).unwrap();
        fs::write(out.join(".obsidian/settings.json"), "settings").unwrap();
        fs::write(out.join("personal.md"), "personal").unwrap();
        let manifest = fs::read(out.join(MANIFEST_FILE)).unwrap();
        let result = publish_with(
            &out,
            &BTreeMap::from([("index.md".into(), "new".into())]),
            &|from, to| {
                if from.file_name().is_some_and(|name| name == "next") {
                    Err(std::io::Error::other("injected rename failure"))
                } else {
                    fs::rename(from, to)
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(out.join("index.md")).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(out.join("personal.md")).unwrap(),
            "personal"
        );
        assert_eq!(
            fs::read_to_string(out.join(".obsidian/settings.json")).unwrap(),
            "settings"
        );
        assert_eq!(fs::read(out.join(MANIFEST_FILE)).unwrap(), manifest);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn rollback_failure_retains_the_previous_vault_at_the_reported_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().canonicalize().unwrap().join("vault");
        publish(&out, &BTreeMap::from([("index.md".into(), "old".into())])).unwrap();
        let result = publish_with(
            &out,
            &BTreeMap::from([("index.md".into(), "new".into())]),
            &|from, to| {
                if from == out {
                    fs::rename(from, to)
                } else {
                    Err(std::io::Error::other("injected rename failure"))
                }
            },
        );
        let VaultError::Io { path, source } = result.unwrap_err() else {
            panic!("expected recovery diagnostic")
        };
        assert!(source.to_string().contains("rollback failed"));
        let retained = Path::new(&path);
        assert_eq!(
            fs::read_to_string(retained.join("previous/index.md")).unwrap(),
            "old"
        );
        assert!(!sibling(&out, ".lock").unwrap().exists());
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

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_cannot_be_mistaken_for_manifest_owned_paths() {
        use std::os::unix::ffi::OsStringExt;
        let name = PathBuf::from(std::ffi::OsString::from_vec(vec![b'u', 0xff]));
        assert_eq!(manifest_key(&name), None);
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
