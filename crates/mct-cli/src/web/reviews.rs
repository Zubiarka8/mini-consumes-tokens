//! User review decisions, separate from the derived analysis and SQLite index.
//!
//! A finding (duplicate group or dead-code candidate) without an entry is
//! "unreviewed". Entries are never deleted by the web UI: reopening a finding
//! removes its current decision but the append-only history keeps every
//! transition, so false positives stay auditable and recoverable.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use mct_index::{DuplicateGroup, NavigationSnapshot, Normalization, SymbolListEntry};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REVIEWS: usize = 100_000;
const MAX_HISTORY: usize = 100_000;
pub(super) const MAX_NOTE_CHARS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    InReview,
    Confirmed,
    FalsePositive,
    Resolved,
    Dismissed,
}

impl State {
    pub const ALL: [State; 5] = [
        Self::InReview,
        Self::Confirmed,
        Self::FalsePositive,
        Self::Resolved,
        Self::Dismissed,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.value() == value)
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::InReview => "in-review",
            Self::Confirmed => "confirmed",
            Self::FalsePositive => "false-positive",
            Self::Resolved => "resolved",
            Self::Dismissed => "dismissed",
        }
    }
}

fn state_or_none(value: Option<State>) -> &'static str {
    value.map_or("none", State::value)
}

fn parse_optional_state(value: &str) -> anyhow::Result<Option<State>> {
    if value == "none" {
        return Ok(None);
    }
    State::parse(value)
        .map(Some)
        .context("invalid review state")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Review {
    pub state: State,
    pub updated: u64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub key: String,
    pub from: Option<State>,
    pub to: Option<State>,
    pub at: u64,
    pub note: String,
}

#[derive(Default)]
struct Snapshot {
    entries: BTreeMap<String, Review>,
    history: Vec<Event>,
}

pub(super) struct Reviews {
    path: PathBuf,
    entries: BTreeMap<String, Review>,
    history: Vec<Event>,
}

impl Reviews {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let path = root.join(".mct-index/duplicate-reviews.toon");
        let Snapshot { entries, history } = read(&path)?;
        Ok(Self {
            path,
            entries,
            history,
        })
    }

    pub fn get(&self, key: &str) -> Option<&Review> {
        self.entries.get(key)
    }

    pub fn state(&self, key: &str) -> Option<State> {
        self.get(key).map(|review| review.state)
    }

    pub fn events(&self) -> &[Event] {
        &self.history
    }

    pub fn history<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Event> {
        self.history.iter().filter(move |event| event.key == key)
    }

    /// Records a decision (`Some`) or reopens the finding (`None`) and appends
    /// the transition to the history. Setting the current state again is a no-op.
    pub fn update(
        &mut self,
        key: &str,
        state: Option<State>,
        note: &str,
        now: u64,
    ) -> anyhow::Result<()> {
        ensure!(valid_key(key), "invalid review key");
        ensure!(
            note.chars().count() <= MAX_NOTE_CHARS && !note.chars().any(char::is_control),
            "invalid review note"
        );
        safe_path(&self.path)?;
        let parent = self
            .path
            .parent()
            .context("review file needs a parent directory")?;
        let lock_path = parent.join("duplicate-reviews.lock");
        let lock_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .context("cannot lock reviews; another writer may be active")?;
        let _lock = Lock {
            path: lock_path,
            file: Some(lock_file),
        };
        // Reload under the writer lock: different web processes must not lose
        // each other's decisions. Publish before changing this process's view.
        let Snapshot {
            mut entries,
            mut history,
        } = read(&self.path)?;
        let previous = entries.get(key).map(|review| review.state);
        if previous == state {
            return Ok(());
        }
        match state {
            Some(state) => {
                entries.insert(
                    key.to_string(),
                    Review {
                        state,
                        updated: now,
                        note: note.to_string(),
                    },
                );
            }
            None => {
                entries.remove(key);
            }
        }
        history.push(Event {
            key: key.to_string(),
            from: previous,
            to: state,
            at: now,
            note: note.to_string(),
        });
        ensure!(entries.len() <= MAX_REVIEWS, "too many reviews");
        ensure!(history.len() <= MAX_HISTORY, "review history is full");
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        write_snapshot(&mut temp, &entries, &history)?;
        temp.as_file_mut().sync_all()?;
        safe_path(&self.path)?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("cannot publish reviews")?;
        self.entries = entries;
        self.history = history;
        Ok(())
    }
}

fn encode(note: &str) -> String {
    form_urlencoded::byte_serialize(note.as_bytes()).collect()
}

fn decode(note: &str) -> String {
    form_urlencoded::parse(format!("n={note}").as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn write_snapshot(
    out: &mut impl Write,
    entries: &BTreeMap<String, Review>,
    history: &[Event],
) -> std::io::Result<()> {
    writeln!(
        out,
        "version: 2\nreviews[{}]{{key,state,updated,note}}:",
        entries.len()
    )?;
    for (key, review) in entries {
        writeln!(
            out,
            "  {key},{},{},{}",
            review.state.value(),
            review.updated,
            encode(&review.note)
        )?;
    }
    writeln!(out, "history[{}]{{key,from,to,at,note}}:", history.len())?;
    for event in history {
        writeln!(
            out,
            "  {},{},{},{},{}",
            event.key,
            state_or_none(event.from),
            state_or_none(event.to),
            event.at,
            encode(&event.note)
        )?;
    }
    Ok(())
}

struct Lock {
    path: PathBuf,
    file: Option<fs::File>,
}
impl Drop for Lock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn safe_path(path: &Path) -> anyhow::Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "refusing review state through a symlink"
                );
                if ancestor == path {
                    ensure!(metadata.is_file(), "review state is not a regular file");
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        ensure!(metadata.nlink() == 1, "refusing hard-linked review state");
                    }
                } else {
                    ensure!(metadata.is_dir(), "review ancestor is not a directory");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn read(path: &Path) -> anyhow::Result<Snapshot> {
    safe_path(path)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Snapshot::default())
        }
        Err(error) => return Err(error.into()),
    };
    let mut content = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut content)?;
    ensure!(
        content.len() as u64 <= MAX_BYTES,
        "review state is too large"
    );
    parse(&content)
}

/// Reads a `name[count]{columns}:` table header and returns the row count.
fn table_header(line: Option<&str>, name: &str, columns: &str) -> anyhow::Result<usize> {
    let count: usize = line
        .and_then(|line| line.strip_prefix(name))
        .and_then(|line| line.strip_prefix('['))
        .and_then(|line| line.strip_suffix(&format!("]{{{columns}}}:")))
        .context("invalid review table")?
        .parse()?;
    ensure!(count <= MAX_HISTORY, "too many review rows");
    Ok(count)
}

fn parse(content: &str) -> anyhow::Result<Snapshot> {
    let mut lines = content.lines();
    match lines.next() {
        Some("version: 1") => parse_v1(lines),
        Some("version: 2") => parse_v2(lines),
        _ => anyhow::bail!("unsupported review format"),
    }
}

fn row(line: &str) -> anyhow::Result<&str> {
    line.strip_prefix("  ").context("invalid review row")
}

fn parse_v1<'a>(mut lines: impl Iterator<Item = &'a str>) -> anyhow::Result<Snapshot> {
    let count = table_header(lines.next(), "reviews", "key,reason")?;
    ensure!(count <= MAX_REVIEWS, "too many reviews");
    let mut entries = BTreeMap::new();
    for line in lines {
        let (key, reason) = row(line)?.split_once(',').context("invalid review row")?;
        ensure!(valid_key(key), "invalid review key");
        let state = State::parse(reason).context("invalid review reason")?;
        let review = Review {
            state,
            updated: 0,
            note: String::new(),
        };
        ensure!(
            entries.insert(key.to_string(), review).is_none(),
            "duplicate review key"
        );
    }
    ensure!(entries.len() == count, "review count mismatch");
    Ok(Snapshot {
        entries,
        history: Vec::new(),
    })
}

fn parse_v2<'a>(mut lines: impl Iterator<Item = &'a str>) -> anyhow::Result<Snapshot> {
    let count = table_header(lines.next(), "reviews", "key,state,updated,note")?;
    ensure!(count <= MAX_REVIEWS, "too many reviews");
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let line = lines.next().context("missing review row")?;
        let mut fields = row(line)?.splitn(4, ',');
        let (Some(key), Some(state), Some(updated), Some(note)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            anyhow::bail!("invalid review row");
        };
        ensure!(valid_key(key), "invalid review key");
        let review = Review {
            state: State::parse(state).context("invalid review state")?,
            updated: updated.parse()?,
            note: decode(note),
        };
        ensure!(
            entries.insert(key.to_string(), review).is_none(),
            "duplicate review key"
        );
    }
    let count = table_header(lines.next(), "history", "key,from,to,at,note")?;
    let mut history = Vec::new();
    for line in lines {
        let mut fields = row(line)?.splitn(5, ',');
        let (Some(key), Some(from), Some(to), Some(at), Some(note)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            anyhow::bail!("invalid review history row");
        };
        ensure!(valid_key(key), "invalid review key");
        history.push(Event {
            key: key.to_string(),
            from: parse_optional_state(from)?,
            to: parse_optional_state(to)?,
            at: at.parse()?,
            note: decode(note),
        });
    }
    ensure!(history.len() == count, "review history count mismatch");
    Ok(Snapshot { entries, history })
}

fn hex(bytes: &[u8]) -> String {
    hmac_sha256::Hash::hash(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn framed(fields: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in fields {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes
}

pub(super) fn group_keys(
    groups: &[DuplicateGroup],
    navigation: &NavigationSnapshot,
    normalization: Normalization,
) -> anyhow::Result<Vec<String>> {
    let hashes = file_hashes(navigation);
    groups
        .iter()
        .map(|group| {
            let mut members = Vec::new();
            for member in &group.members {
                let hash = hashes
                    .get(member.path.as_str())
                    .context("duplicate member missing from navigation snapshot")?;
                members.push((
                    &member.path,
                    &member.symbol_name,
                    &member.symbol_kind,
                    member.start_line,
                    member.end_line,
                    *hash,
                ));
            }
            members.sort();
            let mut fields = vec![
                "duplicate-review-v1".to_string(),
                normalization.as_str().to_string(),
                group.language.clone(),
            ];
            for (path, name, kind, start, end, hash) in members {
                fields.extend([
                    path.clone(),
                    name.clone(),
                    kind.clone(),
                    start.to_string(),
                    end.to_string(),
                    hash.to_string(),
                ]);
            }
            let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            Ok(hex(&framed(&fields)))
        })
        .collect()
}

/// Stable identity of a dead-code candidate: it changes with the symbol's
/// location or with the file content, so a stale decision never hides new code.
pub(super) fn dead_code_key(
    symbol: &SymbolListEntry,
    navigation: &NavigationSnapshot,
) -> Option<String> {
    let hash = *file_hashes(navigation).get(symbol.relative_path.as_str())?;
    let line = symbol.line.to_string();
    Some(hex(&framed(&[
        "dead-code-review-v1",
        &symbol.relative_path,
        &symbol.name,
        &symbol.kind,
        &line,
        hash,
    ])))
}

pub(super) fn file_hashes(navigation: &NavigationSnapshot) -> BTreeMap<&str, &str> {
    navigation
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".mct-index")).unwrap();
        root
    }

    #[test]
    fn decisions_persist_reopen_and_merge_between_writers() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let other = "b".repeat(64);
        let mut first = Reviews::load(&path).unwrap();
        let mut second = Reviews::load(&path).unwrap();
        assert!(!path.join(".mct-index/duplicate-reviews.toon").exists());
        first
            .update(&key, Some(State::FalsePositive), "generated, 1,2", 10)
            .unwrap();
        second
            .update(&other, Some(State::Dismissed), "", 11)
            .unwrap();
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.state(&key), Some(State::FalsePositive));
        assert_eq!(current.get(&key).unwrap().note, "generated, 1,2");
        assert_eq!(current.state(&other), Some(State::Dismissed));
        first.update(&key, None, "code changed", 12).unwrap();
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.state(&key), None);
        assert_eq!(current.state(&other), Some(State::Dismissed));
        let history: Vec<_> = current.history(&key).collect();
        assert_eq!(history.len(), 2, "reopening keeps the audit trail");
        assert_eq!(history[0].from, None);
        assert_eq!(history[1].from, Some(State::FalsePositive));
        assert_eq!(history[1].to, None);
        assert_eq!(history[1].note, "code changed");
    }

    #[test]
    fn repeating_the_current_state_does_not_grow_the_history() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "c".repeat(64);
        let mut reviews = Reviews::load(&path).unwrap();
        reviews.update(&key, Some(State::Confirmed), "", 1).unwrap();
        reviews.update(&key, Some(State::Confirmed), "", 2).unwrap();
        assert_eq!(Reviews::load(&path).unwrap().history(&key).count(), 1);
    }

    #[test]
    fn locking_and_corruption_preserve_saved_and_visible_decisions() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let mut reviews = Reviews::load(&path).unwrap();
        reviews
            .update(&key, Some(State::FalsePositive), "", 1)
            .unwrap();
        let saved = fs::read(&reviews.path).unwrap();
        let lock = path.join(".mct-index/duplicate-reviews.lock");
        fs::write(&lock, "another writer").unwrap();
        assert!(reviews.update(&key, None, "", 2).is_err());
        assert_eq!(fs::read(&reviews.path).unwrap(), saved);
        assert_eq!(reviews.state(&key), Some(State::FalsePositive));
        assert_eq!(fs::read_to_string(&lock).unwrap(), "another writer");
        fs::remove_file(lock).unwrap();
        fs::write(&reviews.path, "user data / corrupt state").unwrap();
        assert!(reviews.update(&key, None, "", 3).is_err());
        assert_eq!(reviews.state(&key), Some(State::FalsePositive));
        assert_eq!(
            fs::read_to_string(&reviews.path).unwrap(),
            "user data / corrupt state"
        );
        assert!(!path.join(".mct-index/duplicate-reviews.lock").exists());
    }

    #[test]
    fn invalid_notes_versions_rows_counts_and_keys_fail_closed() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let mut reviews = Reviews::load(&path).unwrap();
        assert!(reviews
            .update(&key, Some(State::Confirmed), "line\nbreak", 1)
            .is_err());
        assert!(reviews
            .update(&key, Some(State::Confirmed), &"x".repeat(501), 1)
            .is_err());
        for content in [
            "version: 3\nreviews[0]{key,reason}:\n".to_string(),
            "version: 1\nreviews[1]{key,reason}:\n".to_string(),
            "version: 1\nreviews[1]{key,reason}:\n  ../../file,false-positive\n".to_string(),
            format!("version: 1\nreviews[1]{{key,reason}}:\n  {key},unknown\n"),
            format!("version: 1\nreviews[2]{{key,reason}}:\n  {key},dismissed\n  {key},false-positive\n"),
            "version: 1\nreviews[100001]{key,reason}:\n".to_string(),
            "version: 2\nreviews[0]{key,state,updated,note}:\n".to_string(),
            format!("version: 2\nreviews[1]{{key,state,updated,note}}:\n  {key},confirmed,x,\nhistory[0]{{key,from,to,at,note}}:\n"),
            format!("version: 2\nreviews[0]{{key,state,updated,note}}:\nhistory[1]{{key,from,to,at,note}}:\n  {key},none,bogus,1,\n"),
        ] {
            assert!(parse(&content).is_err(), "{content}");
        }
        assert!(parse("version: 1\nreviews[0]{key,reason}:\n")
            .unwrap()
            .entries
            .is_empty());
    }

    #[test]
    fn version_one_files_are_read_and_upgraded_on_the_next_write() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let other = "b".repeat(64);
        fs::write(
            path.join(".mct-index/duplicate-reviews.toon"),
            format!("version: 1\nreviews[1]{{key,reason}}:\n  {key},dismissed\n"),
        )
        .unwrap();
        let mut reviews = Reviews::load(&path).unwrap();
        assert_eq!(reviews.state(&key), Some(State::Dismissed));
        reviews
            .update(&other, Some(State::InReview), "look later", 5)
            .unwrap();
        let text = fs::read_to_string(&reviews.path).unwrap();
        assert!(text.starts_with("version: 2\n"), "{text}");
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.state(&key), Some(State::Dismissed));
        assert_eq!(current.state(&other), Some(State::InReview));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_hard_links_cannot_redirect_review_state() {
        use std::os::unix::fs::symlink;
        let root = project();
        let root_path = root.path().canonicalize().unwrap();
        let file = root_path.join(".mct-index/duplicate-reviews.toon");
        let external = root_path.join("outside.toon");
        let content = "version: 1\nreviews[0]{key,reason}:\n";
        fs::write(&external, content).unwrap();
        symlink(&external, &file).unwrap();
        assert!(Reviews::load(&root_path).is_err());
        fs::remove_file(&file).unwrap();
        fs::hard_link(&external, &file).unwrap();
        assert!(Reviews::load(&root_path).is_err());
        assert_eq!(fs::read_to_string(external).unwrap(), content);
        fs::remove_file(&file).unwrap();
        let link_root = root_path.join("alias");
        symlink(root_path.join(".mct-index"), &link_root).unwrap();
        assert!(read(&link_root.join("duplicate-reviews.toon")).is_err());
    }
}

#[cfg(test)]
mod size_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    #[test]
    fn oversized_state_is_rejected_before_parsing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap().join("state.toon");
        std::fs::write(&path, vec![b'a'; super::MAX_BYTES as usize + 1]).unwrap();
        assert!(super::read(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("too large"));
    }
}
