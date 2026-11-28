//! What changed between two indexes of a project (issue #637), matched by
//! what a definition is, never by SQLite row ids: a file by its path, a
//! symbol by its path, kind, parent and name (its signature compared
//! apart), and a relation by its source
//! symbol, kind and target name. Two indexes built at different times, by
//! different versions, or from scratch number their rows differently; a
//! row id says nothing across them.
//!
//! Each side is read once. Same-keyed rows (two `new` methods of one impl,
//! three calls to `f` in one function) are compared as multisets, so a
//! repeated row is neither lost nor double counted.

use std::collections::BTreeMap;

use crate::queries::{resolution_of, Resolution};
use crate::{Index, Result};

/// Examples kept per section unless the caller asks for another number.
pub const COMPARE_EXAMPLES: usize = 20;

/// Additions and removals of one kind of row, with a few examples of each.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RowChanges {
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub added_examples: Vec<String>,
    pub removed_examples: Vec<String>,
}

/// One row whose value changed under the same key: a relation's outcome
/// or a symbol's signature, before and after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeChange {
    pub key: String,
    pub before: String,
    pub after: String,
}

/// What [`compare_indexes`] found.
#[derive(Debug, Default)]
pub struct IndexComparison {
    pub files: RowChanges,
    /// Files on both sides whose content hash differs.
    pub files_changed: Vec<String>,
    pub symbols: RowChanges,
    /// Symbols on both sides whose signature differs.
    pub signature_changes: usize,
    pub signature_examples: Vec<OutcomeChange>,
    pub relations: RowChanges,
    /// How many relations moved from one resolution to another, by
    /// `(before, after)`; a relation whose resolution held but whose
    /// target changed counts under the same pair.
    pub transitions: BTreeMap<(String, String), usize>,
    pub outcome_examples: Vec<OutcomeChange>,
}

/// `before` and `after` compared row by row, with at most `examples`
/// examples per section.
pub fn compare_indexes(before: &Index, after: &Index, examples: usize) -> Result<IndexComparison> {
    let (old_files, new_files) = (files(before)?, files(after)?);
    let mut comparison = IndexComparison::default();
    let old_keys: BTreeMap<String, usize> = old_files.keys().map(|k| (k.clone(), 1)).collect();
    let new_keys: BTreeMap<String, usize> = new_files.keys().map(|k| (k.clone(), 1)).collect();
    comparison.files = diff_counts(&old_keys, &new_keys, examples);
    comparison.files_changed = old_files
        .iter()
        .filter(|(path, hash)| new_files.get(*path).is_some_and(|h| h != *hash))
        .map(|(path, _)| path.clone())
        .collect();
    let (old_symbols, new_symbols) = (symbols(before)?, symbols(after)?);
    comparison.symbols = diff_counts(&counts(&old_symbols), &counts(&new_symbols), examples);
    for change in changed_values(&old_symbols, &new_symbols) {
        comparison.signature_changes += 1;
        if comparison.signature_examples.len() < examples {
            comparison.signature_examples.push(change);
        }
    }
    let (old_relations, new_relations) = (relations(before)?, relations(after)?);
    comparison.relations = diff_counts(&counts(&old_relations), &counts(&new_relations), examples);
    for change in changed_values(&old_relations, &new_relations) {
        let pair = (
            resolution_word(&change.before),
            resolution_word(&change.after),
        );
        *comparison.transitions.entry(pair).or_default() += 1;
        if comparison.outcome_examples.len() < examples {
            comparison.outcome_examples.push(change);
        }
    }
    Ok(comparison)
}

/// How many rows each key has.
fn counts(map: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, usize> {
    map.iter().map(|(k, v)| (k.clone(), v.len())).collect()
}

/// For each key on both sides, the values only one side has, paired in
/// sorted order: the same row, with a new value.
fn changed_values(
    old: &BTreeMap<String, Vec<String>>,
    new: &BTreeMap<String, Vec<String>>,
) -> Vec<OutcomeChange> {
    let mut changes = Vec::new();
    for (key, was) in old {
        let Some(now) = new.get(key) else {
            continue;
        };
        let (gone, came) = multiset_difference(was, now);
        changes.extend(
            gone.into_iter()
                .zip(came)
                .map(|(before, after)| OutcomeChange {
                    key: key.clone(),
                    before: before.clone(),
                    after: after.clone(),
                }),
        );
    }
    changes
}

/// The resolution word an outcome starts with (`resolved src/a.rs:f`).
fn resolution_word(outcome: &str) -> String {
    outcome.split(' ').next().unwrap_or(outcome).to_string()
}

/// The rows of `old` missing from `new` and of `new` missing from `old`,
/// each once per extra occurrence, in sorted order.
fn multiset_difference<'a>(
    old: &'a [String],
    new: &'a [String],
) -> (Vec<&'a String>, Vec<&'a String>) {
    let mut old: Vec<&String> = old.iter().collect();
    let mut new: Vec<&String> = new.iter().collect();
    old.sort();
    new.sort();
    let (mut gone, mut came) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some(a), Some(b)) if a == b => {
                i += 1;
                j += 1;
            }
            (Some(a), Some(b)) if a < b => {
                gone.push(*a);
                i += 1;
            }
            (Some(a), None) => {
                gone.push(*a);
                i += 1;
            }
            (_, Some(b)) => {
                came.push(*b);
                j += 1;
            }
            (None, None) => break,
        }
    }
    (gone, came)
}

/// Rows added, removed and kept between two key multisets.
fn diff_counts(
    old: &BTreeMap<String, usize>,
    new: &BTreeMap<String, usize>,
    examples: usize,
) -> RowChanges {
    let mut changes = RowChanges::default();
    let note = |kept: &mut Vec<String>, key: &str| {
        if kept.len() < examples {
            kept.push(key.to_string());
        }
    };
    for (key, &was) in old {
        let now = new.get(key).copied().unwrap_or(0);
        changes.unchanged += was.min(now);
        if was > now {
            changes.removed += was - now;
            note(&mut changes.removed_examples, key);
        }
    }
    for (key, &now) in new {
        let was = old.get(key).copied().unwrap_or(0);
        if now > was {
            changes.added += now - was;
            note(&mut changes.added_examples, key);
        }
    }
    changes
}

fn files(index: &Index) -> Result<BTreeMap<String, String>> {
    let mut stmt = index
        .conn
        .prepare("SELECT relative_path, content_hash FROM files")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// `path kind parent.name` of a symbol row, the key both sides share.
const SYMBOL_KEY: &str = "f.relative_path || ' ' || s.kind || ' ' || \
     COALESCE(s.parent || '.', '') || s.name";

/// Each symbol's recorded signature (empty when none) by its key.
fn symbols(index: &Index) -> Result<BTreeMap<String, Vec<String>>> {
    let sql = format!(
        "SELECT {SYMBOL_KEY}, COALESCE(d.signature, '') \
         FROM symbols s JOIN files f ON f.id = s.file_id \
         LEFT JOIN symbol_details d ON d.symbol_id = s.id"
    );
    let mut stmt = index.conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (key, signature) = row?;
        map.entry(key).or_default().push(signature);
    }
    Ok(map)
}

/// Each relation's outcome by its key: `resolution target[|target…]`, the
/// targets as `path:parent.name`, sorted.
fn relations(index: &Index) -> Result<BTreeMap<String, Vec<String>>> {
    let sql = format!(
        "SELECT {SYMBOL_KEY} || ' ' || r.kind || ' ' || r.to_name,
                r.external,
                (SELECT COUNT(*) FROM relation_candidates c WHERE c.relation_id = r.id),
                r.member,
                r.join_kind IS NOT NULL AND (SELECT COUNT(*) FROM relations o
                                             WHERE o.join_kind = r.join_kind
                                               AND o.from_symbol_id = r.from_symbol_id
                                               AND o.line = r.line AND o.column = r.column) > 1,
                (SELECT group_concat(k, '|') FROM (
                    SELECT tf.relative_path || ':' || COALESCE(t.parent || '.', '') || t.name AS k
                      FROM relation_candidates c
                      JOIN symbols t ON t.id = c.symbol_id
                      JOIN files tf ON tf.id = t.file_id
                     WHERE c.relation_id = r.id
                     ORDER BY k))
           FROM relations r
           JOIN symbols s ON s.id = r.from_symbol_id
           JOIN files f ON f.id = s.file_id"
    );
    let mut stmt = index.conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let count: i64 = row.get(2)?;
        let resolution = resolution_of(row.get(1)?, count, row.get(3)?, row.get(4)?);
        let targets: Option<String> = row.get(5)?;
        let outcome = match (resolution, targets) {
            (Resolution::External | Resolution::Unresolved, _) | (_, None) => {
                resolution.as_str().to_string()
            }
            (_, Some(targets)) => format!("{} {targets}", resolution.as_str()),
        };
        Ok((key, outcome))
    })?;
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (key, outcome) = row?;
        map.entry(key).or_default().push(outcome);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn repeated_rows_are_compared_as_multisets() {
        let old = strings(&["a", "a", "b"]);
        let new = strings(&["a", "c", "c"]);
        let (gone, came) = multiset_difference(&old, &new);
        assert_eq!(gone, [&"a".to_string(), &"b".to_string()]);
        assert_eq!(came, [&"c".to_string(), &"c".to_string()]);
    }

    #[test]
    fn counts_split_into_added_removed_and_kept() {
        let old = BTreeMap::from([("x".to_string(), 2), ("y".to_string(), 1)]);
        let new = BTreeMap::from([("x".to_string(), 1), ("z".to_string(), 3)]);
        let changes = diff_counts(&old, &new, 20);
        assert_eq!(
            (changes.added, changes.removed, changes.unchanged),
            (3, 2, 1)
        );
        assert_eq!(changes.added_examples, ["z"]);
        assert_eq!(changes.removed_examples, ["x", "y"]);
    }
}
