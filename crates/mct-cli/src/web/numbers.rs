//! Readable finding numbers ("Duplicate #3", "Dead code #12").
//!
//! The review key of a finding is a 64-character identity hash; people need a
//! short label to match a row of the list with the finding they opened. Each
//! identity gets the next free number of its kind the first time it is seen,
//! and `.mct-index/finding-numbers.tsv` keeps that number, so sorting,
//! filtering, restarting and reindexing never renumber anything.
//!
//! A number is never reused: a finding that disappears keeps it reserved and
//! gets it back if it is detected again. Numbers are only labels; decisions
//! stay keyed by the identity, so a number can never move a decision.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use mct_index::reviews::{FindingKeys, Kind};

const FILE: &str = "finding-numbers.tsv";
const HEADER: &str = "# mct finding numbers v1: kind<TAB>number<TAB>identity";
/// Far above any real project; a larger file is not ours to parse.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct Numbers {
    by_key: HashMap<String, u64>,
    /// Only the findings of this snapshot: a finding that is gone has no page.
    current: HashMap<(Kind, u64), String>,
}

/// One parsed row, or `None` for a line that is not a valid row.
fn row(line: &str) -> Option<(Kind, u64, &str)> {
    let mut fields = line.split('\t');
    let kind = Kind::parse(fields.next()?)?;
    let number = fields.next()?.parse::<u64>().ok().filter(|n| *n > 0)?;
    let identity = fields.next()?;
    let valid = identity.len() == 64 && identity.bytes().all(|b| b.is_ascii_hexdigit());
    (valid && fields.next().is_none()).then_some((kind, number, identity))
}

/// The stored rows, or why the file cannot be trusted (then it is never rewritten).
fn read(path: &Path) -> Result<Vec<(Kind, u64, String)>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("file is larger than 16 MiB".into());
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (kind, number, identity) =
            row(line).ok_or_else(|| format!("line {} is not a valid row", index + 1))?;
        rows.push((kind, number, identity.to_string()));
    }
    Ok(rows)
}

/// Replaces the file through a temporary file and a rename, so a reader never sees half of it.
fn write(path: &Path, rows: &[(Kind, u64, String)]) -> std::io::Result<()> {
    let temporary = path.with_extension("tsv.tmp");
    let mut file = fs::File::create(&temporary)?;
    writeln!(file, "{HEADER}")?;
    for (kind, number, identity) in rows {
        writeln!(file, "{}\t{number}\t{identity}", kind.value())?;
    }
    file.sync_all()?;
    fs::rename(&temporary, path)
}

impl Numbers {
    /// Numbers every finding of the snapshot, in its order, and saves the new
    /// ones. A file that cannot be read or written leaves the numbers working
    /// for this run only, with a warning.
    pub fn load(root: &Path, findings: &[FindingKeys]) -> Self {
        let path = root.join(".mct-index").join(FILE);
        let (mut rows, writable) = match read(&path) {
            Ok(rows) => (rows, true),
            Err(reason) => {
                eprintln!(
                    "warning: finding numbers are not saved until {} is fixed: {reason}",
                    path.display()
                );
                (Vec::new(), false)
            }
        };
        let mut by_key: HashMap<String, u64> = HashMap::new();
        let mut next: HashMap<Kind, u64> = HashMap::new();
        for (kind, number, identity) in &rows {
            by_key.entry(identity.clone()).or_insert(*number);
            let last = next.entry(*kind).or_insert(0);
            *last = (*last).max(*number);
        }
        let known = rows.len();
        for finding in findings {
            if by_key.contains_key(&finding.identity) {
                continue;
            }
            let last = next.entry(finding.kind).or_insert(0);
            *last += 1;
            by_key.insert(finding.identity.clone(), *last);
            rows.push((finding.kind, *last, finding.identity.clone()));
        }
        if writable && rows.len() > known {
            if let Err(error) = write(&path, &rows) {
                eprintln!(
                    "warning: cannot save finding numbers to {}: {error}",
                    path.display()
                );
            }
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
        Self { by_key, current }
    }

    pub fn get(&self, key: &str) -> Option<u64> {
        self.by_key.get(key).copied()
    }

    /// The review key of the current finding of `kind` numbered `number`.
    pub fn key(&self, kind: Kind, number: u64) -> Option<&str> {
        self.current.get(&(kind, number)).map(String::as_str)
    }
}
