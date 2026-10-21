//! User review preferences, separate from the derived analysis and SQLite index.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use mct_index::{DuplicateGroup, NavigationSnapshot, Normalization};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REVIEWS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reason {
    FalsePositive,
    Dismissed,
}

impl Reason {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "false-positive" => Some(Self::FalsePositive),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
    pub fn value(self) -> &'static str {
        match self {
            Self::FalsePositive => "false-positive",
            Self::Dismissed => "dismissed",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::FalsePositive => "False positive",
            Self::Dismissed => "Dismissed",
        }
    }
}

pub(super) struct Reviews {
    path: PathBuf,
    entries: BTreeMap<String, Reason>,
}

impl Reviews {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let path = root.join(".mct-index/duplicate-reviews.toon");
        let entries = read(&path)?;
        Ok(Self { path, entries })
    }
    pub fn reason(&self, key: &str) -> Option<Reason> {
        self.entries.get(key).copied()
    }

    pub fn update(&mut self, key: &str, reason: Option<Reason>) -> anyhow::Result<()> {
        ensure!(valid_key(key), "invalid review key");
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
            .context("cannot lock duplicate reviews; another writer may be active")?;
        let _lock = Lock {
            path: lock_path,
            file: Some(lock_file),
        };
        // Reload under the writer lock: different web processes must not lose
        // each other's decisions. Publish before changing this process's view.
        let mut next = read(&self.path)?;
        if let Some(reason) = reason {
            next.insert(key.to_string(), reason);
        } else {
            next.remove(key);
        }
        ensure!(next.len() <= MAX_REVIEWS, "too many duplicate reviews");
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        writeln!(temp, "version: 1\nreviews[{}]{{key,reason}}:", next.len())?;
        for (key, reason) in &next {
            writeln!(temp, "  {key},{}", reason.value())?;
        }
        temp.as_file_mut().sync_all()?;
        safe_path(&self.path)?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("cannot publish duplicate reviews")?;
        self.entries = next;
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

fn read(path: &Path) -> anyhow::Result<BTreeMap<String, Reason>> {
    safe_path(path)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    let mut content = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut content)?;
    ensure!(
        content.len() as u64 <= MAX_BYTES,
        "duplicate review state is too large"
    );
    parse(&content)
}

fn parse(content: &str) -> anyhow::Result<BTreeMap<String, Reason>> {
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
        let reason = Reason::parse(reason).context("invalid duplicate review reason")?;
        ensure!(
            entries.insert(key.to_string(), reason).is_none(),
            "duplicate review key"
        );
    }
    ensure!(entries.len() == count, "duplicate review count mismatch");
    Ok(entries)
}

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
            let mut bytes = Vec::new();
            let mut field = |value: &str| {
                bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
                bytes.extend_from_slice(value.as_bytes());
            };
            field("duplicate-review-v1");
            field(normalization.as_str());
            field(&group.language);
            for (path, name, kind, start, end, hash) in members {
                for value in [
                    path.as_str(),
                    name.as_str(),
                    kind.as_str(),
                    &start.to_string(),
                    &end.to_string(),
                    hash,
                ] {
                    field(value);
                }
            }
            Ok(hmac_sha256::Hash::hash(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect())
        })
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
    fn decisions_persist_restore_and_merge_between_writers() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let other = "b".repeat(64);
        let mut first = Reviews::load(&path).unwrap();
        let mut second = Reviews::load(&path).unwrap();
        assert!(!path.join(".mct-index/duplicate-reviews.toon").exists());
        first.update(&key, Some(Reason::FalsePositive)).unwrap();
        second.update(&other, Some(Reason::Dismissed)).unwrap();
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.reason(&key), Some(Reason::FalsePositive));
        assert_eq!(current.reason(&other), Some(Reason::Dismissed));
        first.update(&key, None).unwrap();
        let current = Reviews::load(&path).unwrap();
        assert_eq!(current.reason(&key), None);
        assert_eq!(current.reason(&other), Some(Reason::Dismissed));
    }

    #[test]
    fn locking_and_corruption_preserve_saved_and_visible_decisions() {
        let root = project();
        let path = root.path().canonicalize().unwrap();
        let key = "a".repeat(64);
        let mut reviews = Reviews::load(&path).unwrap();
        reviews.update(&key, Some(Reason::FalsePositive)).unwrap();
        let saved = fs::read(&reviews.path).unwrap();
        let lock = path.join(".mct-index/duplicate-reviews.lock");
        fs::write(&lock, "another writer").unwrap();
        assert!(reviews.update(&key, None).is_err());
        assert_eq!(fs::read(&reviews.path).unwrap(), saved);
        assert_eq!(reviews.reason(&key), Some(Reason::FalsePositive));
        assert_eq!(fs::read_to_string(&lock).unwrap(), "another writer");
        fs::remove_file(lock).unwrap();
        fs::write(&reviews.path, "user data / corrupt state").unwrap();
        assert!(reviews.update(&key, None).is_err());
        assert_eq!(reviews.reason(&key), Some(Reason::FalsePositive));
        assert_eq!(
            fs::read_to_string(&reviews.path).unwrap(),
            "user data / corrupt state"
        );
        assert!(!path.join(".mct-index/duplicate-reviews.lock").exists());
    }

    #[test]
    fn invalid_versions_rows_counts_and_keys_fail_closed() {
        let key = "a".repeat(64);
        for content in [
            "version: 2\nreviews[0]{key,reason}:\n".to_string(),
            "version: 1\nreviews[1]{key,reason}:\n".to_string(),
            "version: 1\nreviews[1]{key,reason}:\n  ../../file,false-positive\n".to_string(),
            format!("version: 1\nreviews[1]{{key,reason}}:\n  {key},unknown\n"),
            format!("version: 1\nreviews[2]{{key,reason}}:\n  {key},dismissed\n  {key},false-positive\n"),
            "version: 1\nreviews[100001]{key,reason}:\n".to_string(),
        ] { assert!(parse(&content).is_err(), "{content}"); }
        assert!(parse("version: 1\nreviews[0]{key,reason}:\n")
            .unwrap()
            .is_empty());
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
            .unwrap_err()
            .to_string()
            .contains("too large"));
    }
}
