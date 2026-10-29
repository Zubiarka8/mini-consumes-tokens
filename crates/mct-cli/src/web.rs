//! Local read-only views over public index APIs. No directory is served.

mod api;
mod assets;
mod dead_assessment;
mod http;
mod ignore_file;
mod numbers;
mod obsidian;
mod port;
mod response;
mod reviews;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mct_index::{DuplicateOptions, DuplicateReport, ExcludeSet, Index, NavigationSnapshot};

struct DeadCandidate {
    /// Index into `navigation.symbols`.
    symbol: usize,
    key: String,
    assessment: dead_assessment::Assessment,
}

#[derive(Default)]
struct Timings {
    load_ms: u128,
    duplicates_ms: u128,
    dead_code_ms: u128,
}

/// What identifies one published state of the index file: a reindex changes
/// the modification time or the length of the database or its write-ahead log.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct IndexStamp([Option<(SystemTime, u64)>; 2]);

impl IndexStamp {
    fn of(root: &Path) -> Self {
        let database = root.join(".mct-index/index.sqlite3");
        let wal = root.join(".mct-index/index.sqlite3-wal");
        let read = |path: &Path| {
            let meta = std::fs::metadata(path).ok()?;
            Some((meta.modified().ok()?, meta.len()))
        };
        Self([read(&database), read(&wal)])
    }
}

struct Site {
    index: Index,
    root: PathBuf,
    no_obsidian: bool,
    stamp: IndexStamp,
    navigation: NavigationSnapshot,
    symbols: BTreeMap<String, Vec<usize>>,
    imports: BTreeMap<String, Vec<String>>,
    importers: BTreeMap<String, Vec<String>>,
    groups: BTreeMap<String, Vec<usize>>,
    duplicates: DuplicateReport,
    review_keys: Vec<String>,
    /// Readable number of each finding, by review key.
    numbers: numbers::Numbers,
    dead: Vec<DeadCandidate>,
    references: HashMap<String, usize>,
    reviews: reviews::Reviews,
    vault: obsidian::Vault,
    vault_dir: PathBuf,
    /// Duplicate group ids per symbol id, and the dead-code candidate index per symbol id.
    symbol_groups: HashMap<usize, Vec<usize>>,
    dead_by_symbol: HashMap<usize, usize>,
    timings: Timings,
    /// Measured server-side latency of every API route, in microseconds.
    latency: RefCell<BTreeMap<&'static str, Vec<u64>>>,
    project: String,
    generated: u64,
    warning: Option<String>,
    csrf: String,
}

impl Site {
    fn load(root: &Path, no_obsidian: bool) -> anyhow::Result<Self> {
        Self::load_snapshot(root, no_obsidian, || {})
    }

    // The callback lets integration tests publish between the navigation query
    // and the remaining queries, without timing-dependent threads or sleeps.
    fn load_snapshot(
        root: &Path,
        no_obsidian: bool,
        after_navigation: impl FnOnce(),
    ) -> anyhow::Result<Self> {
        let started = Instant::now();
        let stamp = IndexStamp::of(root);
        let database = root.join(".mct-index/index.sqlite3");
        anyhow::ensure!(
            database.is_file(),
            "no index found; run `mct-cli init` first"
        );
        let index = Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root))?;
        let csrf = random_key()?;
        let warning = if no_obsidian {
            None
        } else {
            mct_obsidian::write_vault(&index, &mct_obsidian::vault_dir(root))
                .err()
                .map(|error| format!("Cannot regenerate the Obsidian vault: {error}"))
        };
        if let Some(warning) = &warning {
            eprintln!("warning: {warning}");
        }
        // Navigation, candidates and review hashes must describe one generation.
        let snapshot = index.read_snapshot()?;
        let mut navigation = index.navigation_snapshot()?;
        after_navigation();
        navigation.files.retain(|file| file.language != "asset");
        navigation
            .files
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let mut symbols: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (id, symbol) in navigation.symbols.iter().enumerate() {
            symbols
                .entry(symbol.relative_path.clone())
                .or_default()
                .push(id);
        }
        for ids in symbols.values_mut() {
            ids.sort_by_key(|id| navigation.symbols[*id].line);
        }
        let mut imports: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut importers: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let known: std::collections::BTreeSet<_> = navigation
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        for edge in &navigation.imports {
            if known.contains(edge.from.as_str()) && known.contains(edge.to.as_str()) {
                imports
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
                importers
                    .entry(edge.to.clone())
                    .or_default()
                    .push(edge.from.clone());
            }
        }
        for paths in imports.values_mut().chain(importers.values_mut()) {
            paths.sort();
            paths.dedup();
        }
        eprintln!("web: analyzing duplicate candidates with the existing default policy…");
        let duplicates_started = Instant::now();
        let duplicates = mct_index::find_duplicate_groups(
            &index,
            &mct_languages::build_token_sources(),
            DuplicateOptions::default(),
        )?;
        let duplicates_ms = duplicates_started.elapsed().as_millis();
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let file_hashes = index.file_hashes()?;
        let mut findings = mct_index::reviews::duplicate_findings(
            &duplicates.groups,
            duplicates.options.normalization,
            &file_hashes,
        );
        let review_keys = findings.iter().map(|f| f.identity.clone()).collect();
        for (id, group) in duplicates.groups.iter().enumerate() {
            for member in &group.members {
                let ids = groups.entry(member.path.clone()).or_default();
                if ids.last() != Some(&id) {
                    ids.push(id);
                }
            }
        }
        let dead_started = Instant::now();
        let (dead, dead_findings) = dead_candidates(root, &index, &navigation, &file_hashes)?;
        let dead_code_ms = dead_started.elapsed().as_millis();
        findings.extend(dead_findings);
        let numbers = numbers::Numbers::load(root, &findings);
        let reviews = reviews::Reviews::load(root, findings);
        if let Some(reason) = reviews.read_only_reason() {
            eprintln!("warning: review decisions are read-only until this is fixed: {reason}");
        }
        let references = index.reference_counts()?;
        drop(snapshot);
        let vault_dir = mct_obsidian::vault_dir(root);
        let vault = obsidian::scan(&vault_dir);
        let mut symbol_groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for (id, group) in duplicates.groups.iter().enumerate() {
            for member in &group.members {
                for symbol in symbols.get(&member.path).into_iter().flatten() {
                    let candidate = &navigation.symbols[*symbol];
                    let inside = (member.start_line..=member.end_line).contains(&candidate.line);
                    let entry = symbol_groups.entry(*symbol).or_default();
                    if inside && candidate.name == member.symbol_name && entry.last() != Some(&id) {
                        entry.push(id);
                    }
                }
            }
        }
        symbol_groups.retain(|_, ids| !ids.is_empty());
        let dead_by_symbol = dead
            .iter()
            .enumerate()
            .map(|(index, candidate)| (candidate.symbol, index))
            .collect();
        let timings = Timings {
            load_ms: started.elapsed().as_millis(),
            duplicates_ms,
            dead_code_ms,
        };
        let canonical_root = root.canonicalize()?;
        let project = project_name(&canonical_root);
        Ok(Self {
            index,
            root: canonical_root,
            no_obsidian,
            stamp,
            navigation,
            symbols,
            imports,
            importers,
            groups,
            duplicates,
            review_keys,
            numbers,
            dead,
            references,
            reviews,
            vault,
            vault_dir,
            symbol_groups,
            dead_by_symbol,
            timings,
            latency: RefCell::default(),
            project,
            generated: now_unix(),
            warning,
            csrf,
        })
    }

    /// Reloads the snapshot when the index was republished since it was read,
    /// keeping the CSRF token so an open page can still save decisions. A
    /// failed reload keeps the previous snapshot; the next reindex retries.
    fn follow_reindex(&mut self) {
        let stamp = IndexStamp::of(&self.root);
        if stamp == self.stamp {
            return;
        }
        match Site::load(&self.root, self.no_obsidian) {
            Ok(mut fresh) => {
                fresh.csrf = std::mem::take(&mut self.csrf);
                fresh.latency = std::mem::take(&mut self.latency);
                *self = fresh;
            }
            Err(error) => {
                eprintln!("warning: cannot reload the reindexed snapshot: {error:#}");
                self.stamp = stamp;
            }
        }
    }

    /// Applies one review decision sent as a JSON body (`csrf`, `key`,
    /// `state`, `note`, `revision`). `revision` is the one the page showed:
    /// if the decision changed since, nothing is saved and 409 returns the
    /// current review.
    fn review(&mut self, body: &str) -> response::Response {
        use mct_index::reviews::{ReviewError, State};
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(body) else {
            return response::Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(|v| v.as_str()) != Some(self.csrf.as_str()) {
            return response::Response::error(403);
        }
        if fields
            .keys()
            .any(|k| !["csrf", "key", "state", "note", "revision"].contains(&k.as_str()))
        {
            return response::Response::failure(400, "unknown field");
        }
        let Some(key) = fields.get("key").and_then(|v| v.as_str()) else {
            return response::Response::failure(400, "missing key");
        };
        if !self.reviews.contains(key) {
            return response::Response::error(404);
        }
        let state = match fields.get("state") {
            None | Some(serde_json::Value::Null) => State::Unreviewed,
            Some(serde_json::Value::String(value)) => match State::parse(value) {
                Some(state) => state,
                None => return response::Response::failure(400, "unknown state"),
            },
            Some(_) => return response::Response::failure(400, "invalid state"),
        };
        let Some(revision) = fields.get("revision").and_then(|v| v.as_str()) else {
            return response::Response::failure(400, "missing revision");
        };
        let note = fields.get("note").and_then(|v| v.as_str()).unwrap_or("");
        if !api::review_sources_current(self, key) {
            return response::Response::failure(
                409,
                "The source changed or is unavailable. Reindex before saving the review.",
            );
        }
        let key = key.to_string();
        match self
            .reviews
            .update(&key, state, note.trim(), revision, now_unix_millis())
        {
            Ok(()) => response::Response::json(&api::review_json(self, &key)),
            Err(ReviewError::Conflict) => {
                let mut current = api::review_json(self, &key);
                current["error"] = "This finding or its review changed meanwhile. \
                                    Nothing was saved; its current state is shown."
                    .into();
                response::Response::json(&current).with_status(409)
            }
            Err(error) => {
                eprintln!("warning: cannot save review: {error}");
                let status = match error {
                    ReviewError::Invalid(_) => 400,
                    ReviewError::Degraded(_) | ReviewError::Locked { .. } => 503,
                    _ => 500,
                };
                response::Response::failure(
                    status,
                    &format!("Could not save the review. No changes were applied. {error}"),
                )
            }
        }
    }
}

fn project_name(root: &Path) -> String {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Review decisions are timed in milliseconds (`review-decisions.toon`).
fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Dead-code candidates from the shared heuristic, mapped to snapshot
/// symbols, with their review keys.
fn dead_candidates(
    root: &Path,
    index: &Index,
    navigation: &NavigationSnapshot,
    file_hashes: &[(String, String)],
) -> anyhow::Result<(Vec<DeadCandidate>, Vec<mct_index::reviews::FindingKeys>)> {
    let mut by_location: HashMap<(&str, &str, u32), usize> = HashMap::new();
    for (id, symbol) in navigation.symbols.iter().enumerate() {
        by_location.insert((&symbol.relative_path, &symbol.name, symbol.line), id);
    }
    let scan = mct_index::scan_dead_code(index, None, None)?;
    let keys = mct_index::reviews::dead_code_findings(&scan.symbols, &scan.candidates, file_hashes);
    let mut dead = Vec::new();
    let mut findings = Vec::new();
    let mut assessor = dead_assessment::Assessor::new(root);
    for (&id, keys) in scan.candidates.iter().zip(keys) {
        let entry = &scan.symbols[id];
        // Candidates outside the snapshot (excluded files) are not shown.
        let Some(&symbol) = by_location.get(&(
            entry.relative_path.as_str(),
            entry.name.as_str(),
            entry.line,
        )) else {
            continue;
        };
        let located = &navigation.symbols[symbol];
        dead.push(DeadCandidate {
            symbol,
            key: keys.identity.clone(),
            assessment: assessor.assess(
                &located.relative_path,
                &located.language,
                &located.name,
                &located.kind,
                located.line,
            ),
        });
        findings.push(keys);
    }
    Ok((dead, findings))
}

fn random_key() -> anyhow::Result<String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| anyhow::anyhow!("cannot create private key: {error}"))?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The address to open: a readable name and no key; access control is the Host/Origin/CSRF checks.
fn dashboard_url(port: u16) -> String {
    format!("http://localhost:{port}/")
}

pub(super) fn run(root: &Path, port: Option<u16>, no_obsidian: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        root.join(".mct-index/index.sqlite3").is_file(),
        "no index found; run `mct-cli init` first"
    );
    // Bind first: a conflicting port must not regenerate anything.
    let listener = port::bind(root, port, port::PREFERRED_PORT)?;
    let port = listener.local_addr()?.port();
    let mut site = Site::load(root, no_obsidian)?;
    println!("Local project web: {}", dashboard_url(port));
    println!(
        "Dashboard ready. Snapshot: {}. A reindex is picked up on the next request; Ctrl-C stops the server.",
        site.generated
    );
    std::io::stdout().flush()?;
    for connection in listener.incoming() {
        match connection {
            Ok(mut stream) => {
                // One bounded request per connection; only review preferences are mutable.
                if let Err(error) = http::serve_connection(&mut stream, port, &mut site) {
                    eprintln!("warning: local web request failed: {error}");
                }
            }
            Err(error) => eprintln!("warning: local web connection failed: {error}"),
        }
    }
    Ok(())
}
