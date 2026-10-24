//! Conservative rebinding of reviewed dead-code candidates after ordinal shifts.
//!
//! The detector's legacy ordinal remains the default for a new symbol. Saved
//! provenance can preserve an existing identity only within the same semantic
//! base and when the match is unique. Duplicate text is deliberately unreviewable.

use std::collections::{BTreeMap, BTreeSet};

use mct_index::review_keys::{dead_base_identity, dead_identity_at};
use mct_index::SymbolListEntry;

use super::reviews::{Kind, Record};

/// Bound recovery work for pre-provenance decisions. Anything beyond these
/// limits keeps its old record and is presented as fresh/uncertain work rather
/// than inheriting a decision without proof.
const LEGACY_ORDINAL_LIMIT: usize = 256;
const LEGACY_LOOKUP_BUDGET: usize = 65_536;

/// Returns identity/fingerprint pairs in the same order as `entries`.
///
/// `aliases` is retained provenance keyed by saved identity, containing the
/// language/path/kind/parent/name base hash and last verified content fingerprint.
/// The current stored record, when present, is authoritative for its fingerprint.
/// A body edit retains a known identity only for an otherwise unambiguous base;
/// its different fingerprint then triggers normal reevaluation in the store.
pub(super) fn resolve_dead_identities(
    entries: &[SymbolListEntry],
    default_keys: &[(String, String)],
    aliases: &BTreeMap<String, (String, String)>,
    stored: &[Record],
) -> Vec<(String, String)> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        groups
            .entry(dead_base_identity(entry))
            .or_default()
            .push(index);
    }
    let mut records = BTreeMap::new();
    for record in stored.iter().filter(|record| record.kind == Kind::Dead) {
        let previous = records.entry(record.identity.clone()).or_insert(record);
        if record.at > previous.at {
            *previous = record;
        }
    }
    let mut known = aliases.clone();
    for (identity, (_, fingerprint)) in &mut known {
        if let Some(record) = records.get(identity) {
            *fingerprint = record.fingerprint.clone();
        }
    }

    // Most legacy decisions still occupy their original ordinal. Recognize
    // those directly, then spend a fixed budget recovering displaced siblings.
    let mut unbound: BTreeSet<String> = records
        .keys()
        .filter(|identity| !known.contains_key(*identity))
        .cloned()
        .collect();
    for (base, indices) in &groups {
        let mut ordered = indices.clone();
        ordered.sort_by_key(|index| entries[*index].line);
        for (ordinal, index) in ordered.into_iter().enumerate() {
            let identity = dead_identity_at(&entries[index], ordinal);
            if unbound.remove(&identity) {
                if let Some(record) = records.get(&identity) {
                    known.insert(identity, (base.clone(), record.fingerprint.clone()));
                }
            }
        }
    }
    let mut remaining_lookups = LEGACY_LOOKUP_BUDGET;
    for (base, indices) in &groups {
        let Some(index) = indices.first() else {
            continue;
        };
        for ordinal in 0..LEGACY_ORDINAL_LIMIT {
            if unbound.is_empty() || remaining_lookups == 0 {
                break;
            }
            remaining_lookups -= 1;
            let identity = dead_identity_at(&entries[*index], ordinal);
            if unbound.remove(&identity) {
                if let Some(record) = records.get(&identity) {
                    known.insert(identity, (base.clone(), record.fingerprint.clone()));
                }
            }
        }
    }

    // Reserve all saved/bound identities before assigning any fresh symbol.
    // Otherwise a newly inserted first sibling could steal the saved ordinal
    // needed by an unchanged sibling processed later in the input.
    let reserved: BTreeSet<_> = known.keys().chain(records.keys()).cloned().collect();
    let mut known_by_base: BTreeMap<&str, Vec<(&String, &String)>> = BTreeMap::new();
    for (identity, (base, fingerprint)) in &known {
        known_by_base
            .entry(base)
            .or_default()
            .push((identity, fingerprint));
    }
    let mut assigned = BTreeSet::new();
    let mut resolved: Vec<Option<(String, String)>> = vec![None; entries.len()];
    let fingerprint = |index: usize| default_keys.get(index).map_or("", |key| key.1.as_str());

    for (base, indices) in &groups {
        let same_base = known_by_base
            .get(base.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let uncertain_content = indices.iter().any(|index| fingerprint(*index).is_empty());
        let mut current_by_fingerprint: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        let mut saved_by_fingerprint: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for index in indices {
            if !fingerprint(*index).is_empty() {
                current_by_fingerprint
                    .entry(fingerprint(*index))
                    .or_default()
                    .push(*index);
            }
        }
        for (identity, saved_fingerprint) in same_base {
            if !saved_fingerprint.is_empty() {
                saved_by_fingerprint
                    .entry(saved_fingerprint.as_str())
                    .or_default()
                    .push(identity);
            }
        }
        // Only an exact one-to-one content match may retain a valid verdict.
        for (content, current) in &current_by_fingerprint {
            if uncertain_content || current.len() != 1 {
                continue;
            }
            let Some(saved) = saved_by_fingerprint.get(content) else {
                continue;
            };
            if saved.len() != 1 {
                continue;
            }
            let identity = saved[0];
            if assigned.insert(identity.clone()) {
                resolved[current[0]] = Some((identity.clone(), (*content).to_string()));
            }
        }
        // A single known symbol can change its body without losing identity.
        // With several current or historical siblings, an unmatched body could
        // instead be a new symbol, so it must not take an old identity.
        if indices.len() == 1 && same_base.len() == 1 && resolved[indices[0]].is_none() {
            let identity = same_base[0].0;
            if assigned.insert(identity.clone()) {
                resolved[indices[0]] =
                    Some((identity.clone(), fingerprint(indices[0]).to_string()));
            }
        }
        let mut next_fresh_ordinal = 0;
        for index in indices {
            if resolved[*index].is_some() {
                continue;
            }
            let content = fingerprint(*index);
            let ambiguous = uncertain_content
                || current_by_fingerprint
                    .get(content)
                    .is_some_and(|matches| matches.len() > 1)
                || saved_by_fingerprint
                    .get(content)
                    .is_some_and(|matches| matches.len() > 1);
            let preferred = default_keys.get(*index).map(|key| key.0.clone());
            let identity = preferred
                .filter(|identity| !reserved.contains(identity) && !assigned.contains(identity))
                .or_else(|| {
                    // Each reserved/assigned identity excludes at most one ordinal.
                    // This finite range therefore contains a free ordinal unless
                    // the underlying digest collides, in which case fail closed.
                    let limit = reserved.len().saturating_add(entries.len());
                    while next_fresh_ordinal <= limit {
                        let ordinal = next_fresh_ordinal;
                        next_fresh_ordinal += 1;
                        let identity = dead_identity_at(&entries[*index], ordinal);
                        if !reserved.contains(&identity) && !assigned.contains(&identity) {
                            return Some(identity);
                        }
                    }
                    None
                })
                .unwrap_or_default();
            let content = if ambiguous || identity.is_empty() {
                String::new()
            } else {
                content.to_string()
            };
            assigned.insert(identity.clone());
            resolved[*index] = Some((identity, content));
        }
    }
    resolved
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::web::reviews::Status;

    fn entry(line: u32) -> SymbolListEntry {
        SymbolListEntry {
            name: "twin".into(),
            kind: "function".into(),
            language: "python".into(),
            relative_path: "twins.py".into(),
            line,
            end_line: Some(line + 1),
            parent: None,
            level: None,
        }
    }

    fn content(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn record(identity: &str, fingerprint: &str) -> Record {
        Record {
            kind: Kind::Dead,
            identity: identity.into(),
            fingerprint: fingerprint.into(),
            status: Status::FalsePositive,
            at: 1,
            reason: "reviewed".into(),
        }
    }

    #[test]
    fn second_twin_retains_saved_identity_after_first_is_removed() {
        let symbol = entry(1);
        let saved_id = dead_identity_at(&symbol, 1);
        let fp = content('a');
        let aliases =
            BTreeMap::from([(saved_id.clone(), (dead_base_identity(&symbol), fp.clone()))]);
        let default = vec![(dead_identity_at(&symbol, 0), fp.clone())];
        let saved = record(&saved_id, &fp);
        assert_eq!(
            resolve_dead_identities(&[symbol], &default, &aliases, &[saved]),
            vec![(saved_id, fp)]
        );
    }

    #[test]
    fn one_known_symbol_keeps_identity_when_its_body_changes() {
        let symbol = entry(1);
        let saved_id = dead_identity_at(&symbol, 1);
        let (old, changed) = (content('a'), content('b'));
        let aliases =
            BTreeMap::from([(saved_id.clone(), (dead_base_identity(&symbol), old.clone()))]);
        let result = resolve_dead_identities(
            std::slice::from_ref(&symbol),
            &[(dead_identity_at(&symbol, 0), changed.clone())],
            &aliases,
            &[record(&saved_id, &old)],
        );
        assert_eq!(result, vec![(saved_id, changed)]);
        assert_ne!(
            result[0].1, old,
            "the retained decision must require reevaluation"
        );
    }

    #[test]
    fn reordered_and_new_siblings_cannot_steal_a_saved_identity() {
        let (first, second) = (entry(1), entry(5));
        let saved_id = dead_identity_at(&first, 0);
        let (old, new) = (content('a'), content('b'));
        let aliases =
            BTreeMap::from([(saved_id.clone(), (dead_base_identity(&first), old.clone()))]);
        let defaults = [
            (dead_identity_at(&first, 0), new.clone()),
            (dead_identity_at(&second, 1), old.clone()),
        ];
        let result = resolve_dead_identities(
            &[first, second],
            &defaults,
            &aliases,
            &[record(&saved_id, &old)],
        );
        assert_eq!(result[1], (saved_id.clone(), old));
        assert_ne!(result[0].0, saved_id);
        assert_ne!(result[0].0, result[1].0);
        assert_eq!(result[0].1, new);
    }

    #[test]
    fn identical_current_twins_never_validate_an_ambiguous_old_verdict() {
        let (first, second) = (entry(1), entry(5));
        let saved_id = dead_identity_at(&first, 0);
        let fp = content('a');
        let aliases =
            BTreeMap::from([(saved_id.clone(), (dead_base_identity(&first), fp.clone()))]);
        let defaults = [
            (dead_identity_at(&first, 0), fp.clone()),
            (dead_identity_at(&second, 1), fp.clone()),
        ];
        let result = resolve_dead_identities(
            &[first, second],
            &defaults,
            &aliases,
            &[record(&saved_id, &fp)],
        );
        assert_ne!(result[0].0, result[1].0);
        assert!(result
            .iter()
            .all(|(identity, fingerprint)| identity != &saved_id && fingerprint.is_empty()));
    }

    #[test]
    fn identical_historical_twins_do_not_transfer_to_one_survivor() {
        let symbol = entry(1);
        let (first_id, second_id) = (dead_identity_at(&symbol, 0), dead_identity_at(&symbol, 1));
        let fp = content('a');
        let base = dead_base_identity(&symbol);
        let aliases = BTreeMap::from([
            (first_id.clone(), (base.clone(), fp.clone())),
            (second_id.clone(), (base, fp.clone())),
        ]);
        let result = resolve_dead_identities(
            &[symbol],
            &[(first_id.clone(), fp.clone())],
            &aliases,
            &[record(&first_id, &fp), record(&second_id, &fp)],
        );
        assert_ne!(result[0].0, first_id);
        assert_ne!(result[0].0, second_id);
        assert!(result[0].1.is_empty());
    }

    #[test]
    fn legacy_second_twin_is_recovered_only_through_verified_shared_hashes() {
        let symbol = entry(1);
        let old_id = dead_identity_at(&symbol, 1);
        let fp = content('a');
        let result = resolve_dead_identities(
            std::slice::from_ref(&symbol),
            &[(dead_identity_at(&symbol, 0), fp.clone())],
            &BTreeMap::new(),
            &[record(&old_id, &fp)],
        );
        assert_eq!(result, vec![(old_id, fp)]);
    }

    #[test]
    fn unmatched_multiple_known_siblings_never_assign_old_identity_to_new_code() {
        let symbol = entry(1);
        let base = dead_base_identity(&symbol);
        let (first_id, second_id) = (dead_identity_at(&symbol, 0), dead_identity_at(&symbol, 1));
        let aliases = BTreeMap::from([
            (first_id.clone(), (base.clone(), content('a'))),
            (second_id.clone(), (base, content('b'))),
        ]);
        let fp = content('c');
        let result = resolve_dead_identities(
            &[symbol],
            &[(first_id.clone(), fp.clone())],
            &aliases,
            &[
                record(&first_id, &content('a')),
                record(&second_id, &content('b')),
            ],
        );
        assert_ne!(result[0].0, first_id);
        assert_ne!(result[0].0, second_id);
        assert_eq!(result[0].1, fp);
    }

    #[test]
    fn an_unreadable_same_name_sibling_prevents_a_claim_of_unique_content() {
        let (first, second) = (entry(1), entry(5));
        let saved_id = dead_identity_at(&first, 0);
        let fp = content('a');
        let aliases =
            BTreeMap::from([(saved_id.clone(), (dead_base_identity(&first), fp.clone()))]);
        let defaults = [
            (saved_id.clone(), fp.clone()),
            (dead_identity_at(&second, 1), String::new()),
        ];
        let result = resolve_dead_identities(
            &[first, second],
            &defaults,
            &aliases,
            &[record(&saved_id, &fp)],
        );
        assert!(result.iter().all(|(_, fingerprint)| fingerprint.is_empty()));
    }
}
