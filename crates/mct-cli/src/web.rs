//! Local read-only views over public index APIs. No directory is served.

mod http;
mod render;
mod reviews;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use mct_index::{DuplicateOptions, DuplicateReport, ExcludeSet, Index, NavigationSnapshot};

struct Site {
    navigation: NavigationSnapshot,
    symbols: BTreeMap<String, Vec<usize>>,
    imports: BTreeMap<String, Vec<String>>,
    importers: BTreeMap<String, Vec<String>>,
    groups: BTreeMap<String, Vec<usize>>,
    duplicates: DuplicateReport,
    project: String,
    generated: u64,
    warning: Option<String>,
    reviews: reviews::Reviews,
    review_keys: Vec<String>,
    csrf: String,
}

impl Site {
    fn load(root: &Path, no_obsidian: bool) -> anyhow::Result<Self> {
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
        let duplicates = mct_index::find_duplicate_groups(
            &index,
            &mct_languages::build_token_sources(),
            DuplicateOptions::default(),
        )?;
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
        Ok(Self {
            navigation,
            symbols,
            imports,
            importers,
            groups,
            duplicates,
            project: root
                .file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .into_owned(),
            generated: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            warning,
            reviews,
            review_keys,
            csrf,
        })
    }

    fn dismissed(&self, id: usize) -> bool {
        self.review_keys
            .get(id)
            .is_some_and(|key| self.reviews.reason(key).is_some())
    }

    fn active_groups(&self, path: &str) -> usize {
        self.groups
            .get(path)
            .into_iter()
            .flatten()
            .filter(|id| !self.dismissed(**id))
            .count()
    }

    fn review(&mut self, body: &str) -> render::Response {
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
        if fields.len() != 3 {
            return render::Response::error(400);
        }
        let Some(key) = fields
            .get("key")
            .filter(|key| self.review_keys.contains(key))
        else {
            return render::Response::error(404);
        };
        let (reason, location) = match fields.get("action").map(String::as_str) {
            Some("restore") => (None, "duplicates?view=discarded"),
            Some(action) => match reviews::Reason::parse(action) {
                Some(reason) => (Some(reason), "duplicates"),
                None => return render::Response::error(400),
            },
            None => return render::Response::error(400),
        };
        if let Err(error) = self.reviews.update(key, reason) {
            eprintln!("warning: cannot save duplicate review: {error:#}");
            let mut response = render::Response::error(500);
            response.body = "Could not save the review. No changes were applied. Check the terminal diagnostics and retry.\n".to_string();
            return response;
        }
        render::Response {
            status: 303,
            content_type: "text/plain; charset=utf-8",
            body: String::new(),
            location: Some(location),
        }
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
    println!("Atlas and duplicates are linked. Snapshot: {}. Restart after reindexing; Ctrl-C stops the server.", site.generated);
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
