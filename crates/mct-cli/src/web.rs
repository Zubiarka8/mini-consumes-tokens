//! Local read-only views over public index APIs. No directory is served.

mod api;
mod assets;
mod http;
mod obsidian;
mod response;
mod reviews;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use mct_index::{DuplicateOptions, DuplicateReport, ExcludeSet, Index, NavigationSnapshot};

struct DeadCandidate {
    /// Index into `navigation.symbols`.
    symbol: usize,
    key: String,
}

#[derive(Default)]
struct Timings {
    load_ms: u128,
    duplicates_ms: u128,
    dead_code_ms: u128,
}

struct Site {
    index: Index,
    root: PathBuf,
    navigation: NavigationSnapshot,
    symbols: BTreeMap<String, Vec<usize>>,
    imports: BTreeMap<String, Vec<String>>,
    importers: BTreeMap<String, Vec<String>>,
    groups: BTreeMap<String, Vec<usize>>,
    duplicates: DuplicateReport,
    review_keys: Vec<String>,
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
        let started = Instant::now();
        let database = root.join(".mct-index/index.sqlite3");
        anyhow::ensure!(
            database.is_file(),
            "no index found; run `mct-cli init` first"
        );
        let index = Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root))?;
        let reviews = reviews::Reviews::load(root)?;
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
        let mut navigation = index.navigation_snapshot()?;
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
        let review_keys = reviews::group_keys(
            &duplicates.groups,
            &navigation,
            duplicates.options.normalization,
        )?;
        for (id, group) in duplicates.groups.iter().enumerate() {
            for member in &group.members {
                let ids = groups.entry(member.path.clone()).or_default();
                if ids.last() != Some(&id) {
                    ids.push(id);
                }
            }
        }
        let dead_started = Instant::now();
        let dead = dead_candidates(&index, &navigation)?;
        let dead_code_ms = dead_started.elapsed().as_millis();
        let references = index.reference_counts()?;
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
        Ok(Self {
            index,
            root: root.canonicalize()?,
            navigation,
            symbols,
            imports,
            importers,
            groups,
            duplicates,
            review_keys,
            dead,
            references,
            reviews,
            vault,
            vault_dir,
            symbol_groups,
            dead_by_symbol,
            timings,
            latency: RefCell::default(),
            project: root
                .file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .into_owned(),
            generated: now_unix(),
            warning,
            csrf,
        })
    }

    /// Applies one review decision sent as a JSON body (`csrf`, `key`, `state`, `note`).
    fn review(&mut self, body: &str) -> response::Response {
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(body) else {
            return response::Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(|v| v.as_str()) != Some(self.csrf.as_str()) {
            return response::Response::error(403);
        }
        if fields.keys().any(|k| !["csrf", "key", "state", "note"].contains(&k.as_str())) {
            return response::Response::failure(400, "unknown field");
        }
        let Some(key) = fields.get("key").and_then(|v| v.as_str()) else {
            return response::Response::failure(400, "missing key");
        };
        if !self.review_keys.iter().any(|k| k == key) && !self.dead.iter().any(|d| d.key == key) {
            return response::Response::error(404);
        }
        let state = match fields.get("state") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(value)) => match reviews::State::parse(value) {
                Some(state) => Some(state),
                None => return response::Response::failure(400, "unknown state"),
            },
            Some(_) => return response::Response::failure(400, "invalid state"),
        };
        let note = fields.get("note").and_then(|v| v.as_str()).unwrap_or("");
        let key = key.to_string();
        if let Err(error) = self.reviews.update(&key, state, note.trim(), now_unix()) {
            eprintln!("warning: cannot save review: {error:#}");
            return response::Response::failure(
                500,
                "Could not save the review. No changes were applied.",
            );
        }
        response::Response::json(&api::review_json(self, &key))
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Dead-code candidates from the shared heuristic, mapped to snapshot symbols.
fn dead_candidates(
    index: &Index,
    navigation: &NavigationSnapshot,
) -> anyhow::Result<Vec<DeadCandidate>> {
    let mut by_location: HashMap<(&str, &str, u32), usize> = HashMap::new();
    for (id, symbol) in navigation.symbols.iter().enumerate() {
        by_location.insert((&symbol.relative_path, &symbol.name, symbol.line), id);
    }
    let mut dead = Vec::new();
    for entry in mct_index::find_dead_code_candidates(index, None, None)? {
        let Some(&symbol) = by_location.get(&(
            entry.relative_path.as_str(),
            entry.name.as_str(),
            entry.line,
        )) else {
            continue;
        };
        if let Some(key) = reviews::dead_code_key(&navigation.symbols[symbol], navigation) {
            dead.push(DeadCandidate { symbol, key });
        }
    }
    Ok(dead)
}

fn random_key() -> anyhow::Result<String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| anyhow::anyhow!("cannot create private key: {error}"))?;
    Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn run(root: &Path, port: u16, no_obsidian: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        root.join(".mct-index/index.sqlite3").is_file(),
        "no index found; run `mct-cli init` first"
    );
    // Bind first: a conflicting port must not regenerate anything.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .context("cannot bind the localhost web port")?;
    let port = listener.local_addr()?.port();
    let token = random_key()?;
    let mut site = Site::load(root, no_obsidian)?;
    println!("Local project web: http://127.0.0.1:{port}/{token}/");
    println!("Dashboard ready. Snapshot: {}. Restart after reindexing; Ctrl-C stops the server.", site.generated);
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
