//! One-shot maintenance for an owner-installed OS scheduler. No daemon.
use super::{backup, reviews};
use anyhow::{ensure, Context};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Explicit invocation policy; no background execution is enabled implicitly.
#[derive(clap::Args, Debug, Clone)]
pub(crate) struct Policy {
    #[arg(long)]
    pub enabled: bool,
    #[arg(long)]
    pub destination: Option<PathBuf>,
    #[arg(long, default_value_t = 86400)]
    pub interval_seconds: u64,
    #[arg(long, default_value_t = 7)]
    pub max_count: usize,
    /// Zero disables age expiration.
    #[arg(long, default_value_t = 2592000)]
    pub max_age_seconds: u64,
    #[arg(long, default_value_t = 1073741824)]
    pub max_bytes: u64,
    #[arg(long, default_value_t = 2)]
    pub min_verified: usize,
    #[arg(long, default_value_t = 604800)]
    pub verify_seconds: u64,
}

const STATE: &str = "management.toon";
const RECEIPT: &str = "managed.toon";
const LIMIT: usize = 10000;

#[derive(Default, Debug)]
struct State {
    attempt: u64,
    success: u64,
    verified: u64,
    failures: u64,
    retry: u64,
    phase: String,
}

fn fields(path: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    reviews::safe_path(path)?;
    ensure!(
        fs::metadata(path)?.len() <= 4096,
        "management record too large"
    );
    let mut text = String::new();
    fs::File::open(path)?.take(4097).read_to_string(&mut text)?;
    ensure!(text.len() <= 4096, "management record too large");
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once(": ").context("invalid management record")?;
        ensure!(
            result.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate management field"
        );
    }
    Ok(result)
}

fn source_id(root: &Path) -> anyhow::Result<String> {
    // Identity, not authentication: only trusted owner-controlled local paths.
    let path = root.canonicalize()?.join(".mct-index/dashboard.sqlite3");
    let bytes = path.as_os_str().as_encoded_bytes();
    Ok(hmac_sha256::Hash::hash(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

impl State {
    fn read(destination: &Path, owner: &str) -> anyhow::Result<Self> {
        let path = destination.join(STATE);
        if !path.exists() {
            return Ok(Self {
                phase: "scheduled".into(),
                ..Self::default()
            });
        }
        let f = fields(&path)?;
        ensure!(
            f.get("format").map(String::as_str) == Some("mct-backup-management/1")
                && f.get("source").map(String::as_str) == Some(owner),
            "backup destination belongs to another source or unsupported manager"
        );
        let number = |key: &str| -> anyhow::Result<u64> {
            Ok(f.get(key).context("missing management field")?.parse()?)
        };
        Ok(Self {
            attempt: number("attempt")?,
            success: number("success")?,
            verified: number("verified")?,
            failures: number("failures")?,
            retry: number("retry")?,
            phase: f.get("phase").context("missing phase")?.clone(),
        })
    }
    fn write(&self, destination: &Path, owner: &str) -> anyhow::Result<()> {
        let path = destination.join(STATE);
        reviews::safe_path(&path)?;
        let mut file = tempfile::NamedTempFile::new_in(destination)?;
        write!(file, "format: mct-backup-management/1\nsource: {owner}\nattempt: {}\nsuccess: {}\nverified: {}\nfailures: {}\nretry: {}\nphase: {}\n", self.attempt, self.success, self.verified, self.failures, self.retry, self.phase)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        backup::sync_directory(destination)
    }
    fn failed(&mut self, now: u64, destination: &Path, owner: &str) -> anyhow::Result<()> {
        self.failures = self.failures.saturating_add(1);
        self.retry = now.saturating_add(
            300u64
                .saturating_mul(1u64 << self.failures.saturating_sub(1).min(4))
                .min(3600),
        );
        self.phase = "failed".into();
        self.write(destination, owner)
    }
}

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    created: u64,
    schema: Option<i64>,
    order: u128,
    attempt: u64,
    bytes: u64,
    valid: bool,
    complete: bool,
    owned: bool,
    preserved: bool,
    deletable: bool,
}

fn receipt_matches(path: &Path, owner: &str) -> bool {
    fields(&path.join(RECEIPT)).is_ok_and(|f| {
        f.len() == 4
            && f.get("format").map(String::as_str) == Some("mct-managed-backup/1")
            && f.get("source").map(String::as_str) == Some(owner)
            && f.get("attempt")
                .and_then(|v| v.parse::<u64>().ok())
                .is_some()
            && f.get("order")
                .and_then(|v| v.parse::<u128>().ok())
                .is_some()
    })
}

/// Bounded, one-level scan. Unknown layouts are never traversed or removed.
fn catalog(destination: &Path, owner: &str, verify: bool) -> anyhow::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for (i, item) in fs::read_dir(destination)?.enumerate() {
        ensure!(
            i < LIMIT,
            "backup catalog exceeds 10000 entries; operator inspection required"
        );
        let item = item?;
        let name = item.file_name();
        if !name.to_string_lossy().starts_with("dashboard-") {
            continue;
        }
        let path = item.path();
        // Fail closed rather than follow links, including broken links.
        reviews::safe_path(&path.join(".bundle-path-check"))?;
        ensure!(item.file_type()?.is_dir(), "unexpected backup entry type");
        backup::private_directory(&path)?;
        let mut bytes = 0u64;
        let mut deletable = true;
        let mut count = 0;
        for child in fs::read_dir(&path)? {
            count += 1;
            ensure!(count <= 16, "unexpected backup bundle layout");
            let child = child?;
            reviews::safe_path(&child.path())?;
            let meta = fs::symlink_metadata(child.path())?;
            if !meta.is_file() {
                deletable = false;
            }
            bytes = bytes.saturating_add(meta.len());
            let name = child.file_name();
            if !["dashboard.sqlite3", "manifest.toon", RECEIPT]
                .iter()
                .any(|n| name == *n)
            {
                deletable = false;
            }
        }
        let created = fields(&path.join("manifest.toon"))
            .ok()
            .and_then(|f| f.get("created_unix")?.parse().ok())
            .unwrap_or(0);
        let valid = verify && backup::verify_restore(&path).is_ok();
        entries.push(Entry {
            owned: receipt_matches(&path, owner),
            preserved: path.join("preserve").exists(),
            path: path.clone(),
            created,
            schema: fields(&path.join("manifest.toon"))
                .ok()
                .and_then(|f| f.get("schema")?.parse().ok()),
            order: fields(&path.join(RECEIPT))
                .ok()
                .and_then(|f| f.get("order")?.parse().ok())
                .unwrap_or(u128::from(created) * 1_000_000_000),
            attempt: fields(&path.join(RECEIPT))
                .ok()
                .and_then(|f| f.get("attempt")?.parse().ok())
                .unwrap_or(0),
            bytes,
            valid,
            complete: path.join("manifest.toon").is_file(),
            deletable,
        });
    }
    entries.sort_by(|a, b| {
        let stamp = |e: &Entry| if e.owned { e.attempt } else { e.created };
        stamp(a)
            .cmp(&stamp(b))
            .then(a.order.cmp(&b.order))
            .then(a.path.cmp(&b.path))
    });
    Ok(entries)
}

/// Account for every regular file in the dedicated destination, including
/// incomplete/foreign files; never recurse into arbitrary directory trees.
fn storage_bytes(destination: &Path) -> anyhow::Result<u64> {
    let mut sum = 0u64;
    for (i, item) in fs::read_dir(destination)?.enumerate() {
        ensure!(i < LIMIT, "backup directory exceeds inventory limit");
        let item = item?;
        let meta = fs::symlink_metadata(item.path())?;
        if meta.is_dir() {
            reviews::safe_path(&item.path().join(".storage-path-check"))?;
            for (j, child) in fs::read_dir(item.path())?.enumerate() {
                ensure!(
                    j < 16,
                    "backup directory layout exceeds storage inventory limit"
                );
                let child = child?;
                reviews::safe_path(&child.path())?;
                ensure!(
                    child.file_type()?.is_file(),
                    "nested backup storage directories are unsupported"
                );
                sum = sum.saturating_add(child.metadata()?.len());
            }
        } else {
            reviews::safe_path(&item.path())?;
            sum = sum.saturating_add(meta.len());
        }
    }
    Ok(sum)
}

fn retain(destination: &Path, owner: &str, policy: &Policy, now: u64) -> anyhow::Result<bool> {
    // No cached validity can authorize deletion.
    let mut entries = catalog(destination, owner, true)?;
    let mut count = entries.iter().filter(|e| e.valid && e.owned).count();
    let mut bytes = storage_bytes(destination)?;
    let newest = entries.iter().rposition(|e| e.valid && e.owned);
    for (i, entry) in entries.iter_mut().enumerate() {
        let expired = policy.max_age_seconds != 0
            && now.saturating_sub(entry.created) > policy.max_age_seconds;
        if !(expired || count > policy.max_count || bytes > policy.max_bytes) {
            continue;
        }
        if !entry.valid
            || !entry.owned
            || !entry.deletable
            || entry.preserved
            || Some(i) == newest
            || count <= policy.min_verified
        {
            continue;
        }
        // Revalidate the exact directory immediately before bounded deletion.
        ensure!(
            receipt_matches(&entry.path, owner),
            "backup ownership changed during retention"
        );
        backup::verify_restore(&entry.path)?;
        let names: Vec<_> = fs::read_dir(&entry.path)?
            .map(|x| x.map(|e| e.file_name()))
            .collect::<Result<_, _>>()?;
        ensure!(
            names.len() == 3
                && names
                    .iter()
                    .all(|n| ["dashboard.sqlite3", "manifest.toon", RECEIPT]
                        .iter()
                        .any(|s| n == s)),
            "backup layout changed during retention"
        );
        for name in ["manifest.toon", "dashboard.sqlite3", RECEIPT] {
            let file = entry.path.join(name);
            reviews::safe_path(&file)?;
            fs::remove_file(file)?;
        }
        fs::remove_dir(&entry.path)?;
        backup::sync_directory(destination)?;
        count -= 1;
        bytes = bytes.saturating_sub(entry.bytes);
        entry.valid = false;
    }
    let age_conflict = entries.iter().any(|e| {
        e.valid
            && e.owned
            && policy.max_age_seconds != 0
            && now.saturating_sub(e.created) > policy.max_age_seconds
    });
    Ok(count <= policy.max_count && bytes <= policy.max_bytes && !age_conflict)
}

fn validate_policy(policy: &Policy) -> anyhow::Result<()> {
    ensure!(
        policy.interval_seconds >= 60 && policy.verify_seconds >= 60,
        "backup intervals must be at least 60 seconds"
    );
    ensure!(
        policy.min_verified >= 1
            && policy.max_count >= policy.min_verified
            && policy.max_count <= LIMIT,
        "retention count must protect at least one verified point and not exceed 10000"
    );
    ensure!(policy.max_bytes > 0, "backup quota must be positive");
    Ok(())
}

pub(crate) fn run(root: &Path, policy: &Policy, status_only: bool) -> anyhow::Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    run_at(root, policy, status_only, now)
}

fn run_at(root: &Path, policy: &Policy, status_only: bool, now: u64) -> anyhow::Result<()> {
    validate_policy(policy)?;
    println!("automatic_enabled: {}\nconfiguration_scope: invocation\nscheduler_installation: operator-managed-unverified", policy.enabled);
    println!("interval_seconds: {}\nmax_count: {}\nmax_age_seconds: {}\nmax_bytes: {}\nmin_verified: {}\nverify_seconds: {}", policy.interval_seconds, policy.max_count, policy.max_age_seconds, policy.max_bytes, policy.min_verified, policy.verify_seconds);
    if !policy.enabled && !status_only {
        println!("phase: disabled");
        return Ok(());
    }
    let destination = backup::prepare_destination(root, policy.destination.as_deref())?;
    let access = mct_index::coordination::DatabaseAccess::shared(
        &destination.join("management"),
        Duration::ZERO,
    )?;
    let _guard = access
        .writer(Duration::ZERO)
        .context("backup maintenance already running; retry on next scheduler tick")?;
    let owner = source_id(root)?;
    let mut state = State::read(&destination, &owner)?;
    ensure!(
        now >= state.attempt && now >= state.success && now >= state.verified,
        "backup clock moved backwards; inspect clock before retry"
    );
    let mut retention_conflict = false;
    if !status_only {
        let entries = catalog(&destination, &owner, false)?;
        // Recover a manifest-published point if process died before state commit.
        let mut recovered = false;
        let attempt = state.attempt;
        let success = state.success;
        for entry in entries
            .iter()
            .filter(|e| e.owned && e.attempt == attempt && attempt > success)
        {
            if backup::verify_restore(&entry.path).is_ok() {
                state.success = now;
                state.verified = now;
                state.failures = 0;
                state.retry = 0;
                state.phase = "valid".into();
                recovered = true;
            }
        }
        if recovered {
            state.write(&destination, &owner)?;
            retention_conflict = !retain(&destination, &owner, policy, now)?;
        }
        let due =
            state.success == 0 || now.saturating_sub(state.success) >= policy.interval_seconds;
        if due && now >= state.retry {
            if storage_bytes(&destination)? >= policy.max_bytes {
                state.phase = "quota-conflict".into();
                state.write(&destination, &owner)?;
                anyhow::bail!("backup storage quota already exhausted; no recovery points removed; increase quota or inspect storage before retry");
            }
            state.attempt = now;
            state.phase = "running".into();
            state.write(&destination, &owner)?;
            let result = (|| -> anyhow::Result<()> {
                let bundle = backup::create_managed(root, &destination, Some((&owner, now)))?;
                state.phase = "verifying".into();
                state.write(&destination, &owner)?;
                backup::verify_restore(&bundle)?;
                Ok(())
            })();
            if result.is_err() {
                state.failed(now, &destination, &owner)?;
                anyhow::bail!("scheduled backup failed; previous recovery points retained; run explicit backup verification and check storage permissions/space/schema");
            }
            state.success = now;
            state.verified = now;
            state.failures = 0;
            state.retry = 0;
            state.phase = "valid".into();
            state.write(&destination, &owner)?;
            let satisfied = retain(&destination, &owner, policy, now)?;
            retention_conflict = !satisfied;
            println!("retention_satisfied: {satisfied}");
        } else if !due
            && now >= state.retry
            && now.saturating_sub(state.verified) >= policy.verify_seconds
        {
            let entries = catalog(&destination, &owner, true)?;
            if entries.iter().any(|e| e.owned && e.complete && !e.valid)
                || !entries.iter().any(|e| e.owned && e.valid)
            {
                state.failed(now, &destination, &owner)?;
                anyhow::bail!("periodic backup verification failed; retention skipped");
            }
            state.verified = now;
            state.phase = "valid".into();
            state.write(&destination, &owner)?;
            retention_conflict = !retain(&destination, &owner, policy, now)?;
            println!("retention_satisfied: {}", !retention_conflict);
        }
    }
    if retention_conflict {
        state.phase = "valid-retention-conflict".into();
        state.write(&destination, &owner)?;
    }
    let entries = catalog(&destination, &owner, status_only)?;
    println!("phase: {}\nlast_attempt: {}\nlast_success: {}\nlast_verified: {}\nfailures: {}\nnext_due_if_enabled: {}\nbackup_storage_bytes: {}", state.phase, state.attempt, state.success, state.verified, state.failures, if state.failures > 0 { state.retry } else { state.success.saturating_add(policy.interval_seconds) }, storage_bytes(&destination)?);
    if status_only {
        let valid = entries.iter().filter(|e| e.valid).count();
        let owned_valid = entries.iter().filter(|e| e.valid && e.owned).count();
        let satisfied = owned_valid <= policy.max_count
            && storage_bytes(&destination)? <= policy.max_bytes
            && !entries.iter().any(|e| {
                e.valid
                    && e.owned
                    && policy.max_age_seconds != 0
                    && now.saturating_sub(e.created) > policy.max_age_seconds
            });
        println!(
            "retention_satisfied: {satisfied}\nlatest_failure: {}",
            if state.failures > 0 {
                "backup_or_verification_failed"
            } else {
                "none"
            }
        );
        println!("valid_backups: {valid}\nsource_verified_backups: {owned_valid}\nminimum_satisfied: {}\nrecovery_ready: {}", owned_valid >= policy.min_verified, owned_valid > 0);
        for (i, entry) in entries.iter().enumerate() {
            println!("backup_{i}: {}\n  created_unix: {}\n  age_seconds: {}\n  bytes: {}\n  valid: {}\n  managed: {}\n  preserved: {}\n  complete: {}\n  schema: {}\n  schema_compatible: {}", entry.path.file_name().unwrap_or_default().to_string_lossy(), entry.created, now.saturating_sub(entry.created), entry.bytes, entry.valid, entry.owned, entry.preserved, entry.complete, entry.schema.map_or_else(|| "unknown".to_owned(), |s| s.to_string()), entry.schema == Some(mct_index::review_state::SUPPORTED_SCHEMA_VERSION));
        }
        ensure!(
            owned_valid > 0,
            "CRITICAL: no verified recovery point for this source available"
        );
    }
    ensure!(!retention_conflict, "retention constraints cannot be met safely; recovery points retained; increase limits or inspect preserved/incomplete files");
    Ok(())
}

#[cfg(test)]
#[path = "backup_manager_tests.rs"]
mod tests;
