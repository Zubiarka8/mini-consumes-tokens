//! Identity, fingerprint and legacy keys of findings.
//!
//! Every key is SHA-256 over length-prefixed fields, as 64 lowercase hex
//! characters. The legacy keys reproduce, byte for byte, what earlier
//! writers stored, so their decisions keep matching.

use std::collections::{BTreeMap, HashMap};

use super::Kind;
use crate::{DuplicateGroup, Normalization, SymbolListEntry};

/// Everything needed to match one current finding against stored decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingKeys {
    pub kind: Kind,
    /// `duplicate-id-v2` or `dead-id-v2`: survives edits of the finding's
    /// code, so a changed finding is recognized and shown for re-evaluation.
    pub identity: String,
    /// The finding's exact code state; a decision suppresses only on a match.
    pub fingerprint: String,
    /// False when a file of the finding has no content hash in the index:
    /// no decision may then hide the finding.
    pub content_known: bool,
    /// Key in `duplicate-reviews.toon` (`A1`/`A2`), when the content is known.
    pub legacy_key: Option<String>,
    /// Identity in the dashboard draft's `reviews.toon` (`B2`).
    pub draft_identity: String,
}

pub(super) fn digest(fields: &[&str]) -> String {
    let mut bytes = Vec::new();
    for value in fields {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    let hash = hmac_sha256::Hash::hash(&bytes);
    let mut hex = String::with_capacity(2 * hash.len());
    for byte in hash {
        hex.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Path, symbol name, symbol kind, start line and end line of a member.
type MemberRange<'a> = (&'a str, &'a str, &'a str, u32, u32);

fn hash_map(file_hashes: &[(String, String)]) -> HashMap<&str, &str> {
    file_hashes
        .iter()
        .map(|(path, hash)| (path.as_str(), hash.as_str()))
        .collect()
}

/// Keys of each duplicate group, in input order. `file_hashes` is
/// [`crate::Index::file_hashes`].
pub fn duplicate_findings(
    groups: &[DuplicateGroup],
    normalization: Normalization,
    file_hashes: &[(String, String)],
) -> Vec<FindingKeys> {
    let hashes = hash_map(file_hashes);
    let mut findings: Vec<FindingKeys> = Vec::with_capacity(groups.len());
    // Groups that share their members, keyed by draft identity, with the
    // sorted member line ranges that order them for the ordinal.
    let mut same_members: BTreeMap<String, Vec<(Vec<MemberRange<'_>>, usize)>> = BTreeMap::new();
    for (id, group) in groups.iter().enumerate() {
        let mut content_known = true;
        let mut members = Vec::with_capacity(group.members.len());
        for member in &group.members {
            let hash = hashes
                .get(member.path.as_str())
                .copied()
                .unwrap_or_else(|| {
                    content_known = false;
                    ""
                });
            members.push((
                member.path.as_str(),
                member.symbol_name.as_str(),
                member.symbol_kind.as_str(),
                member.start_line,
                member.end_line,
                hash,
            ));
        }
        members.sort();
        // The ADR-015 key: `duplicate-reviews.toon` stores exactly this.
        let starts: Vec<String> = members.iter().map(|m| m.3.to_string()).collect();
        let ends: Vec<String> = members.iter().map(|m| m.4.to_string()).collect();
        let mut fields = vec![
            "duplicate-review-v1",
            normalization.as_str(),
            group.language.as_str(),
        ];
        for (member, (start, end)) in members.iter().zip(starts.iter().zip(&ends)) {
            fields.extend([member.0, member.1, member.2, start, end, member.5]);
        }
        let fingerprint = digest(&fields);
        let mut names: Vec<(&str, &str, &str)> = members.iter().map(|m| (m.0, m.1, m.2)).collect();
        names.sort();
        let mut fields = vec!["duplicate-id-v1", group.language.as_str()];
        for (path, name, kind) in &names {
            fields.extend([*path, *name, *kind]);
        }
        let draft_identity = digest(&fields);
        let ranges = members.iter().map(|m| (m.0, m.1, m.2, m.3, m.4)).collect();
        same_members
            .entry(draft_identity.clone())
            .or_default()
            .push((ranges, id));
        findings.push(FindingKeys {
            kind: Kind::Duplicate,
            identity: String::new(),
            legacy_key: content_known.then(|| fingerprint.clone()),
            fingerprint,
            content_known,
            draft_identity,
        });
    }
    for siblings in same_members.values_mut() {
        siblings.sort();
        for (ordinal, (_, id)) in siblings.iter().enumerate() {
            let group = &groups[*id];
            let mut names: Vec<(&str, &str, &str)> = group
                .members
                .iter()
                .map(|m| {
                    (
                        m.path.as_str(),
                        m.symbol_name.as_str(),
                        m.symbol_kind.as_str(),
                    )
                })
                .collect();
            names.sort();
            let ordinal = ordinal.to_string();
            let mut fields = vec!["duplicate-id-v2", group.language.as_str()];
            for (path, name, kind) in &names {
                fields.extend([*path, *name, *kind]);
            }
            fields.push(&ordinal);
            findings[*id].identity = digest(&fields);
        }
    }
    findings
}

/// Keys of the dead-code candidates `candidates` (indices into `symbols`,
/// as returned by [`crate::scan_dead_code`]), in that order. `symbols` must
/// hold every indexed symbol of the candidates' files.
pub fn dead_code_findings(
    symbols: &[SymbolListEntry],
    candidates: &[usize],
    file_hashes: &[(String, String)],
) -> Vec<FindingKeys> {
    let hashes = hash_map(file_hashes);
    fn name_of(entry: &SymbolListEntry) -> (&str, &str, &str, &str) {
        (
            entry.relative_path.as_str(),
            entry.kind.as_str(),
            entry.parent.as_deref().unwrap_or(""),
            entry.name.as_str(),
        )
    }
    let parent = |entry: &SymbolListEntry| entry.parent.clone().unwrap_or_default();

    // `dead-id-v2`: the ordinal among all symbols of the file with the same
    // kind, parent and name, so a sibling gaining a reference shifts nothing.
    // Only names that some candidate has are counted.
    let mut by_name: HashMap<(&str, &str, &str, &str), Vec<usize>> = candidates
        .iter()
        .map(|id| (name_of(&symbols[*id]), Vec::new()))
        .collect();
    for (id, entry) in symbols.iter().enumerate() {
        if let Some(ids) = by_name.get_mut(&name_of(entry)) {
            ids.push(id);
        }
    }
    let mut ordinal = vec![0_usize; symbols.len()];
    for ids in by_name.values_mut() {
        ids.sort_by_key(|id| (symbols[*id].line, symbols[*id].end_line, *id));
        for (position, id) in ids.iter().enumerate() {
            ordinal[*id] = position;
        }
    }

    // `dead-id-v1` (`B2`): the ordinal among candidates of the file, ordered
    // as the dashboard draft ordered them (path, line, name).
    let mut draft_order: Vec<usize> = (0..candidates.len()).collect();
    draft_order.sort_by(|a, b| {
        let (a, b) = (&symbols[candidates[*a]], &symbols[candidates[*b]]);
        (&a.relative_path, a.line, &a.name).cmp(&(&b.relative_path, b.line, &b.name))
    });
    let mut draft_ordinal = vec![0_usize; candidates.len()];
    let mut seen: BTreeMap<(&str, &str, String, &str), usize> = BTreeMap::new();
    for position in draft_order {
        let entry = &symbols[candidates[position]];
        let next = seen
            .entry((
                &entry.relative_path,
                &entry.kind,
                parent(entry),
                &entry.name,
            ))
            .or_default();
        draft_ordinal[position] = *next;
        *next += 1;
    }

    candidates
        .iter()
        .zip(draft_ordinal)
        .map(|(&id, draft)| {
            let entry = &symbols[id];
            let parent = parent(entry);
            let hash = hashes.get(entry.relative_path.as_str()).copied();
            let line = entry.line.to_string();
            let end = entry.end_line.unwrap_or(entry.line).to_string();
            let identity = digest(&[
                "dead-id-v2",
                &entry.language,
                &entry.relative_path,
                &entry.kind,
                &parent,
                &entry.name,
                &ordinal[id].to_string(),
            ]);
            let draft_identity = digest(&[
                "dead-id-v1",
                &entry.language,
                &entry.relative_path,
                &entry.kind,
                &parent,
                &entry.name,
                &draft.to_string(),
            ]);
            FindingKeys {
                kind: Kind::DeadCode,
                identity,
                fingerprint: digest(&["dead-fp-v1", hash.unwrap_or(""), &line, &end]),
                content_known: hash.is_some(),
                legacy_key: hash.map(|hash| {
                    digest(&[
                        "dead-code-review-v1",
                        &entry.relative_path,
                        &entry.name,
                        &entry.kind,
                        &line,
                        hash,
                    ])
                }),
                draft_identity,
            }
        })
        .collect()
}
