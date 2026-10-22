//! Local read-only views over public index APIs. No directory is served.
//! Source previews and notes are read on demand from files the index lists.

mod http;
mod i18n;
mod markdown;
mod notes;
mod render;
mod reviews;
mod source;
mod stats;
#[cfg(test)]
mod tests;
mod view_dead;
mod view_duplicates;
mod view_obsidian;
mod view_reviews;
mod view_status;

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use i18n::{t, Lang};
use mct_index::{
    DuplicateOptions, DuplicateReport, ExcludeSet, Index, IndexStatus, NavigationSnapshot,
    SymbolListEntry,
};
use reviews::{Kind, State, Status};

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
    reviews: reviews::Reviews,
    csrf: String,
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
        let index_stamp = index_stamp(root);
        let index = Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root))?;
        let reviews = reviews::Reviews::load(root)?;
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
        let fingerprints = reviews::group_keys(
            &duplicates.groups,
            &navigation,
            duplicates.options.normalization,
        )?;
        let duplicate_keys: Vec<Duplicate> = reviews::group_identities(&duplicates.groups)
            .into_iter()
            .zip(fingerprints)
            .map(|(identity, fingerprint)| Duplicate {
                identity,
                fingerprint,
            })
            .collect();
        let mut duplicate_ids = HashMap::new();
        for (id, key) in duplicate_keys.iter().enumerate() {
            // Two groups can share an identity only if the detector split one
            // member set; the first keeps the identity and the others stay pending.
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
        let dead: Vec<Dead> = reviews::dead_keys(&entries, &navigation)
            .into_iter()
            .zip(entries)
            .map(|((identity, fingerprint), entry)| Dead {
                entry,
                identity,
                fingerprint,
            })
            .collect();
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
        })
    }

    fn duplicate_state(&self, id: usize) -> State {
        self.duplicate_keys.get(id).map_or(State::Pending, |key| {
            self.reviews
                .state(Kind::Duplicate, &key.identity, &key.fingerprint)
        })
    }

    fn dead_state(&self, id: usize) -> State {
        self.dead.get(id).map_or(State::Pending, |dead| {
            self.reviews
                .state(Kind::Dead, &dead.identity, &dead.fingerprint)
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
            "reload" if fields.len() == 1 => self.reload(lang),
            "lang" => Self::switch_language(&fields),
            _ => render::Response::error(400),
        }
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

    fn review(&mut self, fields: &BTreeMap<String, String>, lang: Lang) -> render::Response {
        const ALLOWED: [&str; 6] = ["csrf", "kind", "id", "action", "reason", "next"];
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
        let status = match action.as_str() {
            "restore" => Status::Pending,
            other => match Status::parse(other) {
                Some(Status::Pending) | None => return render::Response::error(400),
                Some(status) => status,
            },
        };
        let fingerprint = match kind {
            Kind::Duplicate => self
                .duplicate_ids
                .get(identity)
                .map(|id| self.duplicate_keys[*id].fingerprint.clone()),
            Kind::Dead => self
                .dead_ids
                .get(identity)
                .map(|id| self.dead[*id].fingerprint.clone()),
        };
        let Some(fingerprint) = fingerprint else {
            return render::Response::error(404);
        };
        let next = fields
            .get("next")
            .and_then(|next| render::safe_next(next))
            .unwrap_or_else(|| match kind {
                Kind::Duplicate => "duplicados".to_string(),
                Kind::Dead => "muerto".to_string(),
            });
        let reason = fields.get("reason").map_or("", String::as_str);
        if let Err(error) = self
            .reviews
            .set((kind, identity, &fingerprint), status, reason, now())
        {
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
        render::Response::redirect(next)
    }
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
