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

use crate::review_keys::{self, Fingerprints};
use crate::{DuplicateGroup, NavigationSnapshot, Normalization, SymbolListEntry};

const MAX_ROWS: usize = 100_000;
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const HEADER: &str = "{kind,identity,fingerprint,status,at,reason}:";

#[derive(Clone, Debug)]
struct Decision {
    fingerprint: String,
    status: String,
}

/// A consistent read of the saved decisions, not a second analysis index.
#[derive(Default)]
pub struct ReviewSnapshot {
    decisions: BTreeMap<(String, String), Decision>,
    legacy_duplicates: BTreeSet<String>,
    deleted: BTreeSet<(String, String)>,
    dead_aliases: BTreeMap<String, (String, String)>,
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
            state.decisions = read_toon(&dir.join("reviews.toon"))?;
            return Ok(state);
        }
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let transaction = conn.transaction()?;
        let imported: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key = 'reviews_toon_imported')",
            [],
            |row| row.get(0),
        )?;
        if !imported {
            state.decisions = read_toon(&dir.join("reviews.toon"))?;
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
            "SELECT key, value FROM meta WHERE key LIKE 'review_deleted:%' OR key LIKE 'dead_base:%' OR key LIKE 'dead_fingerprint:%' LIMIT 600001",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut bases = BTreeMap::new();
        let mut fingerprints = BTreeMap::new();
        for (count, row) in rows.enumerate() {
            ensure!(count < MAX_ROWS * 6, "too much dashboard identity metadata");
            let (key, value) = row?;
            if let Some(key) = key.strip_prefix("review_deleted:") {
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
        transaction.commit()?;
        Ok(state)
    }

    /// Whether rendered report caches must be bypassed to resolve live decisions.
    pub fn has_decisions(&self) -> bool {
        !self.decisions.is_empty() || !self.legacy_duplicates.is_empty() || !self.deleted.is_empty()
    }

    /// Saved dead-symbol origin bindings, for identity-preserving callers.
    pub fn dead_aliases(&self) -> &BTreeMap<String, (String, String)> {
        &self.dead_aliases
    }

    /// Find one uniquely matching saved dead identity. The caller must also
    /// establish that only one current candidate has this (base, fingerprint).
    /// `None` signals ambiguous saved evidence and must never suppress a hit.
    pub fn resolve_dead_identity(
        &self,
        base: &str,
        default: &str,
        fingerprint: &str,
    ) -> Option<String> {
        let mut matching = self
            .dead_aliases
            .iter()
            .filter(|(identity, (saved_base, saved_fp))| {
                saved_base == base
                    && saved_fp == fingerprint
                    && self
                        .decisions
                        .contains_key(&("dead".into(), (*identity).clone()))
                    && !self.deleted.contains(&("dead".into(), (*identity).clone()))
            });
        match (matching.next(), matching.next()) {
            (Some((identity, _)), None) => Some(identity.clone()),
            (None, _) => Some(default.to_string()),
            _ => None,
        }
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
        let keys = Fingerprints::new(root, navigation).dead_keys(&entries, navigation);
        let mut counts = BTreeMap::new();
        for (entry, (_, fingerprint)) in entries.iter().zip(&keys) {
            *counts
                .entry((review_keys::dead_base_identity(entry), fingerprint.clone()))
                .or_insert(0usize) += 1;
        }
        entries
            .into_iter()
            .zip(keys)
            .zip(legacy)
            .filter_map(|((entry, (identity, fingerprint)), (_, old))| {
                let base = review_keys::dead_base_identity(&entry);
                if counts
                    .get(&(base.clone(), fingerprint.clone()))
                    .copied()
                    .unwrap_or(0)
                    != 1
                {
                    return Some(entry);
                }
                let Some(identity) = self.resolve_dead_identity(&base, &identity, &fingerprint)
                else {
                    return Some(entry);
                };
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
        let ids = review_keys::group_identities(groups);
        let legacy = review_keys::legacy_group_keys(groups, navigation, normalization)?;
        let fingerprints = Fingerprints::new(root, navigation).group_keys(groups, normalization);
        let mut counts = BTreeMap::new();
        for id in &ids {
            *counts.entry(id).or_insert(0usize) += 1;
        }
        Ok(ids
            .iter()
            .zip(fingerprints)
            .zip(legacy)
            .map(|((identity, fingerprint), old)| {
                counts.get(identity).copied().unwrap_or(0) != 1
                    || !self.suppresses("duplicate", identity, &fingerprint, &old)
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
fn read_toon(path: &Path) -> anyhow::Result<BTreeMap<(String, String), Decision>> {
    let Some(text) = read_text(path)? else {
        return Ok(BTreeMap::new());
    };
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some("version: 2"),
        "unsupported review format"
    );
    let mut decisions = BTreeMap::new();
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
            }
        }
    }
    ensure!(lines.next().is_none(), "unexpected review data");
    Ok(decisions)
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
}
