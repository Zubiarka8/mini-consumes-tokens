//! The generated Obsidian vault as read by the web (ADR-018 D5/D7).
//!
//! The note list is the vault manifest, enumerated once. A request names an
//! entry of that list by its index-independent relative name; no request text
//! ever becomes a path. Contents are read from the vault directory through the
//! same symlink-refusing resolver as source previews, within fixed bounds.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::source;

const MAX_NOTES: usize = 50_000;
const MAX_NOTE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

pub(super) struct Note {
    /// Vault-relative name, forward slashes, ends in `.md`.
    pub name: String,
    pub title: String,
    /// Project file this note describes (front matter `path`), if any.
    pub source: Option<String>,
    /// Raw Markdown, empty if unreadable or over the memory budget.
    pub text: String,
    /// Notes this one links to, as indexes into [`Notes::list`], sorted, unique.
    pub links: Vec<usize>,
}

#[derive(Default)]
pub(super) struct Notes {
    pub list: Vec<Note>,
    by_name: HashMap<String, usize>,
    by_source: HashMap<String, usize>,
    /// Notes linking to each note.
    pub backlinks: Vec<Vec<usize>>,
    pub link_count: usize,
}

/// `base`'s directory joined with `target`, with `.` and `..` folded; `None` if it leaves the vault.
pub(super) fn join(base: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Link targets in `text`: `](target)` and `[[target]]`/`[[target|label]]`, fragments removed.
fn targets(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        if let Some(end) = rest.find(')') {
            found.push(rest[..end].split('#').next().unwrap_or("").to_string());
            rest = &rest[end..];
        }
    }
    let mut rest = text;
    while let Some(at) = rest.find("[[") {
        rest = &rest[at + 2..];
        if let Some(end) = rest.find("]]") {
            let inner = rest[..end].split('|').next().unwrap_or("");
            found.push(inner.split('#').next().unwrap_or("").to_string());
            rest = &rest[end..];
        }
    }
    found
}

impl Notes {
    pub fn load(vault: &Path) -> Self {
        let Some(manifest) = mct_obsidian::Manifest::load(vault) else {
            return Self::default();
        };
        let mut notes = Self::default();
        let mut total = 0;
        for name in manifest.notes.keys().take(MAX_NOTES) {
            if !name.ends_with(".md") {
                continue;
            }
            let mut text = if total < MAX_TOTAL_BYTES {
                source::read_text(vault, name, MAX_NOTE_BYTES).unwrap_or_default()
            } else {
                String::new()
            };
            total += text.len();
            let (front, _) = super::markdown::split_front_matter(&text);
            let find = |key: &str| {
                front
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.clone())
            };
            let title = find("title").unwrap_or_else(|| name.clone());
            let source_path = find("path");
            if text.is_empty() {
                text.shrink_to_fit();
            }
            notes.by_name.insert(name.clone(), notes.list.len());
            if let Some(path) = &source_path {
                notes
                    .by_source
                    .entry(path.clone())
                    .or_insert(notes.list.len());
            }
            notes.list.push(Note {
                name: name.clone(),
                title,
                source: source_path,
                text,
                links: Vec::new(),
            });
        }
        notes.backlinks = vec![Vec::new(); notes.list.len()];
        let mut edges: Vec<(usize, Vec<usize>)> = Vec::new();
        for (id, note) in notes.list.iter().enumerate() {
            let mut links: Vec<usize> = targets(&note.text)
                .iter()
                .filter_map(|target| notes.resolve(&note.name, target))
                .filter(|target| *target != id)
                .collect();
            links.sort_unstable();
            links.dedup();
            edges.push((id, links));
        }
        for (id, links) in edges {
            notes.link_count += links.len();
            for target in &links {
                notes.backlinks[*target].push(id);
            }
            notes.list[id].links = links;
        }
        notes
    }

    pub fn get(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    pub fn for_source(&self, path: &str) -> Option<usize> {
        self.by_source.get(path).copied()
    }

    /// Resolve a Markdown link target found in note `from` to a listed note.
    pub fn resolve(&self, from: &str, target: &str) -> Option<usize> {
        let target = target.trim();
        if target.is_empty() || target.contains(['\\', ':', '\0']) {
            return None;
        }
        let relative = join(from, target)?;
        let rooted = target.trim_start_matches("./");
        [
            Some(relative.clone()),
            Some(format!("{relative}.md")),
            Some(rooted.to_string()),
            Some(format!("{rooted}.md")),
        ]
        .into_iter()
        .flatten()
        .find_map(|name| self.get(&name))
    }

    /// Case-insensitive search of titles, names and contents; at most `limit` ids.
    pub fn search(&self, query: &str, limit: usize) -> Vec<usize> {
        let query = query.to_lowercase();
        let mut scored: BTreeMap<(u8, usize), usize> = BTreeMap::new();
        for (id, note) in self.list.iter().enumerate() {
            let rank = if note.title.to_lowercase().contains(&query) {
                0
            } else if note.name.to_lowercase().contains(&query) {
                1
            } else if note.text.to_lowercase().contains(&query) {
                2
            } else {
                continue;
            };
            scored.insert((rank, id), id);
        }
        scored.into_values().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn join_folds_dots_and_refuses_to_leave_the_vault() {
        assert_eq!(
            join("files/a.md", "../index.md").as_deref(),
            Some("index.md")
        );
        assert_eq!(
            join("index.md", "files/b.md").as_deref(),
            Some("files/b.md")
        );
        assert_eq!(join("index.md", "./x/./y.md").as_deref(), Some("x/y.md"));
        assert_eq!(join("a.md", "../x.md"), None);
        assert_eq!(join("a/b.md", "../../x.md"), None);
    }

    #[test]
    fn link_targets_are_found_in_both_syntaxes() {
        let found = targets("see [a](x.md#top), [[y|why]] and [[z#h]] [no](");
        assert_eq!(found, ["x.md", "y", "z"]);
    }
}
