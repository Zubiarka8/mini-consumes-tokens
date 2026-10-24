//! Dashboard user state in `.mct-index/dashboard.sqlite3` (ADR-019): review
//! decisions, their history and comments. The derived index is never written.
//!
//! Rows are keyed by the stable (kind, identity) pair and carry the fingerprint
//! seen at decision time, so reindexing cannot orphan them. Every write is one
//! short `BEGIN IMMEDIATE` transaction, so several processes can write at once.
// Comment writes are only used by the write API (milestone M4 of ADR-019).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::Path;
use std::time::Duration;

use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use rusqlite_migration::{Migrations, M};

use super::reviews::{
    self, clean_reason, valid_key, Kind, LegacyReason, Record, State, Status, MAX_HISTORY,
    MAX_REASON, MAX_REVIEWS,
};

const IMPORT_KEY: &str = "reviews_toon_imported";
const MAX_COMMENT: usize = 2_000;
const MAX_COMMENTS_PER_FINDING: i64 = 500;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA: &str = "
CREATE TABLE meta(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
CREATE TABLE reviews(
    kind TEXT NOT NULL CHECK (kind IN ('duplicate', 'dead')),
    identity TEXT NOT NULL CHECK (length(identity) = 64),
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64),
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'confirmed', 'false_positive', 'ignored')),
    decided_at INTEGER NOT NULL CHECK (decided_at >= 0),
    reason TEXT NOT NULL CHECK (length(CAST(reason AS BLOB)) <= 500),
    PRIMARY KEY (kind, identity)
) STRICT;
CREATE TABLE review_history(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('duplicate', 'dead')),
    identity TEXT NOT NULL CHECK (length(identity) = 64),
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64),
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'confirmed', 'false_positive', 'ignored')),
    decided_at INTEGER NOT NULL CHECK (decided_at >= 0),
    reason TEXT NOT NULL CHECK (length(CAST(reason AS BLOB)) <= 500)
) STRICT;
CREATE INDEX review_history_finding ON review_history(kind, identity, id);
CREATE TABLE comments(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('duplicate', 'dead')),
    identity TEXT NOT NULL CHECK (length(identity) = 64),
    body TEXT NOT NULL CHECK (length(CAST(body AS BLOB)) BETWEEN 1 AND 2000),
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    updated_at INTEGER NOT NULL CHECK (updated_at >= 0)
) STRICT;
CREATE INDEX comments_finding ON comments(kind, identity, id);
";

/// Version 2 (ADR-019, per-file review): a working label and a note for one
/// member file of a duplicate group, keyed by the group's stable identity.
const SCHEMA_FILE_REVIEWS: &str = "
CREATE TABLE file_reviews(
    identity TEXT NOT NULL CHECK (length(identity) = 64),
    path TEXT NOT NULL CHECK (length(CAST(path AS BLOB)) BETWEEN 1 AND 1024),
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'accepted', 'rejected', 'in_review', 'ignored')),
    note TEXT NOT NULL CHECK (length(CAST(note AS BLOB)) <= 2000),
    updated_at INTEGER NOT NULL CHECK (updated_at >= 0),
    PRIMARY KEY (identity, path)
) STRICT;
";

const MAX_FILE_REVIEWS: i64 = 20_000;

/// Review state of one file inside a duplicate group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileStatus {
    Pending,
    Accepted,
    Rejected,
    InReview,
    Ignored,
}

impl FileStatus {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            "in_review" => Self::InReview,
            "ignored" => Self::Ignored,
            _ => return None,
        })
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::InReview => "in_review",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileReview {
    pub path: String,
    pub status: FileStatus,
    pub note: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Comment {
    pub id: i64,
    pub kind: Kind,
    pub identity: String,
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
}

pub(super) struct Store {
    conn: Connection,
    /// ADR-015 decisions, read once and never rewritten.
    legacy: BTreeMap<String, LegacyReason>,
    /// Commits made through this connection, which `data_version` ignores.
    writes: u64,
    legacy_aliases: BTreeMap<String, String>,
}

type RawRecord = (String, String, String, String, i64, String);

/// Another editor changed this entity after the caller loaded its revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Conflict {
    pub expected: u64,
    pub actual: u64,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            output,
            "review conflict: expected revision {}, found {}",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for Conflict {}

fn review_revision_key(kind: Kind, identity: &str) -> String {
    format!("review_revision:{}:{identity}", kind.value())
}

fn file_revision_key(identity: &str, path: &str) -> String {
    format!("file_revision:{identity}:{path}")
}

fn deleted_key(kind: Kind, identity: &str) -> String {
    format!("review_deleted:{}:{identity}", kind.value())
}

fn archived_key(kind: Kind, identity: &str) -> String {
    format!("review_archived:{}:{identity}", kind.value())
}

fn revision(conn: &Connection, key: &str) -> anyhow::Result<u64> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    value.map_or(Ok(0), |value| {
        value.parse().context("invalid stored review revision")
    })
}

fn check_revision(conn: &Connection, key: &str, expected: Option<u64>) -> anyhow::Result<()> {
    if let Some(expected) = expected {
        let actual = revision(conn, key)?;
        if actual != expected {
            return Err(Conflict { expected, actual }.into());
        }
    }
    Ok(())
}

fn advance_revision(conn: &Connection, key: &str) -> anyhow::Result<()> {
    let next = revision(conn, key)?
        .checked_add(1)
        .context("review revision exhausted")?;
    conn.execute("INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, next.to_string()])?;
    Ok(())
}

fn is_deleted(conn: &Connection, kind: Kind, identity: &str) -> anyhow::Result<bool> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [deleted_key(kind, identity)],
            |row| row.get(0),
        )
        .optional()?;
    match value.as_deref() {
        None => Ok(false),
        Some("1") => Ok(true),
        Some(_) => anyhow::bail!("invalid stored review deletion marker"),
    }
}

fn is_archived(conn: &Connection, kind: Kind, identity: &str) -> anyhow::Result<bool> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [archived_key(kind, identity)],
            |row| row.get(0),
        )
        .optional()?;
    match value.as_deref() {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        None => is_deleted(conn, kind, identity),
        Some(_) => anyhow::bail!("invalid stored review archive marker"),
    }
}

/// New annotations make retained state discoverable again without reinstating
/// a legacy verdict that was explicitly deleted.
fn annotation_changed(conn: &Connection, kind: Kind, identity: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, '0') ON CONFLICT(key) DO UPDATE SET value = '0'",
        [archived_key(kind, identity)],
    )?;
    advance_revision(conn, &review_revision_key(kind, identity))
}

fn contains_stored(conn: &Connection, kind: Kind, identity: &str) -> anyhow::Result<bool> {
    if is_archived(conn, kind, identity)? {
        return Ok(false);
    }
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reviews WHERE kind = ?1 AND identity = ?2)
             OR EXISTS(SELECT 1 FROM review_history WHERE kind = ?1 AND identity = ?2)
             OR EXISTS(SELECT 1 FROM comments WHERE kind = ?1 AND identity = ?2)
             OR (?1 = 'duplicate' AND EXISTS(SELECT 1 FROM file_reviews WHERE identity = ?2))",
        params![kind.value(), identity],
        |row| row.get(0),
    )?)
}

impl Store {
    /// Opens (creating it if needed) `.mct-index/dashboard.sqlite3` under `root`,
    /// migrates it and imports `reviews.toon` once. `.mct-index` must exist.
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        let dir = root.join(".mct-index");
        let path = dir.join("dashboard.sqlite3");
        reviews::safe_path(&path)?;
        ensure!(dir.is_dir(), "the .mct-index directory does not exist");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .context("cannot create the dashboard database")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        drop(file);
        let mut conn = Connection::open(&path).context("cannot open the dashboard database")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        import_toon(&mut conn, &dir.join("reviews.toon"))?;
        let legacy = reviews::read_legacy(&dir.join("duplicate-reviews.toon"))?;
        Ok(Self {
            conn,
            legacy,
            writes: 0,
            legacy_aliases: BTreeMap::new(),
        })
    }

    /// Upgrade only rows whose old fingerprint still proves the exact indexed
    /// revision. Identity alone cannot establish whether the code changed.
    pub fn migrate_fingerprints(
        &mut self,
        keys: &[(Kind, String, String, String)],
    ) -> anyhow::Result<()> {
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut changed = 0;
        for (kind, identity, old, new) in keys {
            if !valid_key(new) {
                continue;
            }
            changed += transaction.execute(
                "UPDATE reviews SET fingerprint = ?1, fingerprint_version = 2
                 WHERE kind = ?2 AND identity = ?3 AND fingerprint = ?4 AND fingerprint_version = 1",
                params![new, kind.value(), identity, old],
            )?;
            if *kind == Kind::Duplicate {
                self.legacy_aliases.insert(new.clone(), old.clone());
            }
        }
        transaction.commit()?;
        if changed > 0 {
            self.writes += 1;
        }
        Ok(())
    }

    /// Retains the provenance of a reviewed dead symbol for conservative
    /// identity rebinding after same-name siblings move or disappear. A stale
    /// analysis snapshot cannot rewrite bindings for a newer decision.
    pub fn remember_dead_bindings(
        &mut self,
        bindings: &[(String, String, String)],
    ) -> anyhow::Result<()> {
        for (identity, base, fingerprint) in bindings {
            ensure!(
                valid_key(identity) && valid_key(base) && valid_key(fingerprint),
                "invalid dead binding key"
            );
        }
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut changed = 0;
        for (identity, base, fingerprint) in bindings {
            let matches: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM reviews WHERE kind = 'dead' AND identity = ?1 AND fingerprint = ?2)",
                params![identity, fingerprint], |row| row.get(0),
            )?;
            if !matches {
                continue;
            }
            for (prefix, value) in [("dead_base", base), ("dead_fingerprint", fingerprint)] {
                changed += transaction.execute(
                    "INSERT INTO meta(key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE meta.value <> excluded.value",
                    params![format!("{prefix}:{identity}"), value],
                )?;
            }
        }
        transaction.commit()?;
        if changed > 0 {
            self.writes += 1;
        }
        Ok(())
    }

    /// Monotonic revisions survive delete/recreate, preventing an ABA match.
    pub fn decision_revision(&self, kind: Kind, identity: &str) -> anyhow::Result<u64> {
        ensure!(valid_key(identity), "invalid review key");
        revision(&self.conn, &review_revision_key(kind, identity))
    }

    pub fn file_revision(&self, identity: &str, path: &str) -> anyhow::Result<u64> {
        ensure!(valid_key(identity), "invalid review key");
        revision(&self.conn, &file_revision_key(identity, path))
    }

    /// Explicitly archives orphan state. History and annotations remain readable,
    /// but a tombstone keeps the archived row out of orphan lists and suppresses
    /// ADR-015 fallback when the original finding reappears.
    pub fn delete_decision(&mut self, kind: Kind, identity: &str) -> anyhow::Result<bool> {
        self.delete_decision_inner(kind, identity, None)
    }

    pub fn delete_decision_checked(
        &mut self,
        kind: Kind,
        identity: &str,
        expected_revision: u64,
    ) -> anyhow::Result<bool> {
        self.delete_decision_inner(kind, identity, Some(expected_revision))
    }

    fn delete_decision_inner(
        &mut self,
        kind: Kind,
        identity: &str,
        expected_revision: Option<u64>,
    ) -> anyhow::Result<bool> {
        ensure!(valid_key(identity), "invalid review key");
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let key = review_revision_key(kind, identity);
        check_revision(&transaction, &key, expected_revision)?;
        if !contains_stored(&transaction, kind, identity)? {
            return Ok(false);
        }
        transaction.execute(
            "DELETE FROM reviews WHERE kind = ?1 AND identity = ?2",
            params![kind.value(), identity],
        )?;
        transaction.execute("INSERT INTO meta(key, value) VALUES (?1, '1') ON CONFLICT(key) DO UPDATE SET value = '1'", [deleted_key(kind, identity)])?;
        transaction.execute("INSERT INTO meta(key, value) VALUES (?1, '1') ON CONFLICT(key) DO UPDATE SET value = '1'", [archived_key(kind, identity)])?;
        advance_revision(&transaction, &key)?;
        transaction.commit()?;
        self.writes += 1;
        Ok(true)
    }

    /// Includes restored decisions and annotation/history/file-only identities,
    /// so an absent finding remains inspectable and can be archived explicitly.
    pub fn stored_findings(&self) -> anyhow::Result<Vec<Record>> {
        let mut statement = self.conn.prepare(
            "WITH findings(kind, identity) AS (
                SELECT kind, identity FROM reviews
                UNION SELECT kind, identity FROM review_history
                UNION SELECT kind, identity FROM comments
                UNION SELECT 'duplicate', identity FROM file_reviews
             )
             SELECT f.kind, f.identity,
                COALESCE(r.fingerprint,
                    (SELECT h.fingerprint FROM review_history h WHERE h.kind = f.kind AND h.identity = f.identity ORDER BY h.id DESC LIMIT 1), ?1),
                COALESCE(r.status, 'pending'),
                COALESCE(r.decided_at, MAX(
                    COALESCE((SELECT MAX(h.decided_at) FROM review_history h WHERE h.kind = f.kind AND h.identity = f.identity), 0),
                    COALESCE((SELECT MAX(c.updated_at) FROM comments c WHERE c.kind = f.kind AND c.identity = f.identity), 0),
                    COALESCE((SELECT MAX(v.updated_at) FROM file_reviews v WHERE f.kind = 'duplicate' AND v.identity = f.identity), 0))),
                COALESCE(r.reason, '')
             FROM findings f LEFT JOIN reviews r ON r.kind = f.kind AND r.identity = f.identity
             WHERE NOT EXISTS(SELECT 1 FROM meta m WHERE m.key = 'review_archived:' || f.kind || ':' || f.identity AND m.value = '1')
               AND (EXISTS(SELECT 1 FROM meta m WHERE m.key = 'review_archived:' || f.kind || ':' || f.identity AND m.value = '0')
                    OR NOT EXISTS(SELECT 1 FROM meta m WHERE m.key = 'review_deleted:' || f.kind || ':' || f.identity AND m.value = '1'))
             ORDER BY 5 DESC, f.kind, f.identity",
        )?;
        let rows = statement
            .query_map(["0".repeat(64)], raw)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(record).collect()
    }

    /// Tests for retained state even when there is no current analysis finding.
    /// Explicitly archived findings are excluded from normal orphan navigation.
    pub fn contains_stored_finding(&self, kind: Kind, identity: &str) -> anyhow::Result<bool> {
        ensure!(valid_key(identity), "invalid review key");
        contains_stored(&self.conn, kind, identity)
    }

    pub fn state(&self, kind: Kind, identity: &str, fingerprint: &str) -> anyhow::Result<State> {
        let row = self
            .conn
            .prepare_cached(
                "SELECT fingerprint, status, decided_at, reason FROM reviews
                 WHERE kind = ?1 AND identity = ?2",
            )?
            .query_row(params![kind.value(), identity], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(match row {
            Some((stored, status, at, reason)) => {
                let status = Status::parse(&status).context("invalid stored status")?;
                let at = u64::try_from(at)?;
                if status == Status::Pending {
                    State::Pending
                } else if stored == fingerprint {
                    State::Decided {
                        status,
                        at: Some(at),
                        reason,
                    }
                } else {
                    State::NeedsReevaluation { status, at, reason }
                }
            }
            None if is_deleted(&self.conn, kind, identity)? => State::Pending,
            None => match (
                kind,
                self.legacy.get(
                    self.legacy_aliases
                        .get(fingerprint)
                        .map_or(fingerprint, String::as_str),
                ),
            ) {
                (Kind::Duplicate, Some(reason)) => State::Decided {
                    status: reason.status(),
                    at: None,
                    reason: String::new(),
                },
                _ => State::Pending,
            },
        })
    }

    /// Every stored decision (restored findings excluded), newest first.
    pub fn decisions(&self) -> anyhow::Result<Vec<Record>> {
        let mut statement = self.conn.prepare(
            "SELECT kind, identity, fingerprint, status, decided_at, reason FROM reviews
             WHERE status <> 'pending' ORDER BY decided_at DESC, identity",
        )?;
        let rows = statement
            .query_map([], raw)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(record).collect()
    }

    /// History of one finding, newest first.
    pub fn history(&self, kind: Kind, identity: &str) -> anyhow::Result<Vec<Record>> {
        let mut statement = self.conn.prepare(
            "SELECT kind, identity, fingerprint, status, decided_at, reason FROM review_history
             WHERE kind = ?1 AND identity = ?2 ORDER BY id DESC",
        )?;
        let rows = statement
            .query_map(params![kind.value(), identity], raw)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(record).collect()
    }

    pub fn set(
        &mut self,
        record_of: (Kind, &str, &str),
        status: Status,
        reason: &str,
        now: u64,
    ) -> anyhow::Result<()> {
        self.set_inner(record_of, status, reason, now, None)
    }

    pub fn set_checked(
        &mut self,
        record_of: (Kind, &str, &str),
        status: Status,
        reason: &str,
        now: u64,
        expected_revision: u64,
    ) -> anyhow::Result<()> {
        self.set_inner(record_of, status, reason, now, Some(expected_revision))
    }

    fn set_inner(
        &mut self,
        record_of: (Kind, &str, &str),
        status: Status,
        reason: &str,
        now: u64,
        expected_revision: Option<u64>,
    ) -> anyhow::Result<()> {
        let (kind, identity, fingerprint) = record_of;
        ensure!(
            valid_key(identity) && valid_key(fingerprint),
            "invalid review key"
        );
        let reason = if status == Status::Pending {
            String::new()
        } else {
            clean_reason(reason)
        };
        let now = i64::try_from(now)?;
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision_key = review_revision_key(kind, identity);
        check_revision(&transaction, &revision_key, expected_revision)?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM reviews WHERE kind = ?1 AND identity = ?2)",
            params![kind.value(), identity],
            |row| row.get(0),
        )?;
        // Restoring a finding that never had a decision writes nothing.
        if status == Status::Pending
            && !exists
            && !self.legacy.contains_key(
                self.legacy_aliases
                    .get(fingerprint)
                    .map_or(fingerprint, String::as_str),
            )
        {
            return Ok(());
        }
        if status != Status::Pending {
            let active: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM reviews WHERE kind = ?1 AND identity = ?2 AND status <> 'pending')",
                params![kind.value(), identity], |row| row.get(0),
            )?;
            if !active {
                let total: i64 = transaction.query_row(
                    "SELECT count(*) FROM reviews WHERE status <> 'pending'",
                    [],
                    |row| row.get(0),
                )?;
                ensure!(total < MAX_REVIEWS as i64, "too many reviews");
            }
        }
        let values = params![
            kind.value(),
            identity,
            fingerprint,
            status.value(),
            now,
            reason
        ];
        transaction.execute(
            "INSERT INTO reviews(kind, identity, fingerprint, status, decided_at, reason, fingerprint_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 2)
             ON CONFLICT(kind, identity) DO UPDATE SET
                fingerprint = excluded.fingerprint, status = excluded.status,
                decided_at = excluded.decided_at, reason = excluded.reason, fingerprint_version = 2",
            values,
        )?;
        transaction.execute(
            "INSERT INTO review_history(kind, identity, fingerprint, status, decided_at, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            values,
        )?;
        trim_history(&transaction, MAX_HISTORY)?;
        transaction.execute(
            "DELETE FROM meta WHERE key = ?1",
            [deleted_key(kind, identity)],
        )?;
        transaction.execute(
            "DELETE FROM meta WHERE key = ?1",
            [archived_key(kind, identity)],
        )?;
        advance_revision(&transaction, &revision_key)?;
        transaction.commit()?;
        self.writes += 1;
        Ok(())
    }

    /// Comments of one finding, oldest first.
    pub fn comments(&self, kind: Kind, identity: &str) -> anyhow::Result<Vec<Comment>> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, identity, body, created_at, updated_at FROM comments
             WHERE kind = ?1 AND identity = ?2 ORDER BY id",
        )?;
        let rows = statement
            .query_map(params![kind.value(), identity], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, kind, identity, body, created_at, updated_at)| {
                Ok(Comment {
                    id,
                    kind: Kind::parse(&kind).context("invalid stored kind")?,
                    identity,
                    body,
                    created_at: u64::try_from(created_at)?,
                    updated_at: u64::try_from(updated_at)?,
                })
            })
            .collect()
    }

    pub fn add_comment(
        &mut self,
        kind: Kind,
        identity: &str,
        body: &str,
        now: u64,
    ) -> anyhow::Result<i64> {
        ensure!(valid_key(identity), "invalid review key");
        let body = clean_comment(body)?;
        let now = i64::try_from(now)?;
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = transaction.query_row(
            "SELECT count(*) FROM comments WHERE kind = ?1 AND identity = ?2",
            params![kind.value(), identity],
            |row| row.get(0),
        )?;
        ensure!(count < MAX_COMMENTS_PER_FINDING, "too many comments");
        transaction.execute(
            "INSERT INTO comments(kind, identity, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![kind.value(), identity, body, now],
        )?;
        let id = transaction.last_insert_rowid();
        annotation_changed(&transaction, kind, identity)?;
        transaction.commit()?;
        self.writes += 1;
        Ok(id)
    }

    /// Returns `false` when the comment does not exist.
    pub fn update_comment(&mut self, id: i64, body: &str, now: u64) -> anyhow::Result<bool> {
        let body = clean_comment(body)?;
        let now = i64::try_from(now)?;
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owner: Option<(String, String)> = transaction
            .query_row(
                "SELECT kind, identity FROM comments WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let changed = transaction.execute(
            "UPDATE comments SET body = ?1, updated_at = ?2 WHERE id = ?3",
            params![body, now, id],
        )?;
        if let Some((kind, identity)) = owner {
            annotation_changed(
                &transaction,
                Kind::parse(&kind).context("invalid stored kind")?,
                &identity,
            )?;
        }
        transaction.commit()?;
        self.writes += 1;
        Ok(changed == 1)
    }

    /// Returns `false` when the comment does not exist.
    pub fn delete_comment(&mut self, id: i64) -> anyhow::Result<bool> {
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owner: Option<(String, String)> = transaction
            .query_row(
                "SELECT kind, identity FROM comments WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let changed = transaction.execute("DELETE FROM comments WHERE id = ?1", [id])?;
        if let Some((kind, identity)) = owner {
            annotation_changed(
                &transaction,
                Kind::parse(&kind).context("invalid stored kind")?,
                &identity,
            )?;
        }
        transaction.commit()?;
        self.writes += 1;
        Ok(changed == 1)
    }

    /// File reviews of one duplicate group, by path.
    pub fn file_reviews(&self, identity: &str) -> anyhow::Result<Vec<FileReview>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT path, status, note, updated_at FROM file_reviews
             WHERE identity = ?1 ORDER BY path",
        )?;
        let rows = statement
            .query_map([identity], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(path, status, note, at)| {
                Ok(FileReview {
                    path,
                    status: FileStatus::parse(&status).context("invalid stored file status")?,
                    note,
                    updated_at: u64::try_from(at)?,
                })
            })
            .collect()
    }

    /// Sets the state and note of one file. A pending file with no note is the
    /// default, so its row is removed instead of stored.
    pub fn set_file_review(
        &mut self,
        identity: &str,
        path: &str,
        status: FileStatus,
        note: &str,
        now: u64,
    ) -> anyhow::Result<()> {
        self.set_file_review_inner(identity, path, status, note, now, None)
    }

    pub fn set_file_review_checked(
        &mut self,
        identity: &str,
        path: &str,
        status: FileStatus,
        note: &str,
        now: u64,
        expected_revision: u64,
    ) -> anyhow::Result<()> {
        self.set_file_review_inner(identity, path, status, note, now, Some(expected_revision))
    }

    fn set_file_review_inner(
        &mut self,
        identity: &str,
        path: &str,
        status: FileStatus,
        note: &str,
        now: u64,
        expected_revision: Option<u64>,
    ) -> anyhow::Result<()> {
        ensure!(valid_key(identity), "invalid review key");
        ensure!(
            !path.is_empty() && path.len() <= 1024 && !path.chars().any(char::is_control),
            "invalid file path"
        );
        let note = clean_note(note)?;
        let now = i64::try_from(now)?;
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let key = file_revision_key(identity, path);
        check_revision(&transaction, &key, expected_revision)?;
        if status == FileStatus::Pending && note.is_empty() {
            transaction.execute(
                "DELETE FROM file_reviews WHERE identity = ?1 AND path = ?2",
                params![identity, path],
            )?;
        } else {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM file_reviews WHERE identity = ?1 AND path = ?2)",
                params![identity, path],
                |row| row.get(0),
            )?;
            if !exists {
                let total: i64 =
                    transaction
                        .query_row("SELECT count(*) FROM file_reviews", [], |row| row.get(0))?;
                ensure!(total < MAX_FILE_REVIEWS, "too many file reviews");
            }
            transaction.execute(
                "INSERT INTO file_reviews(identity, path, status, note, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(identity, path) DO UPDATE SET
                    status = excluded.status, note = excluded.note,
                    updated_at = excluded.updated_at",
                params![identity, path, status.value(), note, now],
            )?;
        }
        advance_revision(&transaction, &key)?;
        annotation_changed(&transaction, Kind::Duplicate, identity)?;
        transaction.commit()?;
        self.writes += 1;
        Ok(())
    }

    /// Moves on any commit, ours (`writes`) or another process's (`data_version`).
    pub fn change_token(&self) -> anyhow::Result<(i64, u64)> {
        Ok((self.data_version()?, self.writes))
    }

    /// Changes when *another* connection commits (SQLite `data_version`), so a
    /// dashboard can notice writes made by a second process without polling rows.
    pub fn data_version(&self) -> anyhow::Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "data_version", |row| row.get(0))?)
    }
}

/// Two processes opening a fresh database can both see version 0 and race; the
/// loser fails inside its own transaction, so it retries and finds the schema
/// already in place.
fn migrate(conn: &mut Connection) -> anyhow::Result<()> {
    let migrations = Migrations::new(vec![M::up(SCHEMA), M::up(SCHEMA_FILE_REVIEWS), M::up(
        "ALTER TABLE reviews ADD COLUMN fingerprint_version INTEGER NOT NULL DEFAULT 1 CHECK(fingerprint_version IN (1, 2));"
    )]);
    let mut attempts = 0;
    loop {
        match migrations.to_latest(conn) {
            Ok(()) => return Ok(()),
            Err(_) if attempts < 3 => attempts += 1,
            Err(error) => return Err(error).context("cannot migrate the dashboard database"),
        }
    }
}

fn raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRecord> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn record((kind, identity, fingerprint, status, at, reason): RawRecord) -> anyhow::Result<Record> {
    Ok(Record {
        kind: Kind::parse(&kind).context("invalid stored kind")?,
        identity,
        fingerprint,
        status: Status::parse(&status).context("invalid stored status")?,
        at: u64::try_from(at)?,
        reason,
    })
}

/// Keeps the newest `limit` history rows; reports what was dropped.
fn trim_history(transaction: &rusqlite::Transaction<'_>, limit: usize) -> anyhow::Result<()> {
    let dropped = transaction.execute(
        "DELETE FROM review_history WHERE id <= COALESCE(
            (SELECT id FROM review_history ORDER BY id DESC LIMIT 1 OFFSET ?1), 0)",
        [i64::try_from(limit)?],
    )?;
    if dropped > 0 {
        eprintln!("warning: review history is full; dropped the {dropped} oldest rows");
    }
    Ok(())
}

/// Multi-line free text: CRLF becomes LF, other control characters (tabs
/// included) become spaces, and the result must be 1 to 2,000 bytes.
pub(super) fn clean_comment(raw: &str) -> anyhow::Result<String> {
    let text: String = raw
        .replace("\r\n", "\n")
        .chars()
        .map(|ch| {
            if ch != '\n' && ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let text = text.trim();
    ensure!(!text.is_empty(), "empty comment");
    ensure!(text.len() <= MAX_COMMENT, "comment is too long");
    Ok(text.to_string())
}

/// Like [`clean_comment`] but an empty note is allowed (it clears the note).
pub(super) fn clean_note(raw: &str) -> anyhow::Result<String> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    clean_comment(raw)
}

/// Copies `reviews.toon` into the store once. The file is only read: it stays
/// byte-identical so an older build keeps working. A corrupt file aborts.
fn import_toon(conn: &mut Connection, path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let imported = |conn: &Connection| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key = ?1)",
            [IMPORT_KEY],
            |row| row.get(0),
        )
    };
    if imported(conn)? {
        return Ok(());
    }
    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Another process may have imported while this one waited for the lock.
    if imported(&transaction)? {
        return Ok(());
    }
    let (current, history) = reviews::read(path)?;
    ensure!(
        current.len() <= MAX_REVIEWS && history.len() <= MAX_HISTORY,
        "review state is too large"
    );
    for row in &history {
        transaction.execute(
            "INSERT INTO review_history(kind, identity, fingerprint, status, decided_at, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.kind.value(),
                row.identity,
                row.fingerprint,
                row.status.value(),
                i64::try_from(row.at)?,
                row.reason
            ],
        )?;
    }
    for row in current.values() {
        ensure!(row.reason.len() <= MAX_REASON, "invalid review reason");
        transaction.execute(
            "INSERT OR IGNORE INTO reviews(kind, identity, fingerprint, status, decided_at, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.kind.value(),
                row.identity,
                row.fingerprint,
                row.status.value(),
                i64::try_from(row.at)?,
                row.reason
            ],
        )?;
    }
    transaction.execute(
        "INSERT INTO meta(key, value) VALUES (?1, '1')",
        [IMPORT_KEY],
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn project() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".mct-index")).unwrap();
        let path = root.path().canonicalize().unwrap();
        (root, path)
    }

    fn id(character: char) -> String {
        character.to_string().repeat(64)
    }

    const HEADER: &str = "{kind,identity,fingerprint,status,at,reason}:";

    #[test]
    fn decisions_persist_across_reopen_and_restore_keeps_history() {
        let (_root, path) = project();
        let (a, b, fp) = (id('a'), id('b'), id('f'));
        let mut first = Store::open(&path).unwrap();
        first
            .set((Kind::Duplicate, &a, &fp), Status::FalsePositive, "ok", 10)
            .unwrap();
        first
            .set((Kind::Duplicate, &b, &fp), Status::Ignored, "", 11)
            .unwrap();
        drop(first);
        let mut store = Store::open(&path).unwrap();
        assert!(matches!(
            store.state(Kind::Duplicate, &a, &fp).unwrap(),
            State::Decided { status: Status::FalsePositive, at: Some(10), ref reason } if reason == "ok"
        ));
        store
            .set((Kind::Duplicate, &a, &fp), Status::Pending, "ignored", 12)
            .unwrap();
        assert_eq!(
            store.state(Kind::Duplicate, &a, &fp).unwrap(),
            State::Pending
        );
        assert_eq!(store.decisions().unwrap().len(), 1);
        let history: Vec<_> = store
            .history(Kind::Duplicate, &a)
            .unwrap()
            .iter()
            .map(|r| (r.status, r.at, r.reason.clone()))
            .collect();
        assert_eq!(
            history,
            [
                (Status::Pending, 12, String::new()),
                (Status::FalsePositive, 10, "ok".to_string())
            ]
        );
        // A restore of a finding that never had a decision writes nothing.
        store
            .set((Kind::Duplicate, &id('c'), &fp), Status::Pending, "", 13)
            .unwrap();
        assert!(store.history(Kind::Duplicate, &id('c')).unwrap().is_empty());
    }

    #[test]
    fn a_changed_fingerprint_needs_reevaluation_and_kinds_are_separate() {
        let (_root, path) = project();
        let (identity, old, new) = (id('a'), id('1'), id('2'));
        let mut store = Store::open(&path).unwrap();
        store
            .set(
                (Kind::Dead, &identity, &old),
                Status::Confirmed,
                "seguro",
                5,
            )
            .unwrap();
        assert!(matches!(
            store.state(Kind::Dead, &identity, &new).unwrap(),
            State::NeedsReevaluation {
                status: Status::Confirmed,
                at: 5,
                ..
            }
        ));
        assert_eq!(
            store.state(Kind::Duplicate, &identity, &old).unwrap(),
            State::Pending
        );
        store
            .set((Kind::Dead, &identity, &new), Status::Ignored, "api", 6)
            .unwrap();
        assert!(matches!(
            store.state(Kind::Dead, &identity, &new).unwrap(),
            State::Decided { status: Status::Ignored, ref reason, .. } if reason == "api"
        ));
        assert_eq!(store.history(Kind::Dead, &identity).unwrap().len(), 2);
        assert_eq!(store.decisions().unwrap().len(), 1);
    }

    #[test]
    fn invalid_keys_and_reasons_are_rejected_or_cleaned() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        assert!(store
            .set((Kind::Dead, "../x", &id('f')), Status::Ignored, "", 1)
            .is_err());
        assert!(store
            .set((Kind::Dead, &id('a'), "nope"), Status::Ignored, "", 1)
            .is_err());
        store
            .set(
                (Kind::Dead, &id('a'), &id('f')),
                Status::Ignored,
                &format!(" x\ny{} ", "ñ".repeat(400)),
                1,
            )
            .unwrap();
        let reason = store.decisions().unwrap().remove(0).reason;
        assert!(reason.starts_with("x y") && reason.len() <= MAX_REASON);
        // The database itself refuses what the Rust checks would have caught.
        let direct = store.conn.execute(
            "INSERT INTO reviews(kind, identity, fingerprint, status, decided_at, reason) VALUES ('other', ?1, ?1, 'ignored', 1, '')",
            [id('a')],
        );
        assert!(direct.is_err());
    }

    #[test]
    fn the_adr_015_file_is_read_as_decisions_and_never_modified() {
        let (_root, path) = project();
        let legacy = path.join(".mct-index/duplicate-reviews.toon");
        let (a, b, fp) = (id('a'), id('b'), id('f'));
        let content = format!(
            "version: 1\nreviews[2]{{key,reason}}:\n  {a},false-positive\n  {b},dismissed\n"
        );
        fs::write(&legacy, &content).unwrap();
        let mut store = Store::open(&path).unwrap();
        assert!(matches!(
            store.state(Kind::Duplicate, &id('d'), &a).unwrap(),
            State::Decided {
                status: Status::FalsePositive,
                at: None,
                ..
            }
        ));
        assert_eq!(
            store.state(Kind::Dead, &id('d'), &a).unwrap(),
            State::Pending
        );
        store
            .set((Kind::Duplicate, &id('d'), &a), Status::Pending, "", 9)
            .unwrap();
        assert_eq!(
            store.state(Kind::Duplicate, &id('d'), &a).unwrap(),
            State::Pending
        );
        assert_eq!(
            store.state(Kind::Duplicate, &id('y'), &fp).unwrap(),
            State::Pending
        );
        assert_eq!(fs::read_to_string(&legacy).unwrap(), content);
    }

    #[test]
    fn reviews_toon_is_imported_once_and_left_byte_identical() {
        let (_root, path) = project();
        let file = path.join(".mct-index/reviews.toon");
        let (a, b, f1, f2) = (id('a'), id('b'), id('1'), id('2'));
        let content = format!(
            "version: 2\nreviews[2]{HEADER}\n  duplicate,{a},{f1},ignored,7,por qué\n  dead,{b},{f2},confirmed,9,\nhistory[3]{HEADER}\n  duplicate,{a},{f1},confirmed,5,\n  duplicate,{a},{f1},ignored,7,por qué\n  dead,{b},{f2},confirmed,9,\n"
        );
        fs::write(&file, &content).unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(store.decisions().unwrap().len(), 2);
        assert_eq!(store.history(Kind::Duplicate, &a).unwrap().len(), 2);
        assert!(matches!(
            store.state(Kind::Dead, &b, &f2).unwrap(),
            State::Decided {
                status: Status::Confirmed,
                at: Some(9),
                ..
            }
        ));
        drop(store);
        // A later edit of the old file is ignored: the import already happened.
        fs::write(&file, "garbage").unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(store.decisions().unwrap().len(), 2);
        assert_eq!(store.history(Kind::Duplicate, &a).unwrap().len(), 2);
        assert_eq!(fs::read_to_string(&file).unwrap(), "garbage");
    }

    #[test]
    fn a_corrupt_reviews_toon_fails_closed_and_imports_nothing() {
        let (_root, path) = project();
        let file = path.join(".mct-index/reviews.toon");
        fs::write(&file, "user data / corrupt state").unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "user data / corrupt state"
        );
        // Fix the file: the failed attempt must not have marked it imported.
        let a = id('a');
        fs::write(
            &file,
            format!(
                "version: 2\nreviews[1]{HEADER}\n  dead,{a},{a},ignored,1,\nhistory[0]{HEADER}\n"
            ),
        )
        .unwrap();
        assert_eq!(Store::open(&path).unwrap().decisions().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_writers_lose_nothing() {
        let (_root, path) = project();
        let handles: Vec<_> = (0..4u8)
            .map(|worker| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut store = Store::open(&path).unwrap();
                    for n in 0..25u8 {
                        let identity = format!("{worker:02x}{n:02x}").repeat(16);
                        store
                            .set((Kind::Dead, &identity, &id('f')), Status::Ignored, "", 1)
                            .unwrap();
                        store.add_comment(Kind::Dead, &identity, "note", 1).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.decisions().unwrap().len(), 100);
        let comments: i64 = store
            .conn
            .query_row("SELECT count(*) FROM comments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(comments, 100);
    }

    #[test]
    fn data_version_moves_when_another_connection_commits() {
        let (_root, path) = project();
        let mut writer = Store::open(&path).unwrap();
        let reader = Store::open(&path).unwrap();
        let before = reader.data_version().unwrap();
        writer
            .set((Kind::Dead, &id('a'), &id('f')), Status::Ignored, "", 1)
            .unwrap();
        assert_ne!(reader.data_version().unwrap(), before);
    }

    #[test]
    fn comments_support_create_read_update_delete_and_bounds() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let identity = id('a');
        let first = store
            .add_comment(Kind::Dead, &identity, "  line 1\r\nline\t2  ", 3)
            .unwrap();
        let second = store
            .add_comment(Kind::Dead, &identity, "other", 4)
            .unwrap();
        store
            .add_comment(Kind::Duplicate, &identity, "dup", 5)
            .unwrap();
        let list = store.comments(Kind::Dead, &identity).unwrap();
        assert_eq!(
            list.iter().map(|c| c.id).collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(list[0].body, "line 1\nline 2");
        assert_eq!((list[0].created_at, list[0].updated_at), (3, 3));
        assert!(store.update_comment(first, "edited", 9).unwrap());
        let list = store.comments(Kind::Dead, &identity).unwrap();
        assert_eq!(
            (
                list[0].body.as_str(),
                list[0].created_at,
                list[0].updated_at
            ),
            ("edited", 3, 9)
        );
        assert!(store.delete_comment(second).unwrap());
        assert!(!store.delete_comment(second).unwrap());
        assert!(!store.update_comment(second, "gone", 1).unwrap());
        assert_eq!(store.comments(Kind::Dead, &identity).unwrap().len(), 1);
        assert!(store.add_comment(Kind::Dead, &identity, "   ", 1).is_err());
        assert!(store
            .add_comment(Kind::Dead, &identity, &"x".repeat(MAX_COMMENT + 1), 1)
            .is_err());
        assert!(store
            .add_comment(Kind::Dead, &identity, &"x".repeat(MAX_COMMENT), 1)
            .is_ok());
        assert!(store.add_comment(Kind::Dead, "bad", "x", 1).is_err());
    }

    #[test]
    fn file_reviews_persist_update_clear_and_stay_per_group() {
        let (_root, path) = project();
        let (a, b) = (id('a'), id('b'));
        let mut store = Store::open(&path).unwrap();
        assert!(store.file_reviews(&a).unwrap().is_empty());
        store
            .set_file_review(&a, "src/x.rs", FileStatus::InReview, "mirar", 5)
            .unwrap();
        store
            .set_file_review(&b, "src/x.rs", FileStatus::Accepted, "", 6)
            .unwrap();
        drop(store);
        let mut store = Store::open(&path).unwrap();
        let rows = store.file_reviews(&a).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, FileStatus::InReview);
        assert_eq!(rows[0].note, "mirar");
        assert_eq!(
            store.file_reviews(&b).unwrap()[0].status,
            FileStatus::Accepted
        );
        store
            .set_file_review(&a, "src/x.rs", FileStatus::Rejected, "no", 7)
            .unwrap();
        assert_eq!(store.file_reviews(&a).unwrap()[0].updated_at, 7);
        // Pending with no note is the default: the row disappears.
        store
            .set_file_review(&a, "src/x.rs", FileStatus::Pending, "  ", 8)
            .unwrap();
        assert!(store.file_reviews(&a).unwrap().is_empty());
        assert_eq!(store.file_reviews(&b).unwrap().len(), 1);
    }

    #[test]
    fn file_reviews_reject_bad_keys_paths_and_long_notes() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let a = id('a');
        assert!(store
            .set_file_review("short", "x.rs", FileStatus::Accepted, "", 1)
            .is_err());
        assert!(store
            .set_file_review(&a, "", FileStatus::Accepted, "", 1)
            .is_err());
        assert!(store
            .set_file_review(&a, "a\nb.rs", FileStatus::Accepted, "", 1)
            .is_err());
        assert!(store
            .set_file_review(&a, &"p".repeat(1025), FileStatus::Accepted, "", 1)
            .is_err());
        assert!(store
            .set_file_review(&a, "x.rs", FileStatus::Accepted, &"n".repeat(2001), 1)
            .is_err());
        assert!(store.file_reviews(&a).unwrap().is_empty());
    }

    #[test]
    fn comments_per_finding_are_bounded() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let identity = id('a');
        for _ in 0..MAX_COMMENTS_PER_FINDING {
            store.add_comment(Kind::Dead, &identity, "n", 1).unwrap();
        }
        assert!(store.add_comment(Kind::Dead, &identity, "n", 1).is_err());
        assert!(store.add_comment(Kind::Dead, &id('b'), "n", 1).is_ok());
    }

    #[test]
    fn history_is_trimmed_to_the_newest_rows() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        for at in 0..5u64 {
            store
                .set((Kind::Dead, &id('a'), &id('f')), Status::Ignored, "", at)
                .unwrap();
        }
        let transaction = store
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        trim_history(&transaction, 3).unwrap();
        transaction.commit().unwrap();
        let times: Vec<_> = store
            .history(Kind::Dead, &id('a'))
            .unwrap()
            .iter()
            .map(|r| r.at)
            .collect();
        assert_eq!(times, [4, 3, 2]);
    }

    #[test]
    fn the_store_survives_reopen_with_a_rebuilt_index_next_to_it() {
        // The index database is a separate file; deleting it must not touch ours.
        let (_root, path) = project();
        let index = path.join(".mct-index/index.sqlite3");
        fs::write(&index, "derived").unwrap();
        let mut store = Store::open(&path).unwrap();
        store
            .set((Kind::Dead, &id('a'), &id('f')), Status::Ignored, "", 1)
            .unwrap();
        drop(store);
        fs::remove_file(&index).unwrap();
        assert_eq!(Store::open(&path).unwrap().decisions().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_hard_links_cannot_redirect_the_database() {
        use std::os::unix::fs::symlink;
        let (_root, path) = project();
        let file = path.join(".mct-index/dashboard.sqlite3");
        let external = path.join("outside.sqlite3");
        fs::write(&external, "").unwrap();
        symlink(&external, &file).unwrap();
        assert!(Store::open(&path).is_err(), "symlink");
        fs::remove_file(&file).unwrap();
        fs::hard_link(&external, &file).unwrap();
        assert!(Store::open(&path).is_err(), "hard link");
        assert_eq!(fs::read(&external).unwrap(), b"");
        fs::remove_file(&file).unwrap();
        let alias = path.join("alias");
        symlink(path.join(".mct-index"), &alias).unwrap();
        assert!(Store::open(&alias).is_err(), "symlinked directory");
    }

    #[test]
    fn a_missing_index_directory_is_an_error_not_a_creation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        assert!(Store::open(&path).is_err());
        assert!(!path.join(".mct-index").exists());
    }
    #[test]
    fn v2_store_migrates_only_proven_legacy_fingerprints_and_preserves_history() {
        let (_root, path) = project();
        let file = path.join(".mct-index/dashboard.sqlite3");
        let mut old = Connection::open(&file).unwrap();
        Migrations::new(vec![M::up(SCHEMA), M::up(SCHEMA_FILE_REVIEWS)])
            .to_latest(&mut old)
            .unwrap();
        let (a, b, old_fp, new_fp) = (id('a'), id('b'), id('1'), id('2'));
        for identity in [&a, &b] {
            old.execute(
                "INSERT INTO reviews VALUES ('dead', ?1, ?2, 'ignored', 7, 'reason')",
                params![identity, old_fp],
            )
            .unwrap();
            old.execute("INSERT INTO review_history(kind, identity, fingerprint, status, decided_at, reason) VALUES ('dead', ?1, ?2, 'ignored', 7, 'reason')", params![identity, old_fp]).unwrap();
        }
        drop(old);
        let mut store = Store::open(&path).unwrap();
        store
            .migrate_fingerprints(&[
                (Kind::Dead, a.clone(), old_fp.clone(), new_fp.clone()),
                (Kind::Dead, b.clone(), id('3'), new_fp.clone()),
            ])
            .unwrap();
        assert!(
            matches!(store.state(Kind::Dead, &a, &new_fp).unwrap(), State::Decided { at: Some(7), ref reason, .. } if reason == "reason")
        );
        assert!(matches!(
            store.state(Kind::Dead, &b, &new_fp).unwrap(),
            State::NeedsReevaluation { .. }
        ));
        assert_eq!(
            store.history(Kind::Dead, &a).unwrap()[0].fingerprint,
            old_fp
        );
        // Once v2, even a matching old hash must never silently upgrade again.
        store
            .migrate_fingerprints(&[(Kind::Dead, a.clone(), new_fp.clone(), id('4'))])
            .unwrap();
        drop(store);
        let mut store = Store::open(&path).unwrap();
        assert!(matches!(
            store.state(Kind::Dead, &a, &new_fp).unwrap(),
            State::Decided { .. }
        ));
        store
            .set((Kind::Dead, &b, &new_fp), Status::Confirmed, "checked", 9)
            .unwrap();
        let version: i64 = store
            .conn
            .query_row(
                "SELECT fingerprint_version FROM reviews WHERE identity = ?1",
                [&b],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 2);
    }

    #[cfg(unix)]
    #[test]
    fn database_is_private_on_creation_and_reopen() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, path) = project();
        let file = path.join(".mct-index/dashboard.sqlite3");
        let store = Store::open(&path).unwrap();
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(store);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        let _store = Store::open(&path).unwrap();
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn stale_decision_writes_conflict_without_changing_state_or_history() {
        let (_root, path) = project();
        let mut first = Store::open(&path).unwrap();
        let mut second = Store::open(&path).unwrap();
        let (identity, fp) = (id('a'), id('f'));
        assert_eq!(first.decision_revision(Kind::Dead, &identity).unwrap(), 0);
        first
            .set_checked(
                (Kind::Dead, &identity, &fp),
                Status::FalsePositive,
                "reviewed",
                1,
                0,
            )
            .unwrap();
        let error = second
            .set_checked((Kind::Dead, &identity, &fp), Status::Ignored, "stale", 2, 0)
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<Conflict>(),
            Some(&Conflict {
                expected: 0,
                actual: 1
            })
        );
        assert_eq!(first.history(Kind::Dead, &identity).unwrap().len(), 1);
        assert!(matches!(
            first.state(Kind::Dead, &identity, &fp).unwrap(),
            State::Decided {
                status: Status::FalsePositive,
                ..
            }
        ));
        second
            .set_checked(
                (Kind::Dead, &identity, &fp),
                Status::Ignored,
                "new review",
                3,
                1,
            )
            .unwrap();
        assert_eq!(first.decision_revision(Kind::Dead, &identity).unwrap(), 2);
    }

    #[test]
    fn file_revisions_survive_clearing_and_refuse_aba_writes() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let identity = id('a');
        store
            .set_file_review_checked(&identity, "a.rs", FileStatus::Accepted, "first", 1, 0)
            .unwrap();
        assert!(store
            .set_file_review_checked(&identity, "a.rs", FileStatus::Rejected, "stale", 2, 0)
            .unwrap_err()
            .is::<Conflict>());
        store
            .set_file_review_checked(&identity, "a.rs", FileStatus::Pending, "", 3, 1)
            .unwrap();
        assert!(store.file_reviews(&identity).unwrap().is_empty());
        assert_eq!(store.file_revision(&identity, "a.rs").unwrap(), 2);
        assert!(store
            .set_file_review_checked(&identity, "a.rs", FileStatus::Rejected, "old editor", 4, 0)
            .unwrap_err()
            .is::<Conflict>());
        store
            .set_file_review_checked(&identity, "a.rs", FileStatus::InReview, "current", 5, 2)
            .unwrap();
        assert_eq!(store.file_revision(&identity, "a.rs").unwrap(), 3);
        assert_eq!(store.file_revision(&identity, "b.rs").unwrap(), 0);
    }

    #[test]
    fn deleted_legacy_verdicts_stay_deleted_across_reopen_and_new_annotations() {
        let (_root, path) = project();
        let (identity, fp) = (id('a'), id('f'));
        fs::write(
            path.join(".mct-index/duplicate-reviews.toon"),
            format!("version: 1\nreviews[1]{{key,reason}}:\n  {fp},false-positive\n"),
        )
        .unwrap();
        let mut store = Store::open(&path).unwrap();
        store
            .set_checked(
                (Kind::Duplicate, &identity, &fp),
                Status::Ignored,
                "newer",
                1,
                0,
            )
            .unwrap();
        store
            .add_comment(Kind::Duplicate, &identity, "retained", 2)
            .unwrap();
        let revision = store.decision_revision(Kind::Duplicate, &identity).unwrap();
        assert!(store
            .delete_decision_checked(Kind::Duplicate, &identity, revision)
            .unwrap());
        assert!(!store
            .contains_stored_finding(Kind::Duplicate, &identity)
            .unwrap());
        assert!(store.stored_findings().unwrap().is_empty());
        drop(store);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(
            store.state(Kind::Duplicate, &identity, &fp).unwrap(),
            State::Pending
        );
        assert_eq!(store.history(Kind::Duplicate, &identity).unwrap().len(), 1);
        assert_eq!(store.comments(Kind::Duplicate, &identity).unwrap().len(), 1);
        let archived_revision = store.decision_revision(Kind::Duplicate, &identity).unwrap();
        assert!(store
            .set_checked(
                (Kind::Duplicate, &identity, &fp),
                Status::Confirmed,
                "stale",
                3,
                1
            )
            .unwrap_err()
            .is::<Conflict>());
        store
            .add_comment(Kind::Duplicate, &identity, "new annotation", 4)
            .unwrap();
        assert!(store
            .contains_stored_finding(Kind::Duplicate, &identity)
            .unwrap());
        assert_eq!(
            store.state(Kind::Duplicate, &identity, &fp).unwrap(),
            State::Pending,
            "new notes must not reinstate ADR-015 verdicts"
        );
        assert!(store.decision_revision(Kind::Duplicate, &identity).unwrap() > archived_revision);
        let revision = store.decision_revision(Kind::Duplicate, &identity).unwrap();
        store
            .set_checked(
                (Kind::Duplicate, &identity, &fp),
                Status::Confirmed,
                "explicitly reconsidered",
                5,
                revision,
            )
            .unwrap();
        assert!(matches!(
            store.state(Kind::Duplicate, &identity, &fp).unwrap(),
            State::Decided {
                status: Status::Confirmed,
                ..
            }
        ));
    }

    #[test]
    fn new_annotations_prevent_stale_orphan_archival() {
        let (_root, path) = project();
        let mut first = Store::open(&path).unwrap();
        let mut second = Store::open(&path).unwrap();
        let identity = id('a');
        first
            .add_comment(Kind::Dead, &identity, "first", 1)
            .unwrap();
        let stale_revision = first.decision_revision(Kind::Dead, &identity).unwrap();
        second
            .add_comment(Kind::Dead, &identity, "written after orphan page loaded", 2)
            .unwrap();
        assert!(first
            .delete_decision_checked(Kind::Dead, &identity, stale_revision)
            .unwrap_err()
            .is::<Conflict>());
        assert_eq!(first.comments(Kind::Dead, &identity).unwrap().len(), 2);
        assert!(first
            .contains_stored_finding(Kind::Dead, &identity)
            .unwrap());
    }

    #[test]
    fn restored_and_annotation_only_findings_remain_discoverable_until_archived() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let (restored, comment_only, file_only, history_only, fp) =
            (id('a'), id('b'), id('c'), id('d'), id('f'));
        store
            .set((Kind::Dead, &restored, &fp), Status::Ignored, "first", 1)
            .unwrap();
        store
            .set((Kind::Dead, &restored, &fp), Status::Pending, "", 2)
            .unwrap();
        store
            .add_comment(Kind::Dead, &comment_only, "annotation", 3)
            .unwrap();
        store
            .set_file_review(&file_only, "a.rs", FileStatus::InReview, "working note", 4)
            .unwrap();
        store.conn.execute("INSERT INTO review_history(kind, identity, fingerprint, status, decided_at, reason) VALUES ('dead', ?1, ?2, 'ignored', 5, 'imported history')", params![history_only, fp]).unwrap();
        let rows = store.stored_findings().unwrap();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.status == Status::Pending));
        for (kind, identity) in [
            (Kind::Dead, &restored),
            (Kind::Dead, &comment_only),
            (Kind::Duplicate, &file_only),
            (Kind::Dead, &history_only),
        ] {
            assert!(store.contains_stored_finding(kind, identity).unwrap());
            assert!(store.delete_decision(kind, identity).unwrap());
            assert!(!store.contains_stored_finding(kind, identity).unwrap());
        }
        assert!(store.stored_findings().unwrap().is_empty());
        assert_eq!(store.comments(Kind::Dead, &comment_only).unwrap().len(), 1);
        assert_eq!(store.file_reviews(&file_only).unwrap().len(), 1);
    }

    #[test]
    fn restored_rows_do_not_exhaust_the_active_review_quota() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        store.conn.execute(
            "WITH RECURSIVE n(value) AS (SELECT 1 UNION ALL SELECT value + 1 FROM n WHERE value < ?1)
             INSERT INTO reviews(kind, identity, fingerprint, status, decided_at, reason)
             SELECT 'dead', printf('%064x', value), ?2, 'pending', 1, '' FROM n",
            params![MAX_REVIEWS as i64, id('f')],
        ).unwrap();
        let identity = id('e');
        store
            .set_checked(
                (Kind::Dead, &identity, &id('f')),
                Status::Ignored,
                "new active decision",
                2,
                0,
            )
            .unwrap();
        assert_eq!(store.decisions().unwrap().len(), 1);
        // Existing restored rows must also pass the active quota check when
        // becoming decided; existence alone cannot bypass it.
        store
            .conn
            .execute(
                "UPDATE reviews SET status = 'ignored' WHERE identity <> ?1",
                [&identity],
            )
            .unwrap();
        store
            .set((Kind::Dead, &identity, &id('f')), Status::Pending, "", 3)
            .unwrap();
        assert!(store
            .set(
                (Kind::Dead, &identity, &id('f')),
                Status::Confirmed,
                "over quota",
                4
            )
            .is_err());
    }

    #[test]
    fn dead_bindings_are_idempotent_and_require_the_current_review_fingerprint() {
        let (_root, path) = project();
        let mut store = Store::open(&path).unwrap();
        let (identity, base, fp) = (id('a'), id('b'), id('f'));
        store
            .set_checked(
                (Kind::Dead, &identity, &fp),
                Status::Ignored,
                "reviewed",
                1,
                0,
            )
            .unwrap();
        let revision = store.decision_revision(Kind::Dead, &identity).unwrap();
        let before = store.change_token().unwrap();
        store
            .remember_dead_bindings(&[(identity.clone(), base.clone(), fp.clone())])
            .unwrap();
        assert_ne!(store.change_token().unwrap(), before);
        assert_eq!(
            store.decision_revision(Kind::Dead, &identity).unwrap(),
            revision
        );
        let saved: String = store
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                [format!("dead_base:{identity}")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved, base);
        let before = store.change_token().unwrap();
        store
            .remember_dead_bindings(&[(identity.clone(), base.clone(), fp.clone())])
            .unwrap();
        assert_eq!(
            store.change_token().unwrap(),
            before,
            "repeated snapshot loads must not create a change loop"
        );
        store
            .remember_dead_bindings(&[
                (identity.clone(), id('c'), id('1')),
                (id('e'), base.clone(), fp.clone()),
            ])
            .unwrap();
        assert_eq!(
            store.change_token().unwrap(),
            before,
            "stale or unknown bindings write nothing"
        );
        let saved: String = store
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                [format!("dead_fingerprint:{identity}")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved, fp);
        assert!(store
            .remember_dead_bindings(&[(identity, "invalid".into(), fp)])
            .is_err());
    }
}
