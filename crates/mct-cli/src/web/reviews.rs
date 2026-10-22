//! User review decisions for duplicate and dead-code findings (ADR-018),
//! separate from the derived analysis and the SQLite index.
//!
//! Two files live in `.mct-index/`:
//! - `reviews.toon` (version 2, read and written here): the current decision and
//!   the append-only history of every finding, keyed by a stable *identity*.
//! - `duplicate-reviews.toon` (version 1, ADR-015): read only, never rewritten.
//!   Its keys are fingerprints, so they match a duplicate group exactly.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};

use super::i18n::{t, Lang};
use mct_index::{DuplicateGroup, NavigationSnapshot, Normalization, SymbolListEntry};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_REVIEWS: usize = 100_000;
const MAX_HISTORY: usize = 50_000;
const MAX_REASON: usize = 500;
const HEADER: &str = "{kind,identity,fingerprint,status,at,reason}:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Kind {
    Duplicate,
    Dead,
}

impl Kind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "duplicate" => Some(Self::Duplicate),
            "dead" => Some(Self::Dead),
            _ => None,
        }
    }
    pub fn value(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Dead => "dead",
        }
    }
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Duplicate => t(lang, "Código duplicado"),
            Self::Dead => t(lang, "Código muerto"),
        }
    }
}

/// `Pending` is stored only as the result of a restore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Pending,
    Confirmed,
    FalsePositive,
    Ignored,
}

impl Status {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "confirmed" => Some(Self::Confirmed),
            "false_positive" => Some(Self::FalsePositive),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
    pub fn value(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::FalsePositive => "false_positive",
            Self::Ignored => "ignored",
        }
    }
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Pending => t(lang, "Pendiente de revisar"),
            Self::Confirmed => t(lang, "Confirmado"),
            Self::FalsePositive => t(lang, "Falso positivo"),
            Self::Ignored => t(lang, "Ignorado a propósito"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Record {
    pub kind: Kind,
    pub identity: String,
    pub fingerprint: String,
    pub status: Status,
    pub at: u64,
    pub reason: String,
}

/// What the dashboard shows for one current finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum State {
    Pending,
    Decided {
        status: Status,
        /// `None` for a decision read from the ADR-015 file, which has no time.
        at: Option<u64>,
        reason: String,
    },
    /// A decision exists for this identity but the code changed since.
    NeedsReevaluation {
        status: Status,
        at: u64,
        reason: String,
    },
}

#[derive(Clone, Copy)]
enum LegacyReason {
    FalsePositive,
    Dismissed,
}

impl LegacyReason {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "false-positive" => Some(Self::FalsePositive),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
    fn status(self) -> Status {
        match self {
            Self::FalsePositive => Status::FalsePositive,
            Self::Dismissed => Status::Ignored,
        }
    }
}

type Current = BTreeMap<(Kind, String), Record>;

pub(super) struct Reviews {
    path: PathBuf,
    legacy: BTreeMap<String, LegacyReason>,
    current: Current,
    history: Vec<Record>,
}

impl Reviews {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let path = root.join(".mct-index/reviews.toon");
        let (current, history) = read(&path)?;
        let legacy = read_legacy(&root.join(".mct-index/duplicate-reviews.toon"))?;
        Ok(Self {
            path,
            legacy,
            current,
            history,
        })
    }

    pub fn state(&self, kind: Kind, identity: &str, fingerprint: &str) -> State {
        match self.current.get(&(kind, identity.to_string())) {
            Some(record) if record.status == Status::Pending => State::Pending,
            Some(record) if record.fingerprint == fingerprint => State::Decided {
                status: record.status,
                at: Some(record.at),
                reason: record.reason.clone(),
            },
            Some(record) => State::NeedsReevaluation {
                status: record.status,
                at: record.at,
                reason: record.reason.clone(),
            },
            None => match (kind, self.legacy.get(fingerprint)) {
                (Kind::Duplicate, Some(reason)) => State::Decided {
                    status: reason.status(),
                    at: None,
                    reason: String::new(),
                },
                _ => State::Pending,
            },
        }
    }

    /// Every stored decision (restored findings excluded), newest first.
    pub fn decisions(&self) -> Vec<&Record> {
        let mut records: Vec<_> = self
            .current
            .values()
            .filter(|record| record.status != Status::Pending)
            .collect();
        records.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.identity.cmp(&b.identity)));
        records
    }

    /// History of one finding, newest first.
    pub fn history(&self, kind: Kind, identity: &str) -> Vec<&Record> {
        self.history
            .iter()
            .rev()
            .filter(|record| record.kind == kind && record.identity == identity)
            .collect()
    }

    pub fn set(
        &mut self,
        record_of: (Kind, &str, &str),
        status: Status,
        reason: &str,
        now: u64,
    ) -> anyhow::Result<()> {
        let (kind, identity, fingerprint) = record_of;
        ensure!(
            valid_key(identity) && valid_key(fingerprint),
            "invalid review key"
        );
        safe_path(&self.path)?;
        let parent = self
            .path
            .parent()
            .context("review file needs a parent directory")?;
        let lock_path = parent.join("reviews.lock");
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
        let (mut current, mut history) = read(&self.path)?;
        let key = (kind, identity.to_string());
        if status == Status::Pending
            && !current.contains_key(&key)
            && !self.legacy.contains_key(fingerprint)
        {
            return Ok(());
        }
        let record = Record {
            kind,
            identity: identity.to_string(),
            fingerprint: fingerprint.to_string(),
            status,
            at: now,
            reason: if status == Status::Pending {
                String::new()
            } else {
                clean_reason(reason)
            },
        };
        current.insert(key, record.clone());
        history.push(record);
        if history.len() > MAX_HISTORY {
            let dropped = history.len() - MAX_HISTORY;
            history.drain(..dropped);
            eprintln!("warning: review history is full; dropped the {dropped} oldest rows");
        }
        ensure!(current.len() <= MAX_REVIEWS, "too many reviews");
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        writeln!(temp, "version: 2")?;
        for (name, rows) in [
            ("reviews", current.values().collect::<Vec<_>>()),
            ("history", history.iter().collect()),
        ] {
            writeln!(temp, "{name}[{}]{HEADER}", rows.len())?;
            for row in rows {
                writeln!(
                    temp,
                    "  {},{},{},{},{},{}",
                    row.kind.value(),
                    row.identity,
                    row.fingerprint,
                    row.status.value(),
                    row.at,
                    row.reason
                )?;
            }
        }
        temp.as_file_mut().sync_all()?;
        safe_path(&self.path)?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("cannot publish reviews")?;
        self.current = current;
        self.history = history;
        Ok(())
    }
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

fn clean_reason(raw: &str) -> String {
    let text: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut text = text.trim().to_string();
    if text.len() > MAX_REASON {
        let mut end = MAX_REASON;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text = text.trim_end().to_string();
    }
    text
}

pub(super) fn valid_key(key: &str) -> bool {
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

fn read_text(path: &Path) -> anyhow::Result<Option<String>> {
    safe_path(path)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut content = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut content)?;
    ensure!(
        content.len() as u64 <= MAX_BYTES,
        "review state is too large"
    );
    Ok(Some(content))
}

fn read(path: &Path) -> anyhow::Result<(Current, Vec<Record>)> {
    match read_text(path)? {
        Some(content) => parse(&content),
        None => Ok((Current::new(), Vec::new())),
    }
}

fn table<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    name: &str,
    limit: usize,
) -> anyhow::Result<Vec<Record>> {
    let header = lines
        .next()
        .with_context(|| format!("missing {name} table"))?;
    let count: usize = header
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('['))
        .and_then(|rest| rest.strip_suffix(&format!("]{HEADER}")))
        .with_context(|| format!("invalid {name} table"))?
        .parse()?;
    ensure!(count <= limit, "too many {name} rows");
    let mut rows = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let line = lines
            .next()
            .and_then(|line| line.strip_prefix("  "))
            .with_context(|| format!("missing {name} row"))?;
        let mut fields = line.splitn(6, ',');
        let mut next = || fields.next().context("short review row");
        let kind = Kind::parse(next()?).context("invalid review kind")?;
        let identity = next()?;
        let fingerprint = next()?;
        let status = Status::parse(next()?).context("invalid review status")?;
        let at: u64 = next()?.parse().context("invalid review time")?;
        let reason = next()?;
        ensure!(
            valid_key(identity) && valid_key(fingerprint),
            "invalid review key"
        );
        ensure!(reason == clean_reason(reason), "invalid review reason");
        rows.push(Record {
            kind,
            identity: identity.to_string(),
            fingerprint: fingerprint.to_string(),
            status,
            at,
            reason: reason.to_string(),
        });
    }
    Ok(rows)
}

fn parse(content: &str) -> anyhow::Result<(Current, Vec<Record>)> {
    let mut lines = content.lines();
    ensure!(
        lines.next() == Some("version: 2"),
        "unsupported review format"
    );
    let reviews = table(&mut lines, "reviews", MAX_REVIEWS)?;
    let history = table(&mut lines, "history", MAX_HISTORY)?;
    ensure!(lines.next().is_none(), "unexpected review data");
    let mut current = Current::new();
    for record in reviews {
        let key = (record.kind, record.identity.clone());
        ensure!(
            current.insert(key, record).is_none(),
            "duplicate review key"
        );
    }
    Ok((current, history))
}

fn read_legacy(path: &Path) -> anyhow::Result<BTreeMap<String, LegacyReason>> {
    let Some(content) = read_text(path)? else {
        return Ok(BTreeMap::new());
    };
    let mut lines = content.lines();
    ensure!(
        lines.next() == Some("version: 1"),
        "unsupported duplicate review format"
    );
    let header = lines.next().context("missing duplicate review table")?;
    let count: usize = header
        .strip_prefix("reviews[")
        .and_then(|value| value.strip_suffix("]{key,reason}:"))
        .context("invalid duplicate review table")?
        .parse()?;
    ensure!(count <= MAX_REVIEWS, "too many duplicate reviews");
    let mut entries = BTreeMap::new();
    for line in lines {
        let (key, reason) = line
            .strip_prefix("  ")
            .and_then(|line| line.split_once(','))
            .context("invalid duplicate review row")?;
        ensure!(valid_key(key), "invalid duplicate review key");
        let reason = LegacyReason::parse(reason).context("invalid duplicate review reason")?;
        ensure!(
            entries.insert(key.to_string(), reason).is_none(),
            "duplicate review key"
        );
    }
    ensure!(entries.len() == count, "duplicate review count mismatch");
    Ok(entries)
}

fn digest(fields: &[&str]) -> String {
    let mut bytes = Vec::new();
    for value in fields {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    hmac_sha256::Hash::hash(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Fingerprints of duplicate groups: the ADR-015 key, byte for byte, so decisions
/// saved in `duplicate-reviews.toon` keep matching.
pub(super) fn group_keys(
    groups: &[DuplicateGroup],
    navigation: &NavigationSnapshot,
    normalization: Normalization,
) -> anyhow::Result<Vec<String>> {
    let hashes: BTreeMap<_, _> = navigation
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect();
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
            let mut fields: Vec<String> = vec![
                "duplicate-review-v1".into(),
                normalization.as_str().into(),
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
            let refs: Vec<&str> = fields.iter().map(String::as_str).collect();
            Ok(digest(&refs))
        })
        .collect()
}

/// Identity of a duplicate group: language and members' path, name and kind.
/// No lines and no content, so it survives edits and reindexing.
pub(super) fn group_identities(groups: &[DuplicateGroup]) -> Vec<String> {
    groups
        .iter()
        .map(|group| {
            let mut members: Vec<_> = group
                .members
                .iter()
                .map(|member| {
                    (
                        member.path.as_str(),
                        member.symbol_name.as_str(),
                        member.symbol_kind.as_str(),
                    )
                })
                .collect();
            members.sort();
            let mut fields = vec!["duplicate-id-v1", group.language.as_str()];
            for (path, name, kind) in members {
                fields.extend([path, name, kind]);
            }
            digest(&fields)
        })
        .collect()
}

/// Identity and fingerprint of each dead-code candidate, in input order.
/// Same-named symbols with the same parent in one file are told apart by their
/// order in the file.
pub(super) fn dead_keys(
    entries: &[SymbolListEntry],
    navigation: &NavigationSnapshot,
) -> Vec<(String, String)> {
    let hashes: BTreeMap<_, _> = navigation
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect();
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|id| (&entries[*id].relative_path, entries[*id].line));
    let mut seen: BTreeMap<(&str, &str, &str, &str), usize> = BTreeMap::new();
    let mut keys = vec![(String::new(), String::new()); entries.len()];
    for id in order {
        let entry = &entries[id];
        let parent = entry.parent.as_deref().unwrap_or("");
        let ordinal = seen
            .entry((&entry.relative_path, &entry.kind, parent, &entry.name))
            .or_default();
        let ordinal_text = ordinal.to_string();
        *ordinal += 1;
        let identity = digest(&[
            "dead-id-v1",
            &entry.language,
            &entry.relative_path,
            &entry.kind,
            parent,
            &entry.name,
            &ordinal_text,
        ]);
        let hash = hashes
            .get(entry.relative_path.as_str())
            .copied()
            .unwrap_or("");
        let end = entry.end_line.unwrap_or(entry.line).to_string();
        let fingerprint = digest(&["dead-fp-v1", hash, &entry.line.to_string(), &end]);
        keys[id] = (identity, fingerprint);
    }
    keys
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

    fn id(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn save(reviews: &mut Reviews, identity: &str, fingerprint: &str, status: Status, at: u64) {
        reviews
            .set((Kind::Duplicate, identity, fingerprint), status, "", at)
            .unwrap();
    }

    #[test]
    fn decisions_persist_restore_and_merge_between_writers() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let (a, b, fp) = (id('a'), id('b'), id('f'));
        let mut first = Reviews::load(&path).unwrap();
        let mut second = Reviews::load(&path).unwrap();
        assert!(!path.join(".mct-index/reviews.toon").exists());
        save(&mut first, &a, &fp, Status::FalsePositive, 10);
        save(&mut second, &b, &fp, Status::Ignored, 11);
        let current = Reviews::load(&path).unwrap();
        assert!(matches!(
            current.state(Kind::Duplicate, &a, &fp),
            State::Decided {
                status: Status::FalsePositive,
                at: Some(10),
                ..
            }
        ));
        assert!(matches!(
            current.state(Kind::Duplicate, &b, &fp),
            State::Decided {
                status: Status::Ignored,
                ..
            }
        ));
        save(&mut first, &a, &fp, Status::Pending, 12);
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.state(Kind::Duplicate, &a, &fp), State::Pending);
        assert_eq!(current.decisions().len(), 1);
        let history: Vec<_> = current
            .history(Kind::Duplicate, &a)
            .iter()
            .map(|r| (r.status, r.at))
            .collect();
        assert_eq!(
            history,
            [(Status::Pending, 12), (Status::FalsePositive, 10)],
            "restore keeps the earlier decision in the history"
        );
        // A restore of a finding that never had a decision writes nothing.
        let mut third = Reviews::load(&path).unwrap();
        save(&mut third, &id('c'), &fp, Status::Pending, 13);
        assert_eq!(Reviews::load(&path).unwrap().history.len(), 3);
    }

    #[test]
    fn a_changed_fingerprint_needs_reevaluation_and_a_new_decision_replaces_it() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let (identity, old, new) = (id('a'), id('1'), id('2'));
        let mut reviews = Reviews::load(&path).unwrap();
        reviews
            .set(
                (Kind::Dead, &identity, &old),
                Status::Confirmed,
                "seguro",
                5,
            )
            .unwrap();
        assert!(matches!(
            reviews.state(Kind::Dead, &identity, &new),
            State::NeedsReevaluation {
                status: Status::Confirmed,
                at: 5,
                ..
            }
        ));
        // Another kind with the same identity is a different finding.
        assert_eq!(
            reviews.state(Kind::Duplicate, &identity, &old),
            State::Pending
        );
        reviews
            .set(
                (Kind::Dead, &identity, &new),
                Status::Ignored,
                "api pública",
                6,
            )
            .unwrap();
        let reloaded = Reviews::load(&path).unwrap();
        assert!(matches!(
            reloaded.state(Kind::Dead, &identity, &new),
            State::Decided { status: Status::Ignored, ref reason, .. } if reason == "api pública"
        ));
        assert_eq!(reloaded.history(Kind::Dead, &identity).len(), 2);
    }

    #[test]
    fn the_adr_015_file_is_read_as_decisions_and_never_modified() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let legacy = path.join(".mct-index/duplicate-reviews.toon");
        let (a, b, fp) = (id('a'), id('b'), id('f'));
        let content = format!(
            "version: 1\nreviews[2]{{key,reason}}:\n  {a},false-positive\n  {b},dismissed\n"
        );
        fs::write(&legacy, &content).unwrap();
        let mut reviews = Reviews::load(&path).unwrap();
        assert!(matches!(
            reviews.state(Kind::Duplicate, &id('d'), &a),
            State::Decided {
                status: Status::FalsePositive,
                at: None,
                ..
            }
        ));
        assert!(matches!(
            reviews.state(Kind::Duplicate, &id('d'), &b),
            State::Decided {
                status: Status::Ignored,
                ..
            }
        ));
        assert_eq!(reviews.state(Kind::Dead, &id('d'), &a), State::Pending);
        // Restoring a legacy decision hides it without touching the old file.
        reviews
            .set((Kind::Duplicate, &id('d'), &a), Status::Pending, "", 9)
            .unwrap();
        assert_eq!(reviews.state(Kind::Duplicate, &id('d'), &a), State::Pending);
        assert_eq!(
            reviews.state(Kind::Duplicate, &id('y'), &fp),
            State::Pending
        );
        assert_eq!(fs::read_to_string(&legacy).unwrap(), content);
        assert_eq!(
            Reviews::load(&path)
                .unwrap()
                .state(Kind::Duplicate, &id('d'), &a),
            State::Pending
        );
    }

    #[test]
    fn locking_and_corruption_preserve_saved_and_visible_decisions() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let (identity, fp) = (id('a'), id('f'));
        let mut reviews = Reviews::load(&path).unwrap();
        save(&mut reviews, &identity, &fp, Status::FalsePositive, 1);
        let saved = fs::read(&reviews.path).unwrap();
        let lock = path.join(".mct-index/reviews.lock");
        fs::write(&lock, "another writer").unwrap();
        assert!(reviews
            .set((Kind::Duplicate, &identity, &fp), Status::Pending, "", 2)
            .is_err());
        assert_eq!(fs::read(&reviews.path).unwrap(), saved);
        assert!(matches!(
            reviews.state(Kind::Duplicate, &identity, &fp),
            State::Decided { .. }
        ));
        assert_eq!(fs::read_to_string(&lock).unwrap(), "another writer");
        fs::remove_file(lock).unwrap();
        fs::write(&reviews.path, "user data / corrupt state").unwrap();
        assert!(reviews
            .set((Kind::Duplicate, &identity, &fp), Status::Pending, "", 3)
            .is_err());
        assert_eq!(
            fs::read_to_string(&reviews.path).unwrap(),
            "user data / corrupt state"
        );
        assert!(Reviews::load(&path).is_err(), "corrupt state fails closed");
        assert!(!path.join(".mct-index/reviews.lock").exists());
    }

    #[test]
    fn invalid_versions_rows_counts_and_keys_fail_closed() {
        let (a, f) = (id('a'), id('f'));
        let row = |status: &str, reason: &str| format!("  duplicate,{a},{f},{status},7,{reason}\n");
        let file = |reviews: &str, count: usize, history: &str| {
            format!("version: 2\nreviews[{count}]{HEADER}\n{reviews}history[0]{HEADER}\n{history}")
        };
        assert!(parse(&file("", 0, "")).is_ok());
        assert!(parse(&file(&row("ignored", "a, b, c"), 1, "")).is_ok());
        for content in [
            file("", 0, "").replace("version: 2", "version: 3"),
            file("", 1, ""),
            file(&row("unknown", ""), 1, ""),
            file(&row("ignored", ""), 1, "") + "extra\n",
            file(&(row("ignored", "") + &row("confirmed", "")), 2, ""),
            file("  duplicate,../../x,../../y,ignored,7,\n", 1, ""),
            file("  other,a,b,ignored,7,\n", 1, ""),
            file(&row("ignored", "tab\there"), 1, ""),
            format!("version: 2\nreviews[100001]{HEADER}\nhistory[0]{HEADER}\n"),
            format!("version: 2\nreviews[0]{HEADER}\nhistory[50001]{HEADER}\n"),
        ] {
            assert!(parse(&content).is_err(), "{content}");
        }
    }

    #[test]
    fn reasons_are_single_line_bounded_and_utf8_safe() {
        assert_eq!(clean_reason("  a\nb\tc  "), "a b c");
        let long = "ñ".repeat(400);
        let cleaned = clean_reason(&long);
        assert!(cleaned.len() <= MAX_REASON && cleaned.chars().all(|ch| ch == 'ñ'));
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let mut reviews = Reviews::load(&path).unwrap();
        reviews
            .set(
                (Kind::Dead, &id('a'), &id('f')),
                Status::Ignored,
                "x,y\nz",
                1,
            )
            .unwrap();
        let reloaded = Reviews::load(&path).unwrap();
        assert_eq!(reloaded.decisions()[0].reason, "x,y z");
    }

    #[test]
    fn dead_identities_ignore_lines_and_content_but_fingerprints_do_not() {
        let entry = |line: u32, name: &str| SymbolListEntry {
            name: name.into(),
            kind: "function".into(),
            language: "rust".into(),
            relative_path: "src/a.rs".into(),
            line,
            end_line: Some(line + 2),
            parent: None,
            level: None,
        };
        let navigation = |hash: &str| NavigationSnapshot {
            files: vec![mct_index::IndexedFile {
                relative_path: "src/a.rs".into(),
                language: "rust".into(),
                content_hash: hash.into(),
            }],
            symbols: Vec::new(),
            imports: Vec::new(),
        };
        let before = dead_keys(&[entry(3, "old"), entry(9, "old")], &navigation("h1"));
        let after = dead_keys(&[entry(30, "old"), entry(90, "old")], &navigation("h2"));
        assert_ne!(before[0].0, before[1].0, "ordinals tell twins apart");
        assert_eq!(before[0].0, after[0].0);
        assert_eq!(before[1].0, after[1].0);
        assert_ne!(before[0].1, after[0].1);
        assert!(valid_key(&before[0].0) && valid_key(&before[0].1));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_hard_links_cannot_redirect_review_state() {
        use std::os::unix::fs::symlink;
        let root = project();
        let root_path = root.path().canonicalize().unwrap();
        let content = format!("version: 2\nreviews[0]{HEADER}\nhistory[0]{HEADER}\n");
        for name in ["reviews.toon", "duplicate-reviews.toon"] {
            let file = root_path.join(".mct-index").join(name);
            let external = root_path.join("outside.toon");
            fs::write(&external, &content).unwrap();
            symlink(&external, &file).unwrap();
            assert!(Reviews::load(&root_path).is_err(), "{name} symlink");
            fs::remove_file(&file).unwrap();
            fs::hard_link(&external, &file).unwrap();
            assert!(Reviews::load(&root_path).is_err(), "{name} hard link");
            assert_eq!(fs::read_to_string(&external).unwrap(), content);
            fs::remove_file(&file).unwrap();
            fs::remove_file(&external).unwrap();
        }
        let link_root = root_path.join("alias");
        symlink(root_path.join(".mct-index"), &link_root).unwrap();
        assert!(read(&link_root.join("reviews.toon")).is_err());
    }

    #[test]
    fn oversized_state_is_rejected_before_parsing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap().join("state.toon");
        fs::write(&path, vec![b'a'; MAX_BYTES as usize + 1]).unwrap();
        assert!(read(&path).unwrap_err().to_string().contains("too large"));
    }
}
