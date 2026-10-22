//! Read-only view of the generated Obsidian vault (`.mct-index/obsidian`).
//!
//! Only Markdown files below the vault directory are listed. Links are the
//! Markdown/wiki links actually written in the notes and are kept only when
//! they resolve to another note of the vault; nothing is inferred.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_NOTES: usize = 20_000;
const MAX_NOTE_BYTES: u64 = 256 * 1024;
const MAX_DEPTH: usize = 16;

pub(super) struct Note {
    /// Vault-relative path with forward slashes.
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub frontmatter: Vec<(String, String)>,
    /// Resolved vault-relative paths of linked notes, sorted and deduplicated.
    pub links: Vec<String>,
    pub modified: u64,
    pub bytes: u64,
}

impl Note {
    /// Source file the note was generated for, from the `path` front-matter key.
    pub fn code_path(&self) -> Option<&str> {
        self.frontmatter
            .iter()
            .find(|(key, _)| key == "path")
            .map(|(_, value)| value.as_str())
    }

    pub fn symbol_count(&self) -> Option<usize> {
        self.frontmatter
            .iter()
            .find(|(key, _)| key == "symbol_count")
            .and_then(|(_, value)| value.parse().ok())
    }
}

pub(super) struct Vault {
    pub notes: Vec<Note>,
    /// note path -> notes that link to it.
    pub backlinks: BTreeMap<String, Vec<String>>,
    pub truncated: bool,
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `symlink_metadata`: never follow links out of the vault.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && !name.starts_with('.') {
            collect(&entry.path(), depth + 1, out);
        } else if kind.is_file() && name.ends_with(".md") && out.len() < MAX_NOTES {
            out.push(entry.path());
        }
    }
}

fn relative(path: &Path, base: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let parts: Vec<_> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

pub(super) fn read_text(path: &Path) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_NOTE_BYTES {
        return None;
    }
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_NOTE_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Splits `---` front matter from the body. Only flat `key: value` lines are read.
pub(super) fn split_frontmatter(text: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (Vec::new(), text);
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), text);
    };
    let pairs = rest[..end]
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .map_or_else(|| value.to_string(), |inner| inner.replace("\\\"", "\""));
            (key.trim().to_string(), value)
        })
        .collect();
    let body = rest[end + 4..].trim_start_matches(['\n', '\r']);
    (pairs, body)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%' && index + 2 < bytes.len())
            .then(|| u8::from_str_radix(value.get(index + 1..index + 3)?, 16).ok())
            .flatten();
        if let Some(byte) = decoded {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves a link target relative to `from` inside the vault; `None` if it escapes.
fn resolve(from: &str, target: &str) -> Option<String> {
    let target = percent_decode(target.split(['#', '?']).next().unwrap_or_default());
    if target.is_empty() || target.contains("://") || target.starts_with('/') {
        return None;
    }
    let mut parts: Vec<String> = from.split('/').map(str::to_string).collect();
    parts.pop();
    for component in Path::new(&target).components() {
        match component {
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// Markdown `](target)` links and `[[wiki]]` links of a note body.
fn raw_links(body: &str) -> (Vec<String>, Vec<String>) {
    let mut markdown = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find(')') {
            markdown.push(rest[..end].to_string());
        }
    }
    let mut wiki = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let inner = rest[..end].split(['|', '#']).next().unwrap_or_default();
            wiki.push(inner.trim().to_string());
        }
    }
    (markdown, wiki)
}

/// `#tag` tokens in a note body, outside code fences.
pub(super) fn body_tags(body: &str) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    let mut fenced = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let heading = line.trim_start();
        if fenced || heading.starts_with('#') && heading.trim_start_matches('#').starts_with(' ') {
            continue;
        }
        for word in line.split_whitespace() {
            if let Some(tag) = word.strip_prefix('#') {
                let tag = tag.trim_end_matches(|c: char| ",.;:)".contains(c));
                if !tag.is_empty()
                    && tag
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
                    && !tag.chars().all(|c| c.is_ascii_digit())
                {
                    tags.insert(tag.to_string());
                }
            }
        }
    }
    tags
}

fn title_of(frontmatter: &[(String, String)], body: &str, path: &str) -> String {
    frontmatter
        .iter()
        .find(|(key, _)| key == "title")
        .map(|(_, value)| value.clone())
        .or_else(|| {
            body.lines()
                .find_map(|line| line.strip_prefix("# "))
                .map(|heading| heading.trim().trim_matches('`').to_string())
        })
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string())
}

pub(super) fn scan(vault: &Path) -> Vault {
    let mut files = Vec::new();
    collect(vault, 0, &mut files);
    let truncated = files.len() >= MAX_NOTES;
    let mut parsed = Vec::new();
    for file in &files {
        let (Some(path), Some(text)) = (relative(file, vault), read_text(file)) else {
            continue;
        };
        let metadata = fs::metadata(file).ok();
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let (frontmatter, body) = split_frontmatter(&text);
        let mut tags: BTreeSet<String> = frontmatter
            .iter()
            .filter(|(key, _)| key == "tags")
            .flat_map(|(_, value)| {
                value
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(|tag| tag.trim().trim_start_matches('#').to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|tag| !tag.is_empty())
            .collect();
        tags.extend(body_tags(body));
        let (markdown, wiki) = raw_links(body);
        parsed.push((
            Note {
                title: title_of(&frontmatter, body, &path),
                path,
                tags: tags.into_iter().collect(),
                frontmatter,
                links: Vec::new(),
                modified,
                bytes: text.len() as u64,
            },
            markdown,
            wiki,
        ));
    }
    let known: BTreeSet<String> = parsed.iter().map(|(note, ..)| note.path.clone()).collect();
    let mut by_stem: BTreeMap<String, String> = BTreeMap::new();
    for path in &known {
        let stem = path.rsplit('/').next().unwrap_or(path);
        let stem = stem.strip_suffix(".md").unwrap_or(stem);
        by_stem
            .entry(stem.to_string())
            .or_insert_with(|| path.clone());
    }
    let mut backlinks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut notes = Vec::new();
    for (mut note, markdown, wiki) in parsed {
        let mut links: BTreeSet<String> = markdown
            .iter()
            .filter_map(|target| resolve(&note.path, target))
            .filter(|target| known.contains(target))
            .collect();
        links.extend(wiki.iter().filter_map(|name| by_stem.get(name).cloned()));
        links.remove(&note.path);
        for target in &links {
            backlinks
                .entry(target.clone())
                .or_default()
                .push(note.path.clone());
        }
        note.links = links.into_iter().collect();
        notes.push(note);
    }
    Vault {
        notes,
        backlinks,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn links_resolve_inside_the_vault_only() {
        assert_eq!(
            resolve("a/b.md", "../c/d%20e.md#x").as_deref(),
            Some("c/d e.md")
        );
        assert_eq!(resolve("a.md", "../../etc/passwd"), None);
        assert_eq!(resolve("a.md", "https://example.com/x.md"), None);
        assert_eq!(resolve("a.md", "/abs.md"), None);
    }

    #[test]
    fn frontmatter_tags_and_links_are_read_without_inference() {
        let (pairs, body) = split_frontmatter("---\ntitle: \"A\"\ntags: [x, y]\n---\n# H\n#z text");
        assert_eq!(pairs[0], ("title".into(), "A".into()));
        assert!(body.starts_with("# H"));
        assert_eq!(body_tags(body), BTreeSet::from(["z".to_string()]));
        let (markdown, wiki) = raw_links("[l](b.md) and [[c|alias]]");
        assert_eq!((markdown, wiki), (vec!["b.md".into()], vec!["c".into()]));
    }
}
