//! Local read-only views over public index APIs. No directory is served.

mod api;
mod areas;
mod assets;
mod autoreindex;
mod dead_assessment;
mod http;
mod idle;
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
    /// The group of the project this candidate is listed under (see `areas`).
    area: String,
    /// What the index knows of a framework possibly loading it (never a
    /// claim that it does); empty for almost every candidate.
    possible_entry_points: Vec<String>,
}

#[derive(Default)]
struct Timings {
    load_ms: u128,
    duplicates_ms: u128,
    dead_code_ms: u128,
}

/// What identifies one published state of the index: the database file (a
/// rebuild replaces it) and the generation published in it. Only a reindex
/// that changed something advances the generation, so checkpoints, metadata
/// and runs that found nothing to change never trigger a reload.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
struct IndexStamp {
    file: Option<FileIdentity>,
    generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FileIdentity(u64, u64);

impl FileIdentity {
    fn of(root: &Path) -> Option<Self> {
        let meta = std::fs::metadata(root.join(".mct-index/index.sqlite3")).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(Self(meta.dev(), meta.ino()))
        }
        #[cfg(not(unix))]
        {
            let created = meta.created().ok()?.duration_since(UNIX_EPOCH).ok()?;
            Some(Self(created.as_secs(), u64::from(created.subsec_nanos())))
        }
    }
}

/// A snapshot being built on another thread while the current one answers.
struct PendingReload {
    /// The stamp to record if building fails, as by `Site::reload`.
    failed: IndexStamp,
    /// Decisions saved when building started.
    writes: u64,
    result: std::sync::mpsc::Receiver<anyhow::Result<Box<Site>>>,
}

struct Site {
    index: Index,
    root: PathBuf,
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
    /// Shared with the thread that builds the next snapshot, which reuses its
    /// unchanged notes.
    vault: std::sync::Arc<obsidian::Vault>,
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
        Self::load_snapshot(root, no_obsidian, None, || {})
    }

    // The callback lets integration tests publish between the navigation query
    // and the remaining queries, without timing-dependent threads or sleeps.
    // `previous_vault` lets the vault scan reuse notes whose file is unchanged.
    fn load_snapshot(
        root: &Path,
        no_obsidian: bool,
        previous_vault: Option<&obsidian::Vault>,
        after_navigation: impl FnOnce(),
    ) -> anyhow::Result<Self> {
        let started = Instant::now();
        let file = FileIdentity::of(root);
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
        let stamp = IndexStamp {
            file,
            generation: snapshot.generation(),
        };
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
        let reviews = reviews::Reviews::load(root, findings);
        let numbers = numbers::Numbers::load(root, reviews.findings(), &reviews.reviewed_keys());
        if let Some(reason) = reviews.read_only_reason() {
            eprintln!("warning: review decisions are read-only until this is fixed: {reason}");
        }
        let references = index.reference_counts()?;
        drop(snapshot);
        let vault_dir = mct_obsidian::vault_dir(root);
        let vault = obsidian::scan(&vault_dir, previous_vault);
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
            vault: std::sync::Arc::new(vault),
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
    #[cfg(test)]
    fn follow_reindex(&mut self) {
        if let Some(failed) = self.stale() {
            self.reload(failed);
        }
    }

    /// Whether the index was republished since this snapshot was read, and if
    /// so the stamp to record when loading the new state fails.
    fn stale(&self) -> Option<IndexStamp> {
        // A replaced database file (a rebuild) is a change whatever the open
        // connection still reads; it is retried on every request until it
        // loads, since a failure there is an early open error. Otherwise one
        // indexed read of the published generation decides, and a failed
        // reload of a generation waits for the next one.
        let file = FileIdentity::of(&self.root);
        if file != self.stamp.file {
            return Some(self.stamp);
        }
        match self.index.published_generation() {
            Ok(generation) if generation == self.stamp.generation => None,
            Ok(generation) => Some(IndexStamp { file, generation }),
            Err(error) => {
                eprintln!("warning: cannot read the index generation: {error:#}");
                None
            }
        }
    }

    /// Replaces the snapshot with a fresh one, or records `failed` as the
    /// stamp already tried.
    #[cfg(test)]
    fn reload(&mut self, failed: IndexStamp) {
        // The process that published the new generation also published the
        // vault from its post-reindex hook; doing it again here doubled the
        // work and competed for the writer lock.
        match Site::load_snapshot(&self.root, true, Some(&self.vault), || {}) {
            Ok(fresh) => self.adopt(fresh),
            Err(error) => self.reload_failed(&error, failed),
        }
    }

    /// Like `follow_reindex`, without waiting: the new snapshot is built on
    /// another thread and swapped in by a later call once it is ready; until
    /// then this one keeps answering. `writes` counts the decisions saved so
    /// far: a snapshot started before the latest one may lack it, so it is
    /// discarded and built again.
    fn follow_reindex_in_background(&mut self, pending: &mut Option<PendingReload>, writes: u64) {
        if let Some(reload) = pending.take() {
            match reload.result.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    *pending = Some(reload);
                    return;
                }
                Ok(Ok(fresh)) if reload.writes == writes => {
                    self.adopt(*fresh);
                    return;
                }
                // Built before a decision was saved: build it again below.
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    self.reload_failed(&error, reload.failed);
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let error = anyhow::anyhow!("the loading thread stopped");
                    self.reload_failed(&error, reload.failed);
                    return;
                }
            }
        }
        if let Some(failed) = self.stale() {
            let (send, result) = std::sync::mpsc::channel();
            // As in `reload`: the reindexer already published the vault.
            let root = self.root.clone();
            let previous = std::sync::Arc::clone(&self.vault);
            let loading = std::thread::Builder::new()
                .name("snapshot-reload".into())
                .spawn(move || {
                    let fresh = Site::load_snapshot(&root, true, Some(&previous), || {});
                    let _ = send.send(fresh.map(Box::new));
                });
            match loading {
                Ok(_) => {
                    *pending = Some(PendingReload {
                        failed,
                        writes,
                        result,
                    });
                }
                Err(error) => {
                    let error =
                        anyhow::Error::from(error).context("cannot start the loading thread");
                    self.reload_failed(&error, failed);
                }
            }
        }
    }

    /// Takes `fresh` as the snapshot, keeping the CSRF token and the latency
    /// samples of the open session.
    fn adopt(&mut self, mut fresh: Site) {
        fresh.csrf = std::mem::take(&mut self.csrf);
        fresh.latency = std::mem::take(&mut self.latency);
        *self = fresh;
    }

    fn reload_failed(&mut self, error: &anyhow::Error, failed: IndexStamp) {
        eprintln!("warning: cannot reload the reindexed snapshot: {error:#}");
        self.stamp = failed;
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
            Ok(()) => {
                let reviewed = self
                    .reviews
                    .resolution(&key)
                    .is_some_and(|resolution| resolution.state() != State::Unreviewed);
                self.numbers.set_reviewed(&key, reviewed);
                response::Response::json(&api::review_json(self, &key))
            }
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
            possible_entry_points: scan
                .possible_entry_points(id)
                .iter()
                .map(mct_index::EntryPointNote::describe)
                .collect(),
            assessment: assessor.assess(
                &located.relative_path,
                &located.language,
                &located.name,
                &located.kind,
                located.line,
            ),
            area: String::new(),
        });
        findings.push(keys);
    }
    // The areas depend on where all the candidates are, so they are set last.
    let paths: Vec<&str> = dead
        .iter()
        .map(|candidate| navigation.symbols[candidate.symbol].relative_path.as_str())
        .collect();
    let areas = areas::assign(&paths);
    for (candidate, area) in dead.iter_mut().zip(areas) {
        candidate.area = area;
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

/// `idle_timeout`: stop after this long without a request (`None`: never).
pub(super) fn run(
    root: &Path,
    port: Option<u16>,
    no_obsidian: bool,
    idle_timeout: Option<std::time::Duration>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        root.join(".mct-index/index.sqlite3").is_file(),
        "no index found; run `mct-cli init` first"
    );
    // Bind first: a conflicting port must not regenerate anything.
    let listener = port::bind(root, port, port::PREFERRED_PORT)?;
    let port = listener.local_addr()?.port();
    let site = Site::load(root, no_obsidian)?;
    println!("Local project web: {}", dashboard_url(port));
    // Kept for the server's lifetime: dropping it would stop the watch.
    let _autoreindex = autoreindex::start(root, no_obsidian);
    println!(
        "Dashboard ready. Snapshot: {}. Saved files and .mctignore edits are reindexed automatically and picked up on the next request; Ctrl-C stops the server.",
        site.generated
    );
    std::io::stdout().flush()?;
    // Always watching: the settings page can turn the limit on or change it later.
    let activity = idle::Activity::with_timeout(idle_timeout);
    idle::watch(activity.clone(), || {
        println!("No use of the dashboard for the set time: stopping it.");
        let _ = std::io::stdout().flush();
        // The lease and file locks are released by the OS, and SQLite rolls
        // back a reindex cut short, as after Ctrl-C.
        std::process::exit(0);
    });
    serve(listener, port, site, &activity)?;
    Ok(())
}

/// Connections read at the same time; more are refused with 503.
const MAX_OPEN_CONNECTIONS: usize = 64;

/// One complete request, waiting for the thread that owns the site.
struct Job {
    request: http::Complete,
    reply: std::sync::mpsc::Sender<response::Response>,
}

/// Serves `site` on `listener` for as long as the listener accepts.
///
/// Each connection is read and answered on its own thread, so a client that
/// is slow to send or to read (a preconnect, a stalled tab) delays nobody but
/// itself. Complete requests are answered one at a time by this thread, the
/// only owner of `site`: snapshot reloads and review decisions stay serial.
fn serve(
    listener: std::net::TcpListener,
    port: u16,
    mut site: Site,
    activity: &idle::Activity,
) -> std::io::Result<()> {
    let (jobs, queue) = std::sync::mpsc::channel::<Job>();
    let open = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    std::thread::Builder::new()
        .name("web-accept".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => accept(stream, port, &jobs, &open),
                    Err(error) => eprintln!("warning: local web connection failed: {error}"),
                }
            }
        })?;
    let mut pending = None;
    let mut writes = 0;
    for job in queue {
        let _serving = if job.request.is_passive() {
            activity.passive()
        } else {
            activity.serving()
        };
        // A request never waits for a reload; the next one after it is ready
        // sees the new snapshot.
        site.follow_reindex_in_background(&mut pending, writes);
        writes += u64::from(job.request.is_post());
        // The connection may have gone; its thread then writes nothing.
        let _ = job.reply.send(job.request.answer(&mut site, activity));
    }
    Ok(())
}

/// One of the `MAX_OPEN_CONNECTIONS` slots, given back when dropped, so a
/// connection thread that panics still frees its slot.
struct OpenSlot(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl OpenSlot {
    /// A slot of `open`, or `None` when `limit` are already taken.
    fn claim(open: &std::sync::Arc<std::sync::atomic::AtomicUsize>, limit: usize) -> Option<Self> {
        use std::sync::atomic::Ordering;
        if open.fetch_add(1, Ordering::SeqCst) >= limit {
            open.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(open.clone()))
    }
}

impl Drop for OpenSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Hands one accepted connection to a thread of its own, or refuses it when
/// `MAX_OPEN_CONNECTIONS` are already open.
fn accept(
    mut stream: std::net::TcpStream,
    port: u16,
    jobs: &std::sync::mpsc::Sender<Job>,
    open: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let Some(slot) = OpenSlot::claim(open, MAX_OPEN_CONNECTIONS) else {
        // Best effort, never blocking the accept loop for long.
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(100)));
        let _ = http::write_response(&mut stream, &response::Response::error(503), false);
        return;
    };
    let jobs = jobs.clone();
    let spawned = std::thread::Builder::new().spawn(move || {
        let _slot = slot;
        if let Err(error) = answer_connection(&mut stream, port, &jobs) {
            eprintln!("warning: local web request failed: {error}");
        }
    });
    // Without a thread the connection and its slot are dropped: the client
    // sees the connection close.
    if let Err(error) = spawned {
        eprintln!("warning: cannot start a local web connection thread: {error}");
    }
}

/// Reads one request, has the site thread answer it, and writes the answer.
fn answer_connection(
    stream: &mut std::net::TcpStream,
    port: u16,
    jobs: &std::sync::mpsc::Sender<Job>,
) -> std::io::Result<()> {
    http::Connection::write_timeout(stream, http::REQUEST_TIMEOUT)?;
    let (incoming, head) = http::read_request(stream, port)?;
    let response = match incoming {
        Ok(request) => {
            let (reply, answer) = std::sync::mpsc::channel();
            if jobs.send(Job { request, reply }).is_err() {
                return Ok(());
            }
            match answer.recv() {
                Ok(response) => response,
                Err(_) => return Ok(()),
            }
        }
        Err(response) => response,
    };
    http::write_response(stream, &response, head)
}
