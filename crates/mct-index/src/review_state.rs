//! Read-only effective review decisions for AI-facing findings.
//!
//! Missing user state means all candidates remain visible. Invalid state returns
//! an error rather than silently treating unverified code as already reviewed.
//! This module never creates, migrates, or writes a dashboard/index database.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{ensure, Context};
use rusqlite::{Connection, OpenFlags};

use crate::dead_identity::resolve_dead_identities;
use crate::review_keys::{self, Fingerprints};
use crate::{DuplicateGroup, NavigationSnapshot, Normalization, SymbolListEntry};

/// Highest dashboard migration understood by effective-review readers.
pub const SUPPORTED_SCHEMA_VERSION: i64 = 4;

const MAX_ROWS: usize = 100_000;
/// Placeholder fingerprint of an annotation-only finding, as the dashboard
/// store reports it; it never matches real content.
const NO_FINGERPRINT: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const HEADER: &str = "{kind,identity,fingerprint,status,at,reason}:";

#[derive(Clone, Debug)]
struct Decision {
    fingerprint: String,
    status: String,
}

/// Current group-level interpretation; copy workflow labels never enter this model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateReviewState {
    pub identity: String,
    pub saved_decision: String,
    pub effective_decision: String,
    /// unreviewed, valid, changed, unavailable, or ambiguous.
    pub validity: &'static str,
    pub source_verified: bool,
    pub actionable: bool,
}

/// A consistent read of the saved decisions, not a second analysis index.
#[derive(Default)]
pub struct ReviewSnapshot {
    decisions: BTreeMap<(String, String), Decision>,
    legacy_duplicates: BTreeSet<String>,
    deleted: BTreeSet<(String, String)>,
    dead_aliases: BTreeMap<String, (String, String)>,
    /// Every retained dead-code identity with its stored fingerprint, exactly
    /// the set the dashboard store lists: decisions (restored rows included),
    /// then history and annotation-only identities that are not archived.
    retained_dead: BTreeMap<String, String>,
}

impl ReviewSnapshot {
    /// Read existing state only. No directory or database is created.
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let dir = root.join(".mct-index");
        let path = dir.join("dashboard.sqlite3");
        safe_path(&path)?;
        let mut state = Self {
            legacy_duplicates: read_legacy(&dir.join("duplicate-reviews.toon"))?,
            ..Self::default()
        };
        if !path.exists() {
            let (decisions, history) = read_toon(&dir.join("reviews.toon"))?;
            state.decisions = decisions;
            state.retain_dead(history, &BTreeMap::new());
            return Ok(state);
        }
        let _access = crate::coordination::DatabaseAccess::shared(
            &path,
            crate::coordination::DEFAULT_TIMEOUT,
        )?;
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(
            version <= SUPPORTED_SCHEMA_VERSION,
            "dashboard schema is newer than this reader supports"
        );
        let transaction = conn.transaction()?;
        let imported: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key = 'reviews_toon_imported')",
            [],
            |row| row.get(0),
        )?;
        let mut toon_history = BTreeMap::new();
        if !imported {
            (state.decisions, toon_history) = read_toon(&dir.join("reviews.toon"))?;
        }
        // Restoring a decision does not consume the active-decision quota.
        // Bound both categories independently so retained Pending tombstones
        // cannot make a valid active decision unreadable or be truncated away.
        let mut seen = BTreeSet::new();
        for (predicate, diagnostic) in [
            (
                "status IS NOT 'pending'",
                "too many active dashboard decisions",
            ),
            (
                "status = 'pending'",
                "too many retained pending dashboard decisions",
            ),
        ] {
            let mut query = transaction.prepare(&format!(
                "SELECT kind, identity, fingerprint, status FROM reviews WHERE {predicate} LIMIT ?1"
            ))?;
            let rows = query.query_map([MAX_ROWS as i64 + 1], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            for (count, row) in rows.enumerate() {
                ensure!(count < MAX_ROWS, "{diagnostic}");
                let (kind, identity, fingerprint, status) = row?;
                validate(&kind, &identity, &fingerprint, &status)?;
                let key = (kind, identity);
                ensure!(
                    seen.insert(key.clone()),
                    "duplicate dashboard review identity"
                );
                state.decisions.insert(
                    key,
                    Decision {
                        fingerprint,
                        status,
                    },
                );
            }
        }
        let mut query = transaction.prepare(
            "SELECT key, value FROM meta WHERE key LIKE 'review_deleted:%' OR key LIKE 'review_archived:%' OR key LIKE 'dead_base:%' OR key LIKE 'dead_fingerprint:%' LIMIT ?1",
        )?;
        let rows = query.query_map([MAX_ROWS as i64 * 8 + 1], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut bases = BTreeMap::new();
        let mut fingerprints = BTreeMap::new();
        let mut archived = BTreeMap::new();
        for (count, row) in rows.enumerate() {
            ensure!(count < MAX_ROWS * 8, "too much dashboard identity metadata");
            let (key, value) = row?;
            if let Some(key) = key.strip_prefix("review_archived:") {
                let (kind, identity) = key.split_once(':').context("invalid archive key")?;
                ensure!(
                    valid_kind(kind) && valid_key(identity),
                    "invalid archive key"
                );
                ensure!(
                    matches!(value.as_str(), "0" | "1"),
                    "invalid archive marker"
                );
                if kind == "dead" {
                    archived.insert(identity.to_string(), value == "1");
                }
            } else if let Some(key) = key.strip_prefix("review_deleted:") {
                let (kind, identity) = key.split_once(':').context("invalid deletion key")?;
                ensure!(
                    valid_kind(kind) && valid_key(identity),
                    "invalid deletion key"
                );
                ensure!(
                    matches!(value.as_str(), "0" | "1"),
                    "invalid deletion marker"
                );
                if value == "1" {
                    state
                        .deleted
                        .insert((kind.to_string(), identity.to_string()));
                }
            } else if let Some(identity) = key.strip_prefix("dead_base:") {
                ensure!(
                    valid_key(identity) && valid_key(&value),
                    "invalid dead identity binding"
                );
                bases.insert(identity.to_string(), value);
            } else if let Some(identity) = key.strip_prefix("dead_fingerprint:") {
                ensure!(
                    valid_key(identity) && valid_key(&value),
                    "invalid dead fingerprint binding"
                );
                fingerprints.insert(identity.to_string(), value);
            }
        }
        for (identity, base) in bases {
            if let Some(fingerprint) = fingerprints.remove(&identity) {
                state.dead_aliases.insert(identity, (base, fingerprint));
            }
        }
        drop(query);
        let history = retained_annotations(&transaction, toon_history)?;
        transaction.commit()?;
        state.retain_dead(history, &archived);
        Ok(state)
    }

    /// Mirrors the dashboard store's retained-finding rule for dead code: a
    /// current row's fingerprint wins, else the newest history fingerprint,
    /// else the annotation placeholder; explicitly archived identities, and
    /// deleted ones not reopened by a later annotation, are left out.
    fn retain_dead(
        &mut self,
        history: BTreeMap<String, String>,
        archived: &BTreeMap<String, bool>,
    ) {
        let mut retained = history;
        for ((kind, identity), decision) in &self.decisions {
            if kind == "dead" {
                retained.insert(identity.clone(), decision.fingerprint.clone());
            }
        }
        retained.retain(|identity, _| match archived.get(identity) {
            Some(archived) => !archived,
            None => !self
                .deleted
                .contains(&("dead".to_string(), identity.clone())),
        });
        self.retained_dead = retained;
    }

    /// Whether rendered report caches must be bypassed to resolve live decisions.
    pub fn has_decisions(&self) -> bool {
        !self.decisions.is_empty() || !self.legacy_duplicates.is_empty() || !self.deleted.is_empty()
    }

    /// Identity and fingerprint of each dead-code candidate, in input order,
    /// from its default ordinal keys (`Fingerprints::dead_keys`). The dashboard
    /// and every AI-facing output resolve identities only here, so a saved
    /// decision is attributed to the same symbol everywhere. An ambiguous or
    /// unverifiable candidate gets an empty fingerprint.
    pub fn resolve_dead(
        &self,
        entries: &[SymbolListEntry],
        default_keys: &[(String, String)],
    ) -> Vec<(String, String)> {
        resolve_dead_identities(
            entries,
            default_keys,
            &self.dead_aliases,
            &self.retained_dead,
        )
    }

    /// Suppress only an unchanged, explicitly discarded finding. Pending,
    /// confirmed, changed and unverifiable findings remain available to agents.
    pub fn suppresses(
        &self,
        kind: &str,
        identity: &str,
        fingerprint: &str,
        legacy_fingerprint: &str,
    ) -> bool {
        if !valid_key(fingerprint) {
            return false;
        }
        let key = (kind.to_string(), identity.to_string());
        if self.deleted.contains(&key) {
            return false;
        }
        if let Some(decision) = self.decisions.get(&key) {
            return matches!(decision.status.as_str(), "false_positive" | "ignored")
                && (decision.fingerprint == fingerprint
                    || decision.fingerprint == legacy_fingerprint);
        }
        kind == "duplicate" && self.legacy_duplicates.contains(legacy_fingerprint)
    }

    /// Resolve identities and filter before output pagination, retaining order.
    pub fn filter_dead(
        &self,
        root: &Path,
        navigation: &NavigationSnapshot,
        entries: Vec<SymbolListEntry>,
    ) -> Vec<SymbolListEntry> {
        let legacy = review_keys::legacy_dead_keys(&entries, navigation);
        let defaults = Fingerprints::new(root, navigation).dead_keys(&entries, navigation);
        let keys = self.resolve_dead(&entries, &defaults);
        entries
            .into_iter()
            .zip(keys)
            .zip(legacy)
            .filter_map(|((entry, (identity, fingerprint)), (_, old))| {
                (!self.suppresses("dead", &identity, &fingerprint, &old)).then_some(entry)
            })
            .collect()
    }

    /// Flags are aligned to the original group order; ambiguous identities stay visible.
    pub fn duplicate_visibility(
        &self,
        root: &Path,
        navigation: &NavigationSnapshot,
        groups: &[DuplicateGroup],
        normalization: Normalization,
    ) -> anyhow::Result<Vec<bool>> {
        Ok(self
            .duplicate_states(root, navigation, groups, normalization)?
            .into_iter()
            .map(|state| state.actionable)
            .collect())
    }

    /// Resolve diagnostics and suppression together for every output surface.
    /// Historical values are read only; ambiguous/unverifiable matches stay actionable.
    pub fn duplicate_states(
        &self,
        root: &Path,
        navigation: &NavigationSnapshot,
        groups: &[DuplicateGroup],
        normalization: Normalization,
    ) -> anyhow::Result<Vec<DuplicateReviewState>> {
        let ids = review_keys::group_identities(groups);
        let legacy = review_keys::legacy_group_keys(groups, navigation, normalization)?;
        let fingerprints = Fingerprints::new(root, navigation).group_keys(groups, normalization);
        let mut counts = BTreeMap::new();
        for id in &ids {
            *counts.entry(id.clone()).or_insert(0usize) += 1;
        }
        Ok(ids
            .into_iter()
            .zip(fingerprints)
            .zip(legacy)
            .map(|((identity, fingerprint), old)| {
                let key = ("duplicate".to_string(), identity.clone());
                let deleted = self.deleted.contains(&key);
                let decision = self.decisions.get(&key).filter(|_| !deleted);
                let legacy =
                    !deleted && decision.is_none() && self.legacy_duplicates.contains(&old);
                let ambiguous = counts.get(&identity).copied().unwrap_or(0) != 1;
                let source_verified = valid_key(&fingerprint);
                let saved = decision.map(|d| d.status.as_str()).unwrap_or(if legacy {
                    "false_positive"
                } else {
                    "pending"
                });
                let valid = !ambiguous
                    && source_verified
                    && (decision
                        .is_some_and(|d| d.fingerprint == fingerprint || d.fingerprint == old)
                        || legacy);
                let validity = if ambiguous {
                    "ambiguous"
                } else if !source_verified {
                    "unavailable"
                } else if valid {
                    "valid"
                } else if decision.is_some() {
                    "changed"
                } else {
                    "unreviewed"
                };
                let actionable =
                    ambiguous || !self.suppresses("duplicate", &identity, &fingerprint, &old);
                DuplicateReviewState {
                    identity,
                    saved_decision: saved.into(),
                    effective_decision: if valid { saved } else { "pending" }.into(),
                    validity,
                    source_verified,
                    actionable,
                }
            })
            .collect())
    }
}

fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
fn valid_kind(kind: &str) -> bool {
    matches!(kind, "dead" | "duplicate")
}
fn validate(kind: &str, identity: &str, fingerprint: &str, status: &str) -> anyhow::Result<()> {
    ensure!(
        valid_kind(kind) && valid_key(identity) && valid_key(fingerprint),
        "invalid review key"
    );
    ensure!(
        matches!(
            status,
            "pending" | "confirmed" | "false_positive" | "ignored"
        ),
        "invalid review status"
    );
    Ok(())
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
    let mut text = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    ensure!(text.len() as u64 <= MAX_BYTES, "review state is too large");
    Ok(Some(text))
}
fn read_legacy(path: &Path) -> anyhow::Result<BTreeSet<String>> {
    let Some(text) = read_text(path)? else {
        return Ok(BTreeSet::new());
    };
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some("version: 1"),
        "unsupported duplicate review format"
    );
    let count: usize = lines
        .next()
        .and_then(|s| s.strip_prefix("reviews["))
        .and_then(|s| s.strip_suffix("]{key,reason}:"))
        .context("invalid duplicate review table")?
        .parse()?;
    ensure!(count <= MAX_ROWS, "too many duplicate reviews");
    let mut keys = BTreeSet::new();
    for line in lines {
        let (key, reason) = line
            .strip_prefix("  ")
            .and_then(|s| s.split_once(','))
            .context("invalid duplicate review row")?;
        ensure!(
            valid_key(key) && matches!(reason, "false-positive" | "dismissed"),
            "invalid duplicate review row"
        );
        ensure!(keys.insert(key.to_string()), "duplicate review key");
    }
    ensure!(keys.len() == count, "duplicate review count mismatch");
    Ok(keys)
}
/// Current decisions plus the newest history fingerprint of each dead identity.
type ToonState = (
    BTreeMap<(String, String), Decision>,
    BTreeMap<String, String>,
);

fn read_toon(path: &Path) -> anyhow::Result<ToonState> {
    let Some(text) = read_text(path)? else {
        return Ok(Default::default());
    };
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some("version: 2"),
        "unsupported review format"
    );
    let mut decisions = BTreeMap::new();
    let mut history = BTreeMap::new();
    for (table, limit) in [("reviews", MAX_ROWS), ("history", 50_000)] {
        let count: usize = lines
            .next()
            .and_then(|s| s.strip_prefix(&format!("{table}[")))
            .and_then(|s| s.strip_suffix(&format!("]{HEADER}")))
            .context("invalid review table")?
            .parse()?;
        ensure!(count <= limit, "too many review rows");
        for _ in 0..count {
            let line = lines
                .next()
                .and_then(|s| s.strip_prefix("  "))
                .context("missing review row")?;
            let fields: Vec<_> = line.splitn(6, ',').collect();
            ensure!(fields.len() == 6, "short review row");
            validate(fields[0], fields[1], fields[2], fields[3])?;
            let _: u64 = fields[4].parse()?;
            ensure!(
                fields[5].len() <= 500 && !fields[5].chars().any(char::is_control),
                "invalid review reason"
            );
            if table == "reviews" {
                ensure!(
                    decisions
                        .insert(
                            (fields[0].into(), fields[1].into()),
                            Decision {
                                fingerprint: fields[2].into(),
                                status: fields[3].into()
                            }
                        )
                        .is_none(),
                    "duplicate review key"
                );
            } else if fields[0] == "dead" {
                history.insert(fields[1].to_string(), fields[2].to_string());
            }
        }
    }
    ensure!(lines.next().is_none(), "unexpected review data");
    Ok((decisions, history))
}

/// Newest history fingerprint of each dead identity, then annotation-only
/// identities, as the dashboard store would list them. `toon_history` is the
/// not-yet-imported legacy history, which an import appends last.
fn retained_annotations(
    transaction: &rusqlite::Transaction<'_>,
    toon_history: BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut tables = transaction.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('review_history', 'comments')",
    )?;
    let tables = tables
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut retained = BTreeMap::new();
    if tables.contains("review_history") {
        let mut query = transaction.prepare(
            "SELECT identity, fingerprint FROM review_history WHERE kind = 'dead' ORDER BY id LIMIT ?1",
        )?;
        let rows = query.query_map([MAX_ROWS as i64 + 1], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for (count, row) in rows.enumerate() {
            ensure!(count < MAX_ROWS, "too much dashboard review history");
            let (identity, fingerprint) = row?;
            ensure!(
                valid_key(&identity) && valid_key(&fingerprint),
                "invalid review history row"
            );
            retained.insert(identity, fingerprint);
        }
    }
    retained.extend(toon_history);
    if tables.contains("comments") {
        let mut query = transaction
            .prepare("SELECT DISTINCT identity FROM comments WHERE kind = 'dead' LIMIT ?1")?;
        let rows = query.query_map([MAX_ROWS as i64 + 1], |row| row.get::<_, String>(0))?;
        for (count, row) in rows.enumerate() {
            ensure!(count < MAX_ROWS, "too many dashboard annotations");
            let identity = row?;
            ensure!(valid_key(&identity), "invalid annotation identity");
            retained
                .entry(identity)
                .or_insert_with(|| NO_FINGERPRINT.to_string());
        }
    }
    Ok(retained)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Project(PathBuf);
    impl Project {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mct-review-state-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn db(&self) -> Connection {
            fs::create_dir_all(self.0.join(".mct-index")).unwrap();
            let conn = Connection::open(self.0.join(".mct-index/dashboard.sqlite3")).unwrap();
            conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE reviews(kind TEXT,identity TEXT,fingerprint TEXT,status TEXT);").unwrap();
            conn
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn id(c: char) -> String {
        c.to_string().repeat(64)
    }
    fn row(conn: &Connection, kind: &str, key: &str, fp: &str, status: &str) {
        conn.execute(
            "INSERT INTO reviews VALUES (?1,?2,?3,?4)",
            rusqlite::params![kind, key, fp, status],
        )
        .unwrap();
    }
    fn entry(line: u32) -> SymbolListEntry {
        SymbolListEntry {
            name: "twin".into(),
            kind: "function".into(),
            language: "python".into(),
            relative_path: "a.py".into(),
            line,
            end_line: Some(line + 1),
            parent: None,
            level: None,
        }
    }
    fn navigation(text: &str) -> NavigationSnapshot {
        NavigationSnapshot {
            files: vec![crate::IndexedFile {
                relative_path: "a.py".into(),
                language: "python".into(),
                content_hash: git2::Oid::hash_object(git2::ObjectType::Blob, text.as_bytes())
                    .unwrap()
                    .to_string(),
            }],
            symbols: Vec::new(),
            imports: Vec::new(),
        }
    }

    #[test]
    fn missing_state_does_not_create_a_database_or_directory() {
        let project = Project::new();
        assert!(!ReviewSnapshot::load(&project.0).unwrap().has_decisions());
        assert!(!project.0.join(".mct-index").exists());
    }

    #[test]
    fn only_verified_discarded_states_are_suppressed_and_reads_do_not_write() {
        let project = Project::new();
        let conn = project.db();
        for (key, status) in [
            ('a', "false_positive"),
            ('b', "ignored"),
            ('c', "confirmed"),
            ('d', "pending"),
        ] {
            row(&conn, "dead", &id(key), &id('1'), status);
        }
        drop(conn);
        let path = project.0.join(".mct-index/dashboard.sqlite3");
        let before = fs::read(&path).unwrap();
        let snapshot = ReviewSnapshot::load(&project.0).unwrap();
        for key in ['a', 'b'] {
            assert!(snapshot.suppresses("dead", &id(key), &id('1'), &id('2')));
            assert!(
                snapshot.suppresses("dead", &id(key), &id('2'), &id('1')),
                "matching legacy fingerprint is preserved conservatively"
            );
            assert!(!snapshot.suppresses("dead", &id(key), &id('3'), &id('4')));
            assert!(
                !snapshot.suppresses("dead", &id(key), "", &id('1')),
                "unreadable source never suppresses"
            );
        }
        for key in ['c', 'd'] {
            assert!(!snapshot.suppresses("dead", &id(key), &id('1'), &id('2')));
        }
        assert_eq!(fs::read(path).unwrap(), before);
    }

    #[test]
    fn deleted_identity_blocks_both_current_and_legacy_fallback() {
        let project = Project::new();
        let conn = project.db();
        row(&conn, "duplicate", &id('a'), &id('1'), "ignored");
        conn.execute(
            "INSERT INTO meta VALUES (?1,'1')",
            [format!("review_deleted:duplicate:{}", id('a'))],
        )
        .unwrap();
        fs::write(
            project.0.join(".mct-index/duplicate-reviews.toon"),
            format!(
                "version: 1\nreviews[1]{{key,reason}}:\n  {},dismissed\n",
                id('2')
            ),
        )
        .unwrap();
        let snapshot = ReviewSnapshot::load(&project.0).unwrap();
        assert!(!snapshot.suppresses("duplicate", &id('a'), &id('1'), &id('2')));
        assert!(snapshot.suppresses("duplicate", &id('b'), &id('1'), &id('2')));
    }

    #[test]
    fn toon_before_import_is_used_but_imported_files_cannot_override_sqlite() {
        let project = Project::new();
        let conn = project.db();
        let file = project.0.join(".mct-index/reviews.toon");
        fs::write(&file,format!("version: 2\nreviews[1]{HEADER}\n  dead,{},{},ignored,7,reason\nhistory[0]{HEADER}\n",id('a'),id('1'))).unwrap();
        assert!(ReviewSnapshot::load(&project.0).unwrap().suppresses(
            "dead",
            &id('a'),
            &id('1'),
            &id('2')
        ));
        conn.execute("INSERT INTO meta VALUES ('reviews_toon_imported','1')", [])
            .unwrap();
        fs::write(file, "obsolete invalid contents").unwrap();
        assert!(!ReviewSnapshot::load(&project.0).unwrap().suppresses(
            "dead",
            &id('a'),
            &id('1'),
            &id('2')
        ));
    }

    #[test]
    fn invalid_or_unsafe_state_is_an_error_not_a_suppression() {
        let project = Project::new();
        let conn = project.db();
        row(&conn, "dead", &id('a'), &id('1'), "typo");
        assert!(ReviewSnapshot::load(&project.0).is_err());
        drop(conn);
        #[cfg(unix)]
        {
            let db = project.0.join(".mct-index/dashboard.sqlite3");
            let target = project.0.join("outside.sqlite3");
            fs::rename(&db, &target).unwrap();
            std::os::unix::fs::symlink(&target, &db).unwrap();
            assert!(ReviewSnapshot::load(&project.0).is_err());
        }
    }

    #[test]
    fn retained_pending_quota_does_not_hide_an_active_decision_or_reenable_legacy() {
        let project = Project::new();
        let mut conn = project.db();
        let text = "def twin():\n    return 2\n";
        fs::write(project.0.join("a.py"), text).unwrap();
        let nav = navigation(text);
        let candidate = entry(1);
        let (active_id, active_fp) = Fingerprints::new(&project.0, &nav)
            .dead_keys(std::slice::from_ref(&candidate), &nav)
            .remove(0);
        let transaction = conn.transaction().unwrap();
        {
            let mut insert = transaction
                .prepare("INSERT INTO reviews VALUES ('duplicate',?1,?2,'pending')")
                .unwrap();
            for n in 0..MAX_ROWS {
                insert
                    .execute(rusqlite::params![format!("{n:064x}"), id('1')])
                    .unwrap();
            }
        }
        row(
            &transaction,
            "dead",
            &active_id,
            &active_fp,
            "false_positive",
        );
        transaction.commit().unwrap();
        fs::write(
            project.0.join(".mct-index/duplicate-reviews.toon"),
            format!(
                "version: 1\nreviews[1]{{key,reason}}:\n  {},dismissed\n",
                id('2')
            ),
        )
        .unwrap();
        let snapshot = ReviewSnapshot::load(&project.0).unwrap();
        assert!(snapshot
            .filter_dead(&project.0, &nav, vec![candidate])
            .is_empty());
        assert!(
            !snapshot.suppresses("duplicate", &format!("{:064x}", 42), &id('1'), &id('2')),
            "retained Pending rows still block old legacy decisions"
        );
        drop(snapshot);
        row(
            &conn,
            "duplicate",
            &format!("{MAX_ROWS:064x}"),
            &id('1'),
            "pending",
        );
        let diagnostic = ReviewSnapshot::load(&project.0).err().unwrap().to_string();
        assert!(diagnostic.contains("retained pending"), "{diagnostic}");
    }

    #[test]
    fn dead_bindings_preserve_a_surviving_twin_but_ambiguity_and_stale_source_stay_visible() {
        let project = Project::new();
        let conn = project.db();
        let text = "def twin():\n    return 2\n";
        fs::write(project.0.join("a.py"), text).unwrap();
        let snapshot_nav = navigation(text);
        let current = entry(1);
        let fp = Fingerprints::new(&project.0, &snapshot_nav)
            .dead_keys(std::slice::from_ref(&current), &snapshot_nav)
            .remove(0)
            .1;
        let old_id = review_keys::dead_identity_at(&current, 1);
        row(&conn, "dead", &old_id, &fp, "false_positive");
        for (prefix, value) in [
            ("dead_base", review_keys::dead_base_identity(&current)),
            ("dead_fingerprint", fp.clone()),
        ] {
            conn.execute(
                "INSERT INTO meta VALUES (?1,?2)",
                rusqlite::params![format!("{prefix}:{old_id}"), value],
            )
            .unwrap();
        }
        let snapshot = ReviewSnapshot::load(&project.0).unwrap();
        assert!(snapshot
            .filter_dead(&project.0, &snapshot_nav, vec![current.clone()])
            .is_empty());
        fs::write(project.0.join("a.py"), "def twin():\n    return 9\n").unwrap();
        assert_eq!(
            snapshot
                .filter_dead(&project.0, &snapshot_nav, vec![current.clone()])
                .len(),
            1
        );
        let repeated = format!("{text}{text}");
        fs::write(project.0.join("a.py"), &repeated).unwrap();
        let repeated_nav = navigation(&repeated);
        assert_eq!(
            snapshot
                .filter_dead(&project.0, &repeated_nav, vec![current, entry(3)])
                .len(),
            2
        );
    }

    #[test]
    fn retained_history_follows_the_store_archive_rule_and_makes_matches_ambiguous() {
        let project = Project::new();
        let conn = project.db();
        conn.execute_batch(
            "CREATE TABLE review_history(id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT,identity TEXT,fingerprint TEXT);
             CREATE TABLE comments(id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT,identity TEXT);",
        )
        .unwrap();
        let text = "def twin():\n    return 2\n";
        fs::write(project.0.join("a.py"), text).unwrap();
        let nav = navigation(text);
        let current = entry(1);
        let fp = Fingerprints::new(&project.0, &nav)
            .dead_keys(std::slice::from_ref(&current), &nav)
            .remove(0)
            .1;
        let (reviewed, older) = (
            review_keys::dead_identity_at(&current, 0),
            review_keys::dead_identity_at(&current, 1),
        );
        row(&conn, "dead", &reviewed, &fp, "false_positive");
        conn.execute(
            "INSERT INTO review_history(kind,identity,fingerprint) VALUES ('dead',?1,?2)",
            rusqlite::params![older, fp],
        )
        .unwrap();
        // An annotation-only identity carries no content and changes nothing.
        conn.execute(
            "INSERT INTO comments(kind,identity) VALUES ('dead',?1)",
            [review_keys::dead_identity_at(&current, 2)],
        )
        .unwrap();
        let set = |key: &str, value: Option<&str>| {
            let key = format!("{key}:dead:{older}");
            conn.execute("DELETE FROM meta WHERE key = ?1", [&key])
                .unwrap();
            if let Some(value) = value {
                conn.execute(
                    "INSERT INTO meta VALUES (?1,?2)",
                    rusqlite::params![key, value],
                )
                .unwrap();
            }
        };
        let visible = || {
            ReviewSnapshot::load(&project.0)
                .unwrap()
                .filter_dead(&project.0, &nav, vec![current.clone()])
                .len()
        };
        assert_eq!(visible(), 1, "a retained twin with the same content");
        set("review_deleted", Some("1"));
        assert_eq!(visible(), 0, "deleted and never reopened");
        set("review_archived", Some("1"));
        assert_eq!(visible(), 0, "explicitly archived");
        set("review_archived", Some("0"));
        assert_eq!(visible(), 1, "reopened by a later annotation");
        set("review_archived", Some("2"));
        assert!(ReviewSnapshot::load(&project.0).is_err());
    }
}
