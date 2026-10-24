//! Local read-only views over public index APIs. No directory is served.
//! Source previews and notes are read on demand from files the index lists.

mod api;
mod http;
mod i18n;
mod markdown;
mod notes;
mod render;
mod reviews;
mod source;
mod stats;
mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_identity;
mod view_dead;
mod view_duplicates;
mod view_obsidian;
mod view_reviews;
mod view_status;

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use i18n::{t, Lang};
use mct_index::{
    DuplicateOptions, DuplicateReport, ExcludeSet, Index, IndexStatus, NavigationSnapshot,
    SymbolListEntry,
};
use reviews::{Kind, State};

/// A current duplicate group's review keys, in report order.
struct Duplicate {
    identity: String,
    fingerprint: String,
}

/// A dead-code candidate with its review keys.
struct Dead {
    entry: SymbolListEntry,
    identity: String,
    fingerprint: String,
}

struct Site {
    root: PathBuf,
    no_obsidian: bool,
    navigation: NavigationSnapshot,
    status: IndexStatus,
    stats: stats::Stats,
    symbols: BTreeMap<String, Vec<usize>>,
    imports: BTreeMap<String, Vec<String>>,
    importers: BTreeMap<String, Vec<String>>,
    groups: BTreeMap<String, Vec<usize>>,
    duplicates: DuplicateReport,
    duplicate_keys: Vec<Duplicate>,
    duplicate_ids: HashMap<String, usize>,
    dead: Vec<Dead>,
    dead_ids: HashMap<String, usize>,
    notes: notes::Notes,
    project: String,
    generated: u64,
    /// Modification time of the index files when this snapshot was taken.
    index_stamp: Option<SystemTime>,
    warning: Option<String>,
    reviews: store::Store,
    csrf: String,
    /// Wall time of the last full snapshot build, in milliseconds.
    rebuild_ms: u64,
    /// Last time the index files were compared with this snapshot.
    last_check: Instant,
    /// A snapshot being built on a worker thread; the old one keeps serving.
    rebuilding: Option<mpsc::Receiver<anyhow::Result<Site>>>,
}

/// Review counts shown in the navigation.
#[derive(Default)]
struct Counts {
    duplicates_pending: usize,
    dead_pending: usize,
    reevaluate: usize,
}

/// Newest modification time of the database and its write-ahead log.
fn index_stamp(root: &Path) -> Option<SystemTime> {
    ["index.sqlite3", "index.sqlite3-wal"]
        .iter()
        .filter_map(|name| {
            std::fs::metadata(root.join(".mct-index").join(name))
                .ok()?
                .modified()
                .ok()
        })
        .max()
}

/// Minimum time between two index-change checks (ADR-019 D3).
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl Site {
    fn load(root: &Path, no_obsidian: bool) -> anyhow::Result<Self> {
        let database = root.join(".mct-index/index.sqlite3");
        anyhow::ensure!(
            database.is_file(),
            "no index found; run `mct-cli init` first"
        );
        let started = Instant::now();
        let index = Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root))?;
        // After the open, which can create the `-wal` file; every read below
        // starts later, so a commit after this stamp only makes the snapshot
        // look stale, never fresher than it is.
        let index_stamp = index_stamp(root);
        let mut reviews = store::Store::open(root)?;
        let csrf = random_key()?;
        let warning = if no_obsidian {
            None
        } else {
            mct_obsidian::write_vault(&index, &mct_obsidian::vault_dir(root))
                .err()
                .map(|error| error.to_string())
        };
        if let Some(warning) = &warning {
            eprintln!("warning: cannot regenerate the Obsidian vault: {warning}");
        }
        let mut navigation = index.navigation_snapshot()?;
        navigation.files.retain(|file| file.language != "asset");
        navigation
            .files
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let status = index.status()?;
        let stats = stats::Stats::compute(&navigation);
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
        let duplicates = mct_index::find_duplicate_groups(
            &index,
            &mct_languages::build_token_sources(),
            DuplicateOptions::default(),
        )?;
        let old_fingerprints = reviews::legacy_group_keys(
            &duplicates.groups,
            &navigation,
            duplicates.options.normalization,
        )?;
        let mut fingerprints = reviews::Fingerprints::new(root, &navigation);
        let group_fingerprints =
            fingerprints.group_keys(&duplicates.groups, duplicates.options.normalization);
        let mut duplicate_keys: Vec<Duplicate> = reviews::group_identities(&duplicates.groups)
            .into_iter()
            .zip(group_fingerprints)
            .map(|(identity, fingerprint)| Duplicate {
                identity,
                fingerprint,
            })
            .collect();
        // A partial-block detector can emit several groups with one identity.
        // Never let a decision on one block validate another block implicitly.
        let mut identity_counts = HashMap::new();
        for key in &duplicate_keys {
            *identity_counts
                .entry(key.identity.clone())
                .or_insert(0usize) += 1;
        }
        for key in &mut duplicate_keys {
            if identity_counts[&key.identity] > 1 {
                key.fingerprint.clear();
            }
        }
        let mut duplicate_ids = HashMap::new();
        for (id, key) in duplicate_keys.iter().enumerate() {
            // Ambiguous member sets are visible but cannot validate a decision.
            duplicate_ids.entry(key.identity.clone()).or_insert(id);
        }
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (id, group) in duplicates.groups.iter().enumerate() {
            for member in &group.members {
                let ids = groups.entry(member.path.clone()).or_default();
                if ids.last() != Some(&id) {
                    ids.push(id);
                }
            }
        }
        let mut entries = mct_index::find_dead_code_candidates(&index, None, None)?;
        entries.retain(|entry| known.contains(entry.relative_path.as_str()));
        entries.sort_by(|a, b| {
            (&a.relative_path, a.line, &a.name).cmp(&(&b.relative_path, b.line, &b.name))
        });
        let old_dead_keys = reviews::legacy_dead_keys(&entries, &navigation);
        let default_dead_keys = fingerprints.dead_keys(&entries, &navigation);
        // The very reader MCP and CLI use, after the store's one-time import:
        // the dashboard and AI outputs attribute saved decisions identically.
        let saved_snapshot = mct_index::review_state::ReviewSnapshot::load(root)?;
        let resolved_dead_keys = saved_snapshot.resolve_dead(&entries, &default_dead_keys);
        let dead: Vec<Dead> = resolved_dead_keys
            .into_iter()
            .zip(entries)
            .map(|((identity, fingerprint), entry)| Dead {
                entry,
                identity,
                fingerprint,
            })
            .collect();
        let migrations: Vec<_> = duplicate_keys
            .iter()
            .zip(old_fingerprints)
            .map(|(key, old)| {
                (
                    Kind::Duplicate,
                    key.identity.clone(),
                    old,
                    key.fingerprint.clone(),
                )
            })
            .chain(dead.iter().zip(old_dead_keys).map(|(key, (_, old))| {
                (
                    Kind::Dead,
                    key.identity.clone(),
                    old,
                    key.fingerprint.clone(),
                )
            }))
            .collect();
        reviews.migrate_fingerprints(&migrations)?;
        let bindings: Vec<_> = dead
            .iter()
            .filter(|d| reviews::valid_key(&d.fingerprint))
            .map(|d| {
                (
                    d.identity.clone(),
                    mct_index::review_keys::dead_base_identity(&d.entry),
                    d.fingerprint.clone(),
                )
            })
            .collect();
        reviews.remember_dead_bindings(&bindings)?;
        let dead_ids = dead
            .iter()
            .enumerate()
            .map(|(id, dead)| (dead.identity.clone(), id))
            .collect();
        let notes = notes::Notes::load(&mct_obsidian::vault_dir(root));
        Ok(Self {
            root: root.to_path_buf(),
            no_obsidian,
            navigation,
            status,
            stats,
            symbols,
            imports,
            importers,
            groups,
            duplicates,
            duplicate_keys,
            duplicate_ids,
            dead,
            dead_ids,
            notes,
            project: root
                .file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .into_owned(),
            generated: now(),
            index_stamp,
            warning,
            reviews,
            csrf,
            rebuild_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            last_check: Instant::now(),
            rebuilding: None,
        })
    }

    /// Fingerprint of a finding in the current snapshot, if it exists.
    fn fingerprint_of(&self, kind: Kind, identity: &str) -> Option<String> {
        match kind {
            Kind::Duplicate => self
                .duplicate_ids
                .get(identity)
                .map(|id| self.duplicate_keys[*id].fingerprint.clone()),
            Kind::Dead => self
                .dead_ids
                .get(identity)
                .map(|id| self.dead[*id].fingerprint.clone()),
        }
    }

    fn remember_dead_decision(&mut self, identity: &str) {
        let Some(id) = self.dead_ids.get(identity) else {
            return;
        };
        let dead = &self.dead[*id];
        let binding = (
            dead.identity.clone(),
            mct_index::review_keys::dead_base_identity(&dead.entry),
            dead.fingerprint.clone(),
        );
        if let Err(error) = self.reviews.remember_dead_bindings(&[binding]) {
            // The decision remains safely fingerprinted; missing provenance only
            // causes conservative reevaluation until the next snapshot retries.
            eprintln!("warning: cannot retain dead symbol provenance: {error:#}");
        }
    }

    /// A decision applies only to the exact content shown to its editor.
    fn reviewed_fingerprint(
        &self,
        kind: Kind,
        identity: &str,
        expected: &str,
    ) -> Result<String, render::Response> {
        let current = self
            .fingerprint_of(kind, identity)
            .ok_or_else(|| render::Response::error(404))?;
        if !reviews::valid_key(expected)
            || current != expected
            || index_stamp(&self.root) != self.index_stamp
        {
            return Err(render::Response::error(409));
        }
        let mut fingerprints = reviews::Fingerprints::new(&self.root, &self.navigation);
        let verified = match kind {
            Kind::Dead => self.dead_ids.get(identity).and_then(|id| {
                fingerprints
                    .dead_keys(
                        std::slice::from_ref(&self.dead[*id].entry),
                        &self.navigation,
                    )
                    .pop()
                    .map(|(_, fp)| fp)
            }),
            Kind::Duplicate => self.duplicate_ids.get(identity).and_then(|id| {
                fingerprints
                    .group_keys(
                        std::slice::from_ref(&self.duplicates.groups[*id]),
                        self.duplicates.options.normalization,
                    )
                    .pop()
            }),
        };
        if verified.as_deref() != Some(current.as_str()) {
            return Err(render::Response::error(409));
        }
        Ok(current)
    }

    fn duplicate_state(&self, id: usize) -> State {
        self.duplicate_keys.get(id).map_or(State::Pending, |key| {
            self.state_or_pending(Kind::Duplicate, &key.identity, &key.fingerprint)
        })
    }

    fn dead_state(&self, id: usize) -> State {
        self.dead.get(id).map_or(State::Pending, |dead| {
            self.state_or_pending(Kind::Dead, &dead.identity, &dead.fingerprint)
        })
    }

    /// A store read error must not take the whole page down: the finding is
    /// shown as pending and the cause is logged.
    fn state_or_pending(&self, kind: Kind, identity: &str, fingerprint: &str) -> State {
        self.reviews
            .state(kind, identity, fingerprint)
            .unwrap_or_else(|error| {
                eprintln!("warning: cannot read review state: {error:#}");
                State::Pending
            })
    }

    /// Findings that are neither decided nor waiting for a new look.
    fn active_groups(&self, path: &str) -> usize {
        self.groups
            .get(path)
            .into_iter()
            .flatten()
            .filter(|id| self.duplicate_state(**id) == State::Pending)
            .count()
    }

    fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for id in 0..self.duplicate_keys.len() {
            match self.duplicate_state(id) {
                State::Pending => counts.duplicates_pending += 1,
                State::NeedsReevaluation { .. } => counts.reevaluate += 1,
                State::Decided { .. } => (),
            }
        }
        for id in 0..self.dead.len() {
            match self.dead_state(id) {
                State::Pending => counts.dead_pending += 1,
                State::NeedsReevaluation { .. } => counts.reevaluate += 1,
                State::Decided { .. } => (),
            }
        }
        counts
    }

    fn post(&mut self, target: &str, body: &str, lang: Lang) -> render::Response {
        let mut fields = BTreeMap::new();
        for (key, value) in form_urlencoded::parse(body.as_bytes()) {
            if fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return render::Response::error(400);
            }
        }
        if fields.get("csrf") != Some(&self.csrf) {
            return render::Response::error(403);
        }
        match target {
            "review" => self.review(&fields, lang),
            "review/delete" => self.delete_orphan(&fields),
            "reload" if fields.len() == 1 => self.reload(lang),
            "lang" => Self::switch_language(&fields),
            _ => render::Response::error(400),
        }
    }

    /// Swaps in a snapshot finished by the worker thread, or starts one when
    /// the index files changed (checked at most once per `REFRESH_INTERVAL`).
    /// The rebuild takes seconds on a large repository, so it never runs on
    /// the server thread: the old snapshot keeps serving and `api/version`
    /// reports it as stale. A failed rebuild keeps the old snapshot.
    fn refresh_if_stale(&mut self) {
        if let Some(receiver) = &self.rebuilding {
            match receiver.try_recv() {
                Ok(Ok(mut fresh)) => {
                    fresh.csrf = self.csrf.clone();
                    *self = fresh;
                }
                Ok(Err(error)) => {
                    eprintln!("warning: cannot refresh the analysis: {error:#}");
                    self.rebuilding = None;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => self.rebuilding = None,
            }
            return;
        }
        if self.last_check.elapsed() < REFRESH_INTERVAL {
            return;
        }
        self.last_check = Instant::now();
        if index_stamp(&self.root) == self.index_stamp {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let (root, no_obsidian) = (self.root.clone(), self.no_obsidian);
        std::thread::spawn(move || {
            // The receiver is gone when a manual reload replaced the site.
            let _ = sender.send(Self::load(&root, no_obsidian));
        });
        self.rebuilding = Some(receiver);
    }

    fn reload(&mut self, lang: Lang) -> render::Response {
        match Self::load(&self.root.clone(), self.no_obsidian) {
            Ok(mut fresh) => {
                fresh.csrf = self.csrf.clone();
                *self = fresh;
                render::Response::redirect("estado".to_string())
            }
            Err(error) => {
                eprintln!("warning: cannot reload the analysis: {error:#}");
                let mut response = render::Response::error(500);
                response.body = format!(
                    "{}\n",
                    t(
                        lang,
                        "No se pudo recargar el análisis. Se mantiene la vista anterior. Revisa el terminal y reinténtalo."
                    )
                );
                response
            }
        }
    }

    /// Remember the interface language in the browser; no server-side state.
    fn switch_language(fields: &BTreeMap<String, String>) -> render::Response {
        const ALLOWED: [&str; 3] = ["csrf", "lang", "next"];
        if fields.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
            return render::Response::error(400);
        }
        let Some(lang) = fields.get("lang").and_then(|lang| Lang::parse(lang)) else {
            return render::Response::error(400);
        };
        let next = fields
            .get("next")
            .and_then(|next| render::safe_next(next))
            .unwrap_or_else(|| "duplicados".to_string());
        let mut response = render::Response::redirect(next);
        response.set_lang = Some(lang);
        response
    }

    fn delete_orphan(&mut self, fields: &BTreeMap<String, String>) -> render::Response {
        if index_stamp(&self.root) != self.index_stamp {
            return render::Response::error(409);
        }
        if fields.len() != 4 {
            return render::Response::error(400);
        }
        let Some(expected_revision) = fields
            .get("expected_revision")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return render::Response::error(400);
        };
        let (Some(kind), Some(identity)) = (
            fields.get("kind").and_then(|k| Kind::parse(k)),
            fields.get("id"),
        ) else {
            return render::Response::error(400);
        };
        if !reviews::valid_key(identity) {
            return render::Response::error(400);
        }
        if self.fingerprint_of(kind, identity).is_some() {
            return render::Response::error(400);
        }
        match self
            .reviews
            .delete_decision_checked(kind, identity, expected_revision)
        {
            Ok(true) => render::Response::redirect("revisiones?status=obsolete".into()),
            Ok(false) => render::Response::error(404),
            Err(error) if error.downcast_ref::<store::Conflict>().is_some() => {
                render::Response::error(409)
            }
            Err(error) => {
                eprintln!("warning: cannot delete orphan decision: {error:#}");
                render::Response::error(500)
            }
        }
    }

    fn review(&mut self, fields: &BTreeMap<String, String>, lang: Lang) -> render::Response {
        const ALLOWED: [&str; 8] = [
            "csrf",
            "kind",
            "id",
            "action",
            "reason",
            "next",
            "fingerprint",
            "expected_revision",
        ];
        if fields.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
            return render::Response::error(400);
        }
        let (Some(kind), Some(identity), Some(action)) = (
            fields.get("kind").and_then(|kind| Kind::parse(kind)),
            fields.get("id"),
            fields.get("action"),
        ) else {
            return render::Response::error(400);
        };
        if !reviews::valid_key(identity) {
            return render::Response::error(400);
        }
        let Some(status) = reviews::action_status(action) else {
            return render::Response::error(400);
        };
        let Some(expected_revision) = fields
            .get("expected_revision")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return render::Response::error(400);
        };
        let Some(expected_fingerprint) = fields.get("fingerprint") else {
            return render::Response::error(400);
        };
        let fingerprint = match self.reviewed_fingerprint(kind, identity, expected_fingerprint) {
            Ok(fp) => fp,
            Err(response) => return response,
        };
        let next = fields
            .get("next")
            .and_then(|next| render::safe_next(next))
            .unwrap_or_else(|| match kind {
                Kind::Duplicate => "duplicados".to_string(),
                Kind::Dead => "muerto".to_string(),
            });
        let reason = fields.get("reason").map_or("", String::as_str);
        if let Err(error) = self.reviews.set_checked(
            (kind, identity, &fingerprint),
            status,
            reason,
            now(),
            expected_revision,
        ) {
            if error.downcast_ref::<store::Conflict>().is_some() {
                return render::Response::error(409);
            }
            eprintln!("warning: cannot save review: {error:#}");
            let mut response = render::Response::error(500);
            response.body = format!(
                "{}\n",
                t(
                    lang,
                    "No se pudo guardar la revisión. No se aplicó ningún cambio. Revisa el terminal y reinténtalo."
                )
            );
            return response;
        }
        if kind == Kind::Dead {
            self.remember_dead_decision(identity);
        }
        render::Response::redirect(next)
    }
}

fn random_key() -> anyhow::Result<String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| anyhow::anyhow!("cannot create private key: {error}"))?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The URL secret, kept in `.mct-index/web-token` so the link survives restarts.
/// An unreadable or malformed file is replaced; `rotate` forces a new one.
fn persistent_token(root: &Path, rotate: bool) -> anyhow::Result<String> {
    let path = root.join(".mct-index/web-token");
    reviews::safe_path(&path)?;
    if !rotate {
        if let Ok(saved) = std::fs::read_to_string(&path) {
            let saved = saved.trim();
            if reviews::valid_key(saved) {
                return Ok(saved.to_owned());
            }
        }
    }
    let token = random_key()?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .and_then(|mut file| file.write_all(token.as_bytes()))
        .context("cannot save the web link token")?;
    Ok(token)
}

pub(super) fn run(
    root: &Path,
    port: u16,
    no_obsidian: bool,
    rotate_token: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        root.join(".mct-index/index.sqlite3").is_file(),
        "no index found; run `mct-cli init` first"
    );
    // Bind first: a conflicting port must not regenerate anything.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .context("cannot bind the localhost web port")?;
    let port = listener.local_addr()?.port();
    let token = persistent_token(root, rotate_token)?;
    let mut site = Site::load(root, no_obsidian)?;
    println!("Local project web: http://127.0.0.1:{port}/{token}/");
    println!("Duplicates, dead code, Obsidian and reviews are linked. Snapshot: {}. After reindexing, use \"Reload analysis\" on the Status page; Ctrl-C stops the server.", site.generated);
    std::io::stdout().flush()?;
    for connection in listener.incoming() {
        match connection {
            Ok(mut stream) => {
                // One bounded request per connection; only review preferences are mutable.
                if let Err(error) = http::serve_connection(&mut stream, port, &token, &mut site) {
                    eprintln!("warning: local web request failed: {error}");
                }
            }
            Err(error) => eprintln!("warning: local web connection failed: {error}"),
        }
    }
    Ok(())
}
