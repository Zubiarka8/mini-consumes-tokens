//! Stable identities and content fingerprints shared by dashboard and AI outputs.
//! These functions do not modify either database.
use crate::{DuplicateGroup, NavigationSnapshot, Normalization, SymbolListEntry};
use anyhow::Context;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
    }
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES).then_some(path)
}

/// Complete UTF-8 source matching the indexed blob. Never hash a partial read
/// or apply indexed line ranges to a different on-disk revision.
pub fn indexed_text(root: &Path, relative: &str, hash: &str) -> Option<String> {
    let path = resolve(root, relative)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES
        || git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)
            .ok()?
            .to_string()
            != hash
    {
        return None;
    }
    String::from_utf8(bytes).ok()
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
pub fn legacy_group_keys(
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
pub fn group_identities(groups: &[DuplicateGroup]) -> Vec<String> {
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
pub fn legacy_dead_keys(
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
        let identity = dead_identity_at(entry, *ordinal);
        *ordinal += 1;
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

/// One bounded read per member file, shared between dead and duplicate findings.
/// An unavailable fingerprint is empty: it can never match or be saved.
pub struct Fingerprints<'a> {
    root: &'a Path,
    hashes: BTreeMap<&'a str, &'a str>,
    files: BTreeMap<String, Option<Vec<String>>>,
}

impl<'a> Fingerprints<'a> {
    pub fn new(root: &'a Path, navigation: &'a NavigationSnapshot) -> Self {
        Self {
            root,
            hashes: navigation
                .files
                .iter()
                .map(|f| (f.relative_path.as_str(), f.content_hash.as_str()))
                .collect(),
            files: BTreeMap::new(),
        }
    }

    /// Hash the complete indexed line range, caching each verified file once.
    /// Returns None for unavailable, stale, oversized or invalid source/ranges.
    pub fn text_hash(&mut self, path: &str, start: u32, end: u32) -> Option<String> {
        if start == 0 || end < start {
            return None;
        }
        let lines = self
            .files
            .entry(path.to_string())
            .or_insert_with(|| {
                let hash = self.hashes.get(path)?;
                let text = indexed_text(self.root, path, hash)?;
                Some(text.split_inclusive('\n').map(str::to_string).collect())
            })
            .as_ref()?;
        let lines = lines.get((start - 1) as usize..end as usize)?;
        Some(digest(&[&lines.concat()]))
    }

    pub fn dead_keys(
        &mut self,
        entries: &[SymbolListEntry],
        navigation: &NavigationSnapshot,
    ) -> Vec<(String, String)> {
        legacy_dead_keys(entries, navigation)
            .into_iter()
            .zip(entries)
            .map(|((identity, _), entry)| {
                let fingerprint = entry
                    .end_line
                    .and_then(|end| self.text_hash(&entry.relative_path, entry.line, end))
                    .map(|hash| digest(&["dead-fp-v2", &hash]))
                    .unwrap_or_default();
                (identity, fingerprint)
            })
            .collect()
    }

    pub fn group_keys(
        &mut self,
        groups: &[DuplicateGroup],
        normalization: Normalization,
    ) -> Vec<String> {
        groups
            .iter()
            .map(|group| {
                let mut members = Vec::new();
                for member in &group.members {
                    let Some(hash) =
                        self.text_hash(&member.path, member.start_line, member.end_line)
                    else {
                        return String::new();
                    };
                    members.push((&member.path, &member.symbol_name, &member.symbol_kind, hash));
                }
                members.sort();
                let mut fields = vec!["duplicate-fp-v2", normalization.as_str(), &group.language];
                for (path, name, kind, hash) in &members {
                    fields.extend([path.as_str(), name.as_str(), kind.as_str(), hash.as_str()]);
                }
                digest(&fields)
            })
            .collect()
    }
}

/// A same-name symbol family, independent of its ordinal and body text.
pub fn dead_base_identity(entry: &SymbolListEntry) -> String {
    digest(&[
        "dead-base-v1",
        &entry.language,
        &entry.relative_path,
        &entry.kind,
        entry.parent.as_deref().unwrap_or(""),
        &entry.name,
    ])
}

/// Exact legacy namespace identity for one ordinal in a same-name family.
pub fn dead_identity_at(entry: &SymbolListEntry, ordinal: usize) -> String {
    digest(&[
        "dead-id-v1",
        &entry.language,
        &entry.relative_path,
        &entry.kind,
        entry.parent.as_deref().unwrap_or(""),
        &entry.name,
        &ordinal.to_string(),
    ])
}
