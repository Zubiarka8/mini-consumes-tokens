//! Copy identity and conservative label association, independent of group verdicts.
use std::collections::{BTreeMap, BTreeSet};

use mct_index::{review_keys::Fingerprints, DuplicateGroup, NavigationSnapshot};

use super::store::FileReview;

#[derive(Clone)]
pub(super) struct Member {
    pub key: String,
    pub base: String,
    pub hash: Option<String>,
    pub uncertain: bool,
}

fn digest(fields: &[&str]) -> String {
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    hmac_sha256::Hash::hash(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn ordinal_key(base: &str, ordinal: usize) -> String {
    digest(&["copy-id-v1", base, &ordinal.to_string()])
}

/// Indexed ranges locate parents, but never enter a persistent copy key.
pub(super) struct ParentNames(BTreeMap<String, Option<String>>);

impl ParentNames {
    pub fn new(navigation: &NavigationSnapshot) -> Self {
        let mut names = BTreeMap::new();
        for symbol in &navigation.symbols {
            let range = digest(&[
                &symbol.relative_path,
                &symbol.name,
                &symbol.kind,
                &symbol.line.to_string(),
                &symbol.end_line.unwrap_or(symbol.line).to_string(),
            ]);
            names
                .entry(range)
                .and_modify(|value| *value = None)
                .or_insert_with(|| Some(symbol.parent.clone().unwrap_or_default()));
        }
        Self(names)
    }
}

pub(super) fn indexed(
    group: &DuplicateGroup,
    parents: &ParentNames,
    fingerprints: &mut Fingerprints<'_>,
) -> Vec<Member> {
    let mut seen = BTreeMap::new();
    group
        .members
        .iter()
        .map(|member| {
            let range = digest(&[
                &member.path,
                &member.symbol_name,
                &member.symbol_kind,
                &member.start_line.to_string(),
                &member.end_line.to_string(),
            ]);
            let parent = parents
                .0
                .get(&range)
                .and_then(Option::as_deref)
                .unwrap_or("");
            let base = digest(&[
                "copy-base-v1",
                &member.path,
                &member.symbol_name,
                &member.symbol_kind,
                parent,
            ]);
            let ordinal = seen.entry(base.clone()).or_insert(0usize);
            let key = ordinal_key(&base, *ordinal);
            *ordinal += 1;
            Member {
                key,
                base,
                hash: fingerprints.text_hash(&member.path, member.start_line, member.end_line),
                uncertain: false,
            }
        })
        .collect()
}

/// Unique content matches retain keys across sibling moves. Ambiguous unmatched
/// rows remain retained; they must never silently label a different body.
pub(super) fn resolve(current: &[Member], saved: &[FileReview]) -> Vec<Member> {
    let mut result = current.to_vec();
    let mut families: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, member) in current.iter().enumerate() {
        families.entry(&member.base).or_default().push(index);
    }
    let reserved: BTreeSet<&str> = saved.iter().map(|row| row.member_key.as_str()).collect();
    let mut saved_families: BTreeMap<&str, Vec<&FileReview>> = BTreeMap::new();
    for row in saved.iter().filter(|row| !row.member_key.is_empty()) {
        saved_families
            .entry(&row.member_base)
            .or_default()
            .push(row);
    }
    let mut assigned = BTreeSet::new();
    for (base, indices) in families {
        let rows = saved_families.get(base).map(Vec::as_slice).unwrap_or(&[]);
        let mut by_hash: BTreeMap<&str, Vec<&FileReview>> = BTreeMap::new();
        let by_key: BTreeMap<_, _> = rows
            .iter()
            .map(|row| (row.member_key.as_str(), *row))
            .collect();
        for row in rows {
            if let Some(hash) = row.code_hash.as_deref() {
                by_hash.entry(hash).or_default().push(row);
            }
        }
        let mut counts = BTreeMap::new();
        for &index in &indices {
            if let Some(hash) = current[index].hash.as_deref() {
                *counts.entry(hash).or_insert(0usize) += 1;
            }
        }
        let mut matched = BTreeSet::new();
        for &index in &indices {
            let hash = current[index].hash.as_deref();
            let candidates = hash
                .and_then(|hash| by_hash.get(hash))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let twins = hash.and_then(|hash| counts.get(hash)).copied().unwrap_or(0);
            let row = if indices.len() == 1 && rows.len() == 1 {
                Some(rows[0])
            } else if twins == 1 && candidates.len() == 1 {
                Some(candidates[0])
            } else {
                None
            };
            if let Some(row) = row {
                result[index].key.clone_from(&row.member_key);
                assigned.insert(row.member_key.clone());
                matched.insert(index);
            }
        }
        let mut ordinal = 0;
        for &index in &indices {
            if matched.contains(&index) {
                continue;
            }
            // Indistinguishable same-name copies retain their visible slots, but
            // never claim that code association was verified through a reorder.
            let preferred = by_key
                .get(current[index].key.as_str())
                .copied()
                .filter(|row| row.code_hash == current[index].hash);
            if let Some(row) = preferred.filter(|row| !assigned.contains(&row.member_key)) {
                result[index].key.clone_from(&row.member_key);
                result[index].uncertain = true;
                assigned.insert(row.member_key.clone());
                continue;
            }
            if !reserved.contains(current[index].key.as_str())
                && !assigned.contains(&current[index].key)
            {
                assigned.insert(current[index].key.clone());
                continue;
            }
            // Each occupied key excludes at most one ordinal: bounded recovery.
            let limit = reserved.len().saturating_add(current.len());
            while ordinal <= limit {
                let key = ordinal_key(base, ordinal);
                ordinal += 1;
                if !reserved.contains(key.as_str()) && assigned.insert(key.clone()) {
                    result[index].key = key;
                    result[index].uncertain = true;
                    break;
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(ordinal: usize, hash: &str) -> Member {
        Member {
            key: ordinal_key("family", ordinal),
            base: "family".into(),
            hash: Some(hash.into()),
            uncertain: false,
        }
    }

    fn saved(member: &Member) -> FileReview {
        FileReview {
            path: "a.rs".into(),
            member_key: member.key.clone(),
            member_base: member.base.clone(),
            code_hash: member.hash.clone(),
            status: super::super::store::FileStatus::Accepted,
            note: "keep".into(),
            updated_at: 1,
        }
    }

    #[test]
    fn unique_sibling_bodies_retain_keys_through_reordering() {
        let a = member(0, "a");
        let b = member(1, "b");
        let rows = vec![saved(&a), saved(&b)];
        let resolved = resolve(&[member(0, "b"), member(1, "a")], &rows);
        assert_eq!(resolved[0].key, b.key);
        assert_eq!(resolved[1].key, a.key);
        assert!(!resolved[0].uncertain);
        assert!(!resolved[1].uncertain);
    }

    #[test]
    fn changed_ambiguous_sibling_does_not_inherit_a_different_body_label() {
        let a = member(0, "a");
        let b = member(1, "b");
        let rows = vec![saved(&a), saved(&b)];
        let resolved = resolve(&[member(0, "edited"), member(1, "b")], &rows);
        assert_ne!(resolved[0].key, a.key);
        assert_ne!(resolved[0].key, b.key);
        assert!(resolved[0].uncertain);
        assert_eq!(resolved[1].key, b.key);
    }

    #[test]
    fn identical_siblings_keep_distinct_slots_with_explicit_uncertainty() {
        let a = member(0, "same");
        let b = member(1, "same");
        let resolved = resolve(&[a.clone(), b.clone()], &[saved(&a), saved(&b)]);
        assert_eq!(resolved[0].key, a.key);
        assert_eq!(resolved[1].key, b.key);
        assert!(resolved.iter().all(|member| member.uncertain));
    }
    #[test]
    fn a_unique_copy_retains_its_label_when_source_cannot_be_verified() {
        let original = member(0, "reviewed");
        let mut unavailable = original.clone();
        unavailable.hash = None;
        let resolved = resolve(&[unavailable], &[saved(&original)]);
        assert_eq!(resolved[0].key, original.key);
        assert_eq!(resolved[0].hash, None);
    }
}
