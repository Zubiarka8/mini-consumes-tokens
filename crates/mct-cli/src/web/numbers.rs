//! Readable finding numbers ("Duplicate #3", "Dead code #12").
//!
//! The review key of a finding is a 64-character identity hash; people need a
//! short label to match a row of the list with the finding they opened. Each
//! identity gets the next free number of its kind the first time it is seen,
//! and `.mct-index/finding-numbers.tsv` keeps that number, so sorting,
//! filtering, restarting and reindexing never renumber anything.
//!
//! A number is not reused while the exclusion rules stay the same: a finding
//! that disappears keeps it reserved and gets it back if it is detected
//! again. When the rules change (an edit to `.mctignore`, or to the
//! `.gitignore` it imports), the set of findings is rebuilt, so the numbers
//! are too: every finding with a review decision keeps its number, the others
//! are numbered again from 1 around them, and the numbers of findings that
//! are gone and were never reviewed are released.
//!
//! Numbers are only labels; decisions stay keyed by the identity, so a number
//! can never move a decision.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use mct_index::reviews::{is_review_key, FindingKeys, Kind};

const FILE: &str = "finding-numbers.tsv";
const HEADER: &str = "# mct finding numbers v2: kind<TAB>number<TAB>identity[<TAB>kept]";
/// The comment line that records which exclusion rules the numbers belong to.
const RULES_PREFIX: &str = "# rules ";
/// Marks the row of a finding that has a review decision.
const KEPT: &str = "kept";
/// Far above any real project; a larger file is not ours to parse.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    kind: Kind,
    number: u64,
    identity: String,
    /// The finding has a review decision: its number survives a renumbering.
    kept: bool,
}

/// What the file holds.
#[derive(Default)]
struct Stored {
    rows: Vec<Row>,
    /// The fingerprint of the rules the numbers were given under, if recorded
    /// (a v1 file has none).
    rules: Option<String>,
}

pub(super) struct Numbers {
    path: PathBuf,
    writable: bool,
    rules: String,
    rows: Vec<Row>,
    by_key: HashMap<String, u64>,
    /// Only the findings of this snapshot: a finding that is gone has no page.
    current: HashMap<(Kind, u64), String>,
}

/// One parsed row, or `None` for a line that is not a valid row.
fn row(line: &str) -> Option<Row> {
    let mut fields = line.split('\t');
    let kind = Kind::parse(fields.next()?)?;
    let number = fields.next()?.parse::<u64>().ok().filter(|n| *n > 0)?;
    let identity = fields.next()?;
    let kept = match fields.next() {
        None => false,
        Some(KEPT) => true,
        Some(_) => return None,
    };
    (is_review_key(identity) && fields.next().is_none()).then(|| Row {
        kind,
        number,
        identity: identity.to_string(),
        kept,
    })
}

/// The stored rows, or why the file cannot be trusted (then it is never rewritten).
fn read(path: &Path) -> Result<Stored, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Stored::default()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("file is larger than 16 MiB".into());
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut stored = Stored::default();
    for (index, line) in text.lines().enumerate() {
        if let Some(rules) = line.strip_prefix(RULES_PREFIX) {
            stored.rules = Some(rules.trim().to_string());
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let row = row(line).ok_or_else(|| format!("line {} is not a valid row", index + 1))?;
        stored.rows.push(row);
    }
    Ok(stored)
}

/// Replaces the file through a temporary file and a rename, so a reader never sees half of it.
fn write(path: &Path, rules: &str, rows: &[Row]) -> std::io::Result<()> {
    let temporary = path.with_extension("tsv.tmp");
    let mut file = fs::File::create(&temporary)?;
    writeln!(file, "{HEADER}")?;
    writeln!(file, "{RULES_PREFIX}{rules}")?;
    for row in rows {
        write!(
            file,
            "{}\t{}\t{}",
            row.kind.value(),
            row.number,
            row.identity
        )?;
        if row.kept {
            write!(file, "\t{KEPT}")?;
        }
        writeln!(file)?;
    }
    file.sync_all()?;
    fs::rename(&temporary, path)
}

/// A stable fingerprint (64-bit FNV-1a) of the project's effective exclusion
/// patterns: the same rules give the same text across runs and builds, and
/// comments or blank lines in the ignore files do not count.
fn rules_fingerprint(root: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for pattern in mct_index::read_ignore_file(root) {
        for byte in pattern.as_bytes().iter().chain(b"\n") {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Numbers the findings again: kept rows keep their number (and stay even if
/// their finding is gone), and every other current finding, in snapshot
/// order, takes the lowest number of its kind that no kept row holds.
fn renumber(rows: &[Row], findings: &[FindingKeys]) -> Vec<Row> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut renumbered: Vec<Row> = rows
        .iter()
        .filter(|row| row.kept && seen.insert(row.identity.as_str()))
        .cloned()
        .collect();
    let mut taken: HashSet<(Kind, u64)> = renumbered
        .iter()
        .map(|row| (row.kind, row.number))
        .collect();
    let mut next: HashMap<Kind, u64> = HashMap::new();
    for finding in findings {
        if !seen.insert(finding.identity.as_str()) {
            continue;
        }
        let candidate = next.entry(finding.kind).or_insert(1);
        while !taken.insert((finding.kind, *candidate)) {
            *candidate += 1;
        }
        renumbered.push(Row {
            kind: finding.kind,
            number: *candidate,
            identity: finding.identity.clone(),
            kept: false,
        });
    }
    renumbered
}

impl Numbers {
    /// Numbers every finding of the snapshot, in its order, and saves what
    /// changed. `reviewed` holds the identities of the current findings that
    /// have a review decision. A file that cannot be read or written leaves
    /// the numbers working for this run only, with a warning.
    pub fn load(root: &Path, findings: &[FindingKeys], reviewed: &HashSet<&str>) -> Self {
        let path = root.join(".mct-index").join(FILE);
        let rules = rules_fingerprint(root);
        let (stored, writable) = match read(&path) {
            Ok(stored) => (stored, true),
            Err(reason) => {
                eprintln!(
                    "warning: finding numbers are not saved until {} is fixed: {reason}",
                    path.display()
                );
                (Stored::default(), false)
            }
        };
        let before = stored.rows.clone();
        let mut rows = stored.rows;
        // Current findings carry their decision now; a gone finding keeps the
        // mark it had when it was last seen.
        let current_keys: HashSet<&str> = findings.iter().map(|f| f.identity.as_str()).collect();
        for row in &mut rows {
            if current_keys.contains(row.identity.as_str()) {
                row.kept = reviewed.contains(row.identity.as_str());
            }
        }
        if !rows.is_empty() && stored.rules.as_deref() != Some(rules.as_str()) {
            rows = renumber(&rows, findings);
        }
        let mut by_key: HashMap<String, u64> = HashMap::new();
        let mut next: HashMap<Kind, u64> = HashMap::new();
        for row in &rows {
            by_key.entry(row.identity.clone()).or_insert(row.number);
            let last = next.entry(row.kind).or_insert(0);
            *last = (*last).max(row.number);
        }
        for finding in findings {
            if by_key.contains_key(&finding.identity) {
                continue;
            }
            let last = next.entry(finding.kind).or_insert(0);
            *last += 1;
            by_key.insert(finding.identity.clone(), *last);
            rows.push(Row {
                kind: finding.kind,
                number: *last,
                identity: finding.identity.clone(),
                kept: reviewed.contains(finding.identity.as_str()),
            });
        }
        let current = findings
            .iter()
            .map(|finding| {
                (
                    (finding.kind, by_key[&finding.identity]),
                    finding.identity.clone(),
                )
            })
            .collect();
        let changed = rows != before || stored.rules.as_deref() != Some(rules.as_str());
        let numbers = Self {
            path,
            writable,
            rules,
            rows,
            by_key,
            current,
        };
        if changed {
            numbers.save();
        }
        numbers
    }

    fn save(&self) {
        if !self.writable {
            return;
        }
        if let Err(error) = write(&self.path, &self.rules, &self.rows) {
            eprintln!(
                "warning: cannot save finding numbers to {}: {error}",
                self.path.display()
            );
        }
    }

    /// Records whether the finding `key` has a review decision, so its number
    /// survives the next renumbering.
    pub fn set_reviewed(&mut self, key: &str, reviewed: bool) {
        let mut changed = false;
        for row in self.rows.iter_mut().filter(|row| row.identity == key) {
            changed |= row.kept != reviewed;
            row.kept = reviewed;
        }
        if changed {
            self.save();
        }
    }

    pub fn get(&self, key: &str) -> Option<u64> {
        self.by_key.get(key).copied()
    }

    /// The review key of the current finding of `kind` numbered `number`.
    pub fn key(&self, kind: Kind, number: u64) -> Option<&str> {
        self.current.get(&(kind, number)).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(kind: Kind, n: u8) -> FindingKeys {
        let identity = format!("{n:02x}").repeat(32);
        assert!(is_review_key(&identity));
        FindingKeys {
            kind,
            identity,
            fingerprint: String::new(),
            content_known: true,
            legacy_key: None,
            draft_identity: String::new(),
        }
    }

    fn stored(kind: Kind, number: u64, finding: &FindingKeys, kept: bool) -> Row {
        Row {
            kind,
            number,
            identity: finding.identity.clone(),
            kept,
        }
    }

    #[test]
    fn a_v1_row_still_parses_and_an_unknown_flag_is_refused() {
        let id = "ab".repeat(32);
        let kept = |line: String| row(&line).map(|row| row.kept);
        assert_eq!(kept(format!("dead-code\t3\t{id}")), Some(false));
        assert_eq!(kept(format!("dead-code\t3\t{id}\tkept")), Some(true));
        assert!(row(&format!("dead-code\t3\t{id}\tother")).is_none());
    }

    #[test]
    fn renumbering_keeps_reviewed_numbers_and_fills_around_them() {
        let a = finding(Kind::DeadCode, 1);
        let b = finding(Kind::DeadCode, 2);
        let c = finding(Kind::DeadCode, 3);
        let gone_reviewed = finding(Kind::DeadCode, 4);
        let gone = finding(Kind::DeadCode, 5);
        let dup = finding(Kind::Duplicate, 6);
        let rows = vec![
            stored(Kind::DeadCode, 40, &a, false),
            stored(Kind::DeadCode, 2, &b, true),
            stored(Kind::DeadCode, 7, &gone_reviewed, true),
            stored(Kind::DeadCode, 1, &gone, false),
            stored(Kind::Duplicate, 9, &dup, false),
        ];
        let renumbered = renumber(&rows, &[a.clone(), b.clone(), c.clone(), dup.clone()]);
        let number = |f: &FindingKeys| {
            renumbered
                .iter()
                .find(|r| r.identity == f.identity)
                .map(|r| r.number)
        };
        assert_eq!(number(&b), Some(2), "a reviewed finding keeps its number");
        assert_eq!(number(&gone_reviewed), Some(7), "even while it is gone");
        assert_eq!(number(&a), Some(1));
        assert_eq!(number(&c), Some(3), "#2 is held, so the next free one");
        assert_eq!(
            number(&gone),
            None,
            "an unreviewed gone finding is released"
        );
        assert_eq!(number(&dup), Some(1), "each kind counts on its own");
    }
}
