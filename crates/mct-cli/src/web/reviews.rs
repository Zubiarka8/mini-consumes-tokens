//! User review decisions for duplicate and dead-code findings (ADR-018),
//! separate from the derived analysis and the SQLite index.
//!
//! Decisions are stored in `.mct-index/dashboard.sqlite3` (ADR-019, `store.rs`).
//! This module keeps the vocabulary (kinds, statuses, states), the stable
//! identities and fingerprints, and the readers of the two legacy files, which
//! are never modified:
//! - `reviews.toon` (version 2): imported once into the store.
//! - `duplicate-reviews.toon` (version 1, ADR-015): its keys are fingerprints,
//!   so they match a duplicate group exactly.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{ensure, Context};

use super::i18n::{t, Lang};
#[cfg(test)]
use mct_index::{NavigationSnapshot, SymbolListEntry};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_REVIEWS: usize = 100_000;
pub(super) const MAX_HISTORY: usize = 50_000;
pub(super) const MAX_REASON: usize = 500;
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
pub(super) enum LegacyReason {
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
    pub fn status(self) -> Status {
        match self {
            Self::FalsePositive => Status::FalsePositive,
            Self::Dismissed => Status::Ignored,
        }
    }
}

pub(super) type Current = BTreeMap<(Kind, String), Record>;

pub(super) fn clean_reason(raw: &str) -> String {
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

/// Form and API action: a status, or `restore` for pending.
pub(super) fn action_status(action: &str) -> Option<Status> {
    match action {
        "restore" => Some(Status::Pending),
        other => match Status::parse(other) {
            Some(Status::Pending) | None => None,
            some => some,
        },
    }
}

pub(super) fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn safe_path(path: &Path) -> anyhow::Result<()> {
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

pub(super) fn read(path: &Path) -> anyhow::Result<(Current, Vec<Record>)> {
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

pub(super) fn read_legacy(path: &Path) -> anyhow::Result<BTreeMap<String, LegacyReason>> {
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

pub(super) use mct_index::review_keys::{
    group_identities, legacy_dead_keys, legacy_group_keys, Fingerprints,
};

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
        assert_eq!(clean_reason("x,y\nz"), "x,y z");
    }

    #[test]
    fn dead_fingerprints_follow_symbol_text_and_fail_closed() {
        let root = project();
        let path = root.path().join("a.rs");
        let entry = |line| SymbolListEntry {
            name: "unused".into(),
            kind: "function".into(),
            language: "rust".into(),
            relative_path: "a.rs".into(),
            line,
            end_line: Some(line + 2),
            parent: None,
            level: None,
        };
        let keys = |text: &str, line| {
            fs::write(&path, text).unwrap();
            let navigation = NavigationSnapshot {
                files: vec![mct_index::IndexedFile {
                    relative_path: "a.rs".into(),
                    language: "rust".into(),
                    content_hash: git2::Oid::hash_object(git2::ObjectType::Blob, text.as_bytes())
                        .unwrap()
                        .to_string(),
                }],
                symbols: Vec::new(),
                imports: Vec::new(),
            };
            Fingerprints::new(root.path(), &navigation)
                .dead_keys(&[entry(line)], &navigation)
                .remove(0)
        };
        let symbol = "fn unused() {\n    return;\n}\n";
        let before = keys(symbol, 1);
        assert_eq!(before, keys(&format!("{symbol}// unrelated\n"), 1));
        assert_eq!(before, keys(&format!("// header\n{symbol}"), 2));
        let changed = keys(&symbol.replace("return;", "let x = 1;"), 1);
        assert_eq!(before.0, changed.0);
        assert_ne!(before.1, changed.1);
        assert!(valid_key(&before.0) && valid_key(&before.1));
        assert!(
            keys(symbol, 20).1.is_empty(),
            "incomplete ranges never match"
        );
        let mut unknown_end = entry(1);
        unknown_end.end_line = None;
        let navigation = NavigationSnapshot {
            files: Vec::new(),
            symbols: Vec::new(),
            imports: Vec::new(),
        };
        assert!(
            Fingerprints::new(root.path(), &navigation).dead_keys(&[unknown_end], &navigation)[0]
                .1
                .is_empty()
        );
    }

    #[test]
    fn dead_identities_keep_same_name_ordinals_across_moves() {
        let entry = |line| SymbolListEntry {
            name: "twin".into(),
            kind: "function".into(),
            language: "rust".into(),
            relative_path: "a.rs".into(),
            line,
            end_line: Some(line),
            parent: None,
            level: None,
        };
        let navigation = NavigationSnapshot {
            files: Vec::new(),
            symbols: Vec::new(),
            imports: Vec::new(),
        };
        let before = legacy_dead_keys(&[entry(3), entry(9)], &navigation);
        let after = legacy_dead_keys(&[entry(30), entry(90)], &navigation);
        assert_ne!(before[0].0, before[1].0);
        assert_eq!(before[0].0, after[0].0);
        assert_eq!(before[1].0, after[1].0);
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
            let target = |file: &Path| {
                if name == "reviews.toon" {
                    read(file).map(drop)
                } else {
                    read_legacy(file).map(drop)
                }
            };
            assert!(target(&file).is_err(), "{name} symlink");
            fs::remove_file(&file).unwrap();
            fs::hard_link(&external, &file).unwrap();
            assert!(target(&file).is_err(), "{name} hard link");
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
