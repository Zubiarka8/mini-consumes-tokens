//! Discovery, bounded reads and the atomic write of review files.
//!
//! Reading never fails: a file that exists but cannot be read makes the
//! whole store degraded, which hides nothing and refuses every write.
//! Writing touches only `review-decisions.toon`, under an exclusive lock,
//! through a synced temporary file and an atomic rename, after checking that
//! the new file keeps every earlier row and history event.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::format::{
    parse_canonical, parse_draft, parse_legacy, render_canonical, valid_note, Canonical, Event,
    Row, MAX_ROWS, NEWER_FORMAT,
};
use super::resolve::{Resolver, Sources};
use super::{Diagnostics, FindingKeys, Format, HistoryEvent, Resolution, State};

/// The only review file this module writes.
pub const CANONICAL_FILE: &str = "review-decisions.toon";
const DRAFT_FILE: &str = "reviews.toon";
const LEGACY_FILE: &str = "duplicate-reviews.toon";
const LOCK_FILE: &str = "review-decisions.lock";
/// The canonical file as it was before the last save.
const PREVIOUS_FILE: &str = "review-decisions.toon.prev";
const TEMPORARY_SUFFIX: &str = ".tmp";
const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("review files are read-only until this is fixed: {0}")]
    Degraded(String),
    #[error("another writer is saving review decisions ({path}); try again when it has finished")]
    Locked { path: String },
    #[error("the review changed since it was displayed; reload to see the current state")]
    Conflict,
    #[error("invalid review request: {0}")]
    Invalid(&'static str),
    #[error("review file is full: {0}; nothing was dropped or written")]
    Full(&'static str),
    #[error("review write rejected by its safety check: {0}")]
    Invariant(&'static str),
    #[error("review file I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ReviewError + '_ {
    move |source| ReviewError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// A review file that exists but cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProblem {
    pub path: PathBuf,
    pub reason: String,
    /// The canonical file was written by a newer version.
    pub newer_format: bool,
}

/// A decision requested by a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `Unreviewed` restores the finding.
    pub state: State,
    pub note: String,
    /// [`Resolution::revision`] as the person saw it.
    pub expected_revision: String,
}

/// Every review source of one project, read once.
#[derive(Clone, Debug)]
pub struct Reviews {
    directory: PathBuf,
    sources: Sources,
    read: Vec<(Format, PathBuf)>,
    problems: Vec<SourceProblem>,
    leftover_temporary_files: usize,
}

/// Refuses symlinked ancestors, non-regular files and (on Unix) hard links,
/// so review state is never read or replaced through another path.
fn safe_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err("refusing review state through a symlink".into());
                }
                if ancestor == path {
                    if !metadata.is_file() {
                        return Err("review state is not a regular file".into());
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if metadata.nlink() != 1 {
                            return Err(HARD_LINK.into());
                        }
                    }
                } else if !metadata.is_dir() {
                    return Err("review ancestor is not a directory".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn open(path: &Path) -> std::io::Result<fs::File> {
    let opened = fs::File::open(path);
    // A reader racing a rename on Windows can see a sharing violation once.
    #[cfg(windows)]
    if matches!(&opened, Err(error) if error.raw_os_error() == Some(32)) {
        std::thread::sleep(std::time::Duration::from_millis(50));
        return fs::File::open(path);
    }
    opened
}

const HARD_LINK: &str = "refusing hard-linked review state";

/// `safe_path`, retried briefly when the file looks hard-linked: a writer's
/// atomic rename can expose a second link for an instant (observed on
/// macOS). A real hard link stays and is still refused.
fn safe_source(path: &Path) -> Result<(), String> {
    for _ in 0..5 {
        match safe_path(path) {
            Err(reason) if reason == HARD_LINK => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            other => return other,
        }
    }
    safe_path(path)
}

/// The file's text, `None` when it does not exist.
fn read_source(path: &Path) -> Result<Option<String>, String> {
    safe_source(path)?;
    let file = match open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut content = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut content)
        .map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_BYTES {
        return Err("file is larger than 32 MiB".into());
    }
    if content.is_empty() {
        return Err("file is empty".into());
    }
    Ok(Some(content))
}

impl Reviews {
    /// Reads every review file under `<root>/.mct-index`. Never fails: see
    /// [`Reviews::is_degraded`].
    pub fn load(root: &Path) -> Reviews {
        // Links above the project root are the user's choice; from the root
        // down, every component is checked by `safe_path`.
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        Self::load_directory(&root.join(".mct-index"))
    }

    fn load_directory(directory: &Path) -> Reviews {
        let mut reviews = Reviews {
            directory: directory.to_path_buf(),
            sources: Sources::default(),
            read: Vec::new(),
            problems: Vec::new(),
            leftover_temporary_files: count_temporary_files(directory),
        };
        let mut problem = |path: PathBuf, reason: String| {
            let newer_format = reason.ends_with(NEWER_FORMAT);
            reviews.problems.push(SourceProblem {
                path,
                reason,
                newer_format,
            });
        };
        let path = directory.join(CANONICAL_FILE);
        match read_source(&path) {
            Ok(None) => (),
            Ok(Some(text)) => match parse_canonical(&text) {
                Ok(canonical) => {
                    reviews.sources.canonical = canonical;
                    reviews.read.push((Format::C3, path));
                }
                Err(error) => problem(path, error.to_string()),
            },
            Err(reason) => problem(path, reason),
        }
        let path = directory.join(DRAFT_FILE);
        match read_source(&path) {
            Ok(None) => (),
            Ok(Some(text)) => match parse_draft(&text) {
                Ok(draft) => {
                    reviews.sources.draft = draft;
                    reviews.read.push((Format::B2, path));
                }
                Err(error) => problem(path, error.to_string()),
            },
            Err(reason) => problem(path, reason),
        }
        let path = directory.join(LEGACY_FILE);
        match read_source(&path) {
            Ok(None) => (),
            Ok(Some(text)) => match parse_legacy(&text) {
                Ok(legacy) => {
                    let format = if legacy.version == 1 {
                        Format::A1
                    } else {
                        Format::A2
                    };
                    reviews.sources.legacy = legacy;
                    reviews.read.push((format, path));
                }
                Err(error) => problem(path, error.to_string()),
            },
            Err(reason) => problem(path, reason),
        }
        if !reviews.problems.is_empty() {
            // Nothing from a partial read may hide a finding.
            reviews.sources = Sources::default();
        }
        reviews
    }

    /// True when a present file cannot be read: nothing is hidden, no state
    /// is reported and every save is refused until a person fixes the file.
    pub fn is_degraded(&self) -> bool {
        !self.problems.is_empty()
    }

    pub fn problems(&self) -> &[SourceProblem] {
        &self.problems
    }

    /// The files that were read, with their format.
    pub fn sources(&self) -> &[(Format, PathBuf)] {
        &self.read
    }

    /// Temporary files left by an interrupted save. They never hold the only
    /// copy of anything. Loading never removes them; the next save does, under
    /// the writer lock.
    pub fn leftover_temporary_files(&self) -> usize {
        self.leftover_temporary_files
    }

    /// The resolution of each finding, in order. `findings` must be every
    /// current finding of the scope, so ambiguous legacy keys are detected.
    pub fn resolve(&self, findings: &[FindingKeys]) -> Vec<Resolution> {
        let resolver = Resolver::new(&self.sources, findings);
        findings.iter().map(|f| resolver.resolve(f)).collect()
    }

    /// The history of each finding, in order, oldest event first.
    pub fn histories(&self, findings: &[FindingKeys]) -> Vec<Vec<HistoryEvent>> {
        let resolver = Resolver::new(&self.sources, findings);
        findings.iter().map(|f| resolver.history(f)).collect()
    }

    pub fn diagnostics(&self, findings: &[FindingKeys], now: u64) -> Diagnostics {
        Resolver::new(&self.sources, findings).diagnostics(findings, now)
    }

    /// Records `change` for `findings[target]` in `review-decisions.toon`,
    /// `now` being Unix milliseconds from the server clock.
    /// On success, and on [`ReviewError::Conflict`], `self` holds the files
    /// as they are now on disk; on any error nothing was written, and on
    /// errors other than a conflict `self` is unchanged.
    pub fn save(
        &mut self,
        findings: &[FindingKeys],
        target: usize,
        change: &Change,
        now: u64,
    ) -> Result<(), ReviewError> {
        let finding = findings
            .get(target)
            .ok_or(ReviewError::Invalid("unknown finding"))?;
        if !valid_note(&change.note) {
            return Err(ReviewError::Invalid(
                "notes have at most 500 characters and no control characters",
            ));
        }
        let _lock = Lock::acquire(&self.directory)?;
        // Re-read under the lock: other writers must not be overwritten, and
        // the person's view must still be current.
        let current = Self::load_directory(&self.directory);
        if let Some(problem) = current.problems.first() {
            return Err(ReviewError::Degraded(format!(
                "{}: {}",
                problem.path.display(),
                problem.reason
            )));
        }
        // Holding the lock, no live writer can be using a temporary file.
        remove_abandoned_temporaries(&self.directory);
        let resolver = Resolver::new(&current.sources, findings);
        let resolution = resolver.resolve(finding);
        if resolution.revision() != change.expected_revision {
            // Nothing is written, but the caller now sees the newer state.
            *self = current;
            return Err(ReviewError::Conflict);
        }
        let unchanged = resolution.state() == change.state
            && (change.state == State::Unreviewed || !resolution.needs_reevaluation());
        if unchanged {
            *self = current;
            return Ok(());
        }
        // Logical time: the new decision is always the latest one for this
        // finding, even if the clock went back or a row has a future time.
        let at = match resolver.latest_time(finding).checked_add(1) {
            Some(after) => after.max(now),
            None => return Err(ReviewError::Full("the latest review time is the maximum")),
        };
        let mut next = current.sources.canonical.clone();
        let key = (finding.kind, finding.identity.clone());
        let row = Row {
            kind: finding.kind,
            identity: finding.identity.clone(),
            fingerprint: finding.fingerprint.clone(),
            state: change.state,
            updated: at,
            note: change.note.clone(),
        };
        next.reviews.insert(key, row.clone());
        next.history.push(Event {
            kind: finding.kind,
            identity: finding.identity.clone(),
            fingerprint: finding.fingerprint.clone(),
            from: resolution.state(),
            to: change.state,
            at,
            note: change.note.clone(),
        });
        if next.reviews.len() > MAX_ROWS {
            return Err(ReviewError::Full("too many reviews"));
        }
        if next.history.len() > MAX_ROWS {
            return Err(ReviewError::Full("review history is full"));
        }
        let text = render_canonical(&next);
        if text.len() as u64 > MAX_BYTES {
            return Err(ReviewError::Full("the file would exceed 32 MiB"));
        }
        verify_append(&current.sources.canonical, &text, &row)?;
        // Only writers holding the lock change this file, so these are the
        // bytes `current` was parsed from.
        let canonical = self.directory.join(CANONICAL_FILE);
        if let Some(previous) = read_source(&canonical).map_err(ReviewError::Degraded)? {
            publish(&self.directory, PREVIOUS_FILE, &previous)?;
        }
        publish(&self.directory, CANONICAL_FILE, &text)?;
        let mut saved = current;
        saved.sources.canonical = next;
        if !saved.read.iter().any(|(format, _)| *format == Format::C3) {
            saved
                .read
                .insert(0, (Format::C3, self.directory.join(CANONICAL_FILE)));
        }
        *self = saved;
        Ok(())
    }
}

/// The new file must parse, keep every earlier history event in order with
/// exactly one appended, and keep every earlier current row except the one
/// being saved, which must be exactly `row`.
fn verify_append(before: &Canonical, text: &str, row: &Row) -> Result<(), ReviewError> {
    let after =
        parse_canonical(text).map_err(|_| ReviewError::Invariant("the new file does not parse"))?;
    if after.history.len() != before.history.len() + 1
        || after.history[..before.history.len()] != before.history[..]
    {
        return Err(ReviewError::Invariant("history would not be append-only"));
    }
    let key = (row.kind, row.identity.clone());
    if after.reviews.get(&key) != Some(row) {
        return Err(ReviewError::Invariant(
            "the saved decision does not round-trip",
        ));
    }
    let others_kept = before
        .reviews
        .iter()
        .filter(|(other, _)| **other != key)
        .all(|(other, kept)| after.reviews.get(other) == Some(kept));
    let expected_len = before.reviews.len() + usize::from(!before.reviews.contains_key(&key));
    if !others_kept || after.reviews.len() != expected_len {
        return Err(ReviewError::Invariant("other decisions would change"));
    }
    Ok(())
}

/// Atomically replaces `directory/name` with `text`: a new temporary file in
/// the same directory, synced, then renamed over the target.
fn publish(directory: &Path, name: &str, text: &str) -> Result<(), ReviewError> {
    let target = directory.join(name);
    safe_path(&target).map_err(ReviewError::Degraded)?;
    let temporary = Temporary::create(directory, name)?;
    {
        let mut file = &temporary.file;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(io_error(&temporary.path))?;
    }
    safe_path(&target).map_err(ReviewError::Degraded)?;
    fs::rename(&temporary.path, &target).map_err(io_error(&target))?;
    temporary.persisted();
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(io_error(directory))?;
    Ok(())
}

/// `review-decisions.toon.<target>.<pid>.<nanos>.<attempt>.tmp`, the names
/// [`Temporary::create`] gives to the files this module publishes.
fn is_own_temporary(name: &str) -> bool {
    let Some(rest) = name
        .strip_prefix(CANONICAL_FILE)
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(TEMPORARY_SUFFIX))
    else {
        return false;
    };
    let mut parts = rest.rsplitn(4, '.');
    let (Some(attempt), Some(nanos), Some(pid), Some(target)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    [CANONICAL_FILE, PREVIOUS_FILE].contains(&target)
        && [attempt, nanos, pid].into_iter().all(digits)
}

/// Regular files in `directory` that an interrupted save of this module left.
fn temporary_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(is_own_temporary)
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
        .map(|entry| entry.path())
        .collect()
}

fn count_temporary_files(directory: &Path) -> usize {
    temporary_files(directory).len()
}

/// Removes what killed saves left behind. Best effort: a file that cannot be
/// removed stays, is counted on the next load, and hides nothing.
fn remove_abandoned_temporaries(directory: &Path) {
    for path in temporary_files(directory) {
        let _ = fs::remove_file(path);
    }
}

/// A temporary file removed on drop unless it was renamed into place.
struct Temporary {
    path: PathBuf,
    file: fs::File,
    keep: bool,
}

impl Temporary {
    fn create(directory: &Path, name: &str) -> Result<Self, ReviewError> {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        for attempt in 0..16_u32 {
            // Named after the canonical file even for `.prev`, so leftovers of
            // every kind are found by one pattern.
            let path = directory.join(format!(
                "{CANONICAL_FILE}.{name}.{}.{nanos}.{attempt}{TEMPORARY_SUFFIX}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file,
                        keep: false,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error(&path)(error)),
            }
        }
        Err(ReviewError::Invariant(
            "cannot create a unique temporary file",
        ))
    }

    fn persisted(mut self) {
        self.keep = true;
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// The exclusive writer lock: an advisory lock the operating system holds on
/// `review-decisions.lock` while this value lives. The kernel releases it when
/// the holder exits, however it exits, so a killed writer cannot block the
/// next one. The file itself is never removed: deleting a locked file would
/// let two writers lock different files under the same name.
///
/// If the platform cannot lock the file, the save fails with an I/O error.
struct Lock {
    _file: fs::File,
}

impl Lock {
    fn acquire(directory: &Path) -> Result<Self, ReviewError> {
        let path = directory.join(LOCK_FILE);
        safe_path(&path).map_err(ReviewError::Degraded)?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io_error(&path))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(fs::TryLockError::WouldBlock) => Err(ReviewError::Locked {
                path: path.display().to_string(),
            }),
            Err(fs::TryLockError::Error(error)) => Err(io_error(&path)(error)),
        }
    }
}
