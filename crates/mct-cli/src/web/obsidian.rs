//! Read-only view of the generated Obsidian vault (`.mct-index/obsidian`).
//!
//! Only Markdown files below the vault directory are listed. Links are the
//! Markdown/wiki links actually written in the notes and are kept only when
//! they resolve to another note of the vault; nothing is inferred.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Component, Path};

const MAX_NOTES: usize = 20_000;
const MAX_NOTE_BYTES: u64 = 256 * 1024;
const MAX_DEPTH: usize = 16;

#[derive(Clone)]
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
    /// Text after the front matter, kept so searches read no file.
    body: Box<str>,
    /// File size and modification time when read: a rescan reuses the note
    /// while they are unchanged.
    stamp: Option<Stamp>,
    /// Link targets as written, resolved again on every scan since the
    /// notes they point to may have changed.
    markdown: Vec<String>,
    wiki: Vec<String>,
}

/// What a rescan compares to decide whether a note file changed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: std::time::SystemTime,
    len: u64,
}

/// A file whose modification time is this close to a scan may have been
/// written again within the timestamp's resolution: it is always reread.
const RACY: std::time::Duration = std::time::Duration::from_secs(2);

impl Stamp {
    fn of(file: &Path) -> Option<Self> {
        let metadata = fs::symlink_metadata(file).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }

    /// True when the file had last changed well before `scan` started.
    fn settled_before(&self, scan: std::time::SystemTime) -> bool {
        self.modified
            .checked_add(RACY)
            .is_some_and(|settled| settled < scan)
    }
}

impl Note {
    /// The first body line containing `query_lower` (already lower-cased),
    /// trimmed: the line that `contains_ci` would match.
    pub fn body_line(&self, query_lower: &str) -> Option<&str> {
        if query_lower.is_empty() || query_lower.contains(['\n', '\r']) {
            return None;
        }
        if query_lower.is_ascii() && self.body.is_ascii() {
            // The common case, without allocating: for ASCII text, lower-casing
            // and ASCII case folding agree.
            let at = find_ascii_ci(self.body.as_bytes(), query_lower.as_bytes())?;
            let line = self.body.as_bytes()[..at]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            return self.body.lines().nth(line).map(str::trim);
        }
        self.body
            .lines()
            .find(|line| {
                if query_lower.is_ascii() && line.is_ascii() {
                    find_ascii_ci(line.as_bytes(), query_lower.as_bytes()).is_some()
                } else {
                    // Some non-ASCII letters lower-case to ASCII (the Kelvin
                    // sign to `k`): only full lower-casing is exact here.
                    line.to_lowercase().contains(query_lower)
                }
            })
            .map(str::trim)
    }

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
    /// Files that look like notes but were not read, by reason
    /// (`tooLarge`, `notUtf8`, `unreadable`, `extension`).
    pub skipped: BTreeMap<&'static str, usize>,
    /// When this scan started.
    scanned_at: std::time::SystemTime,
}

/// Offset of the first ASCII case-insensitive occurrence of `needle_lower`
/// (lower-case ASCII) in `haystack`.
fn find_ascii_ci(haystack: &[u8], needle_lower: &[u8]) -> Option<usize> {
    let (&first, rest) = needle_lower.split_first()?;
    let upper = first.to_ascii_uppercase();
    let last_start = haystack.len().checked_sub(needle_lower.len())?;
    let mut from = 0;
    while from <= last_start {
        let at = from
            + haystack[from..=last_start]
                .iter()
                .position(|&b| b == first || b == upper)?;
        if haystack[at + 1..at + needle_lower.len()].eq_ignore_ascii_case(rest) {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Visits the files below `dir` in name order, skipping hidden directories,
/// until `visit` breaks.
fn walk(
    dir: &Path,
    depth: usize,
    visit: &mut dyn FnMut(&Path, &str) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if depth > MAX_DEPTH {
        return ControlFlow::Continue(());
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return ControlFlow::Continue(());
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `file_type` does not follow links: never leave the vault through one.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && !name.starts_with('.') {
            walk(&entry.path(), depth + 1, visit)?;
        } else if kind.is_file() {
            visit(&entry.path(), &name)?;
        }
    }
    ControlFlow::Continue(())
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
    read_note(path).ok()
}

#[cfg(test)]
thread_local! {
    /// Note files read by this thread, so tests can prove a rescan skipped them.
    static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A note's text, or why it cannot be shown.
fn read_note(path: &Path) -> Result<String, &'static str> {
    #[cfg(test)]
    READS.with(|reads| reads.set(reads.get() + 1));
    let metadata = fs::symlink_metadata(path).map_err(|_| "unreadable")?;
    if !metadata.is_file() {
        return Err("unreadable");
    }
    if metadata.len() > MAX_NOTE_BYTES {
        return Err("tooLarge");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_NOTE_BYTES).read_to_end(&mut bytes))
        .map_err(|_| "unreadable")?;
    String::from_utf8(bytes).map_err(|_| "notUtf8")
}

/// Splits front matter, between two lines that are exactly `---` (LF or
/// CRLF), from the body. Reads flat `key: value` lines and block lists
/// (`key:` followed by `- item` lines), which become `[a, b]`.
pub(super) fn split_frontmatter(text: &str) -> (Vec<(String, String)>, &str) {
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(|line| line.trim_end_matches(['\n', '\r'])) != Some("---") {
        return (Vec::new(), text);
    }
    let mut end = text.find('\n').map_or(text.len(), |at| at + 1);
    let mut block = Vec::new();
    let mut closed = false;
    for line in lines {
        end += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        if content == "---" {
            closed = true;
            break;
        }
        block.push(content);
    }
    if !closed {
        return (Vec::new(), text);
    }
    let unquote = |value: &str| {
        let value = value.trim();
        value
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .map_or_else(|| value.to_string(), |inner| inner.replace("\\\"", "\""))
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    // Items of a block list under the last key, which had no inline value.
    let mut items: Option<Vec<String>> = None;
    for line in block {
        let item = line
            .trim_start()
            .strip_prefix('-')
            .filter(|rest| rest.is_empty() || rest.starts_with(' '));
        if let (Some(item), Some(list)) = (item, items.as_mut()) {
            list.push(unquote(item));
            continue;
        }
        if let (Some(list), Some(last)) = (items.take(), pairs.last_mut()) {
            last.1 = format!("[{}]", list.join(", "));
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = unquote(value);
            items = value.is_empty().then(Vec::new);
            pairs.push((key.trim().to_string(), value));
        }
    }
    if let (Some(list), Some(last)) = (items, pairs.last_mut()) {
        if !list.is_empty() {
            last.1 = format!("[{}]", list.join(", "));
        }
    }
    (pairs, text[end..].trim_start_matches(['\n', '\r']))
}

/// Lines of `body` outside fenced code blocks (``` or ~~~, closed by a
/// fence of the same character at least as long).
fn prose_lines(body: &str) -> impl Iterator<Item = &str> {
    let mut fence: Option<(u8, usize)> = None;
    body.lines().filter(move |line| {
        let trimmed = line.trim_start();
        let marker = trimmed
            .as_bytes()
            .first()
            .copied()
            .filter(|c| matches!(c, b'`' | b'~'));
        if let Some(marker) = marker {
            let run = trimmed.bytes().take_while(|&c| c == marker).count();
            if run >= 3 {
                match fence {
                    None => {
                        fence = Some((marker, run));
                        return false;
                    }
                    Some((open, length))
                        if open == marker && run >= length && trimmed[run..].trim().is_empty() =>
                    {
                        fence = None;
                        return false;
                    }
                    _ => {}
                }
            }
        }
        fence.is_none()
    })
}

/// `line` with the contents of its inline code spans blanked out.
fn without_code_spans(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = line.as_bytes().to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let run = bytes[index..].iter().take_while(|&&c| c == b'`').count();
        let open_end = index + run;
        // The closing run has exactly the same length.
        let mut search = open_end;
        let mut close = None;
        while search < bytes.len() {
            if bytes[search] == b'`' {
                let other = bytes[search..].iter().take_while(|&&c| c == b'`').count();
                if other == run {
                    close = Some(search);
                    break;
                }
                search += other;
            } else {
                search += 1;
            }
        }
        match close {
            Some(close) => {
                out[open_end..close].fill(b' ');
                index = close + run;
            }
            None => index = open_end,
        }
    }
    // Only ASCII spaces replaced whole code spans, so the text stays UTF-8.
    String::from_utf8(out).unwrap_or_default()
}

/// The destination of a Markdown link whose text ended just before `rest`:
/// `<...>`, or up to the first space or unbalanced `)`.
fn destination(rest: &str) -> Option<&str> {
    let rest = rest.trim_start();
    if let Some(inner) = rest.strip_prefix('<') {
        return inner.find('>').map(|end| &inner[..end]);
    }
    let mut depth = 0usize;
    for (at, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(&rest[..at]),
            ')' => depth -= 1,
            c if c.is_whitespace() => return Some(&rest[..at]),
            _ => {}
        }
    }
    None
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

/// Markdown `](target)` links and `[[wiki]]` link targets of a note body,
/// outside code. Wiki targets drop their `|alias`, `#heading` and `^block`.
fn raw_links(body: &str) -> (Vec<String>, Vec<String>) {
    let mut markdown = Vec::new();
    let mut wiki = Vec::new();
    for line in prose_lines(body) {
        let line = without_code_spans(line);
        let mut rest = line.as_str();
        while let Some(start) = rest.find("](") {
            rest = &rest[start + 2..];
            if let Some(target) = destination(rest) {
                markdown.push(target.to_string());
            }
        }
        let mut rest = line.as_str();
        while let Some(start) = rest.find("[[") {
            rest = &rest[start + 2..];
            if let Some(end) = rest.find("]]") {
                let inner = rest[..end].split('|').next().unwrap_or_default();
                let inner = inner.split(['#', '^']).next().unwrap_or_default();
                wiki.push(inner.trim().to_string());
                rest = &rest[end + 2..];
            }
        }
    }
    (markdown, wiki)
}

/// Note paths by lower-cased file stem and by lower-cased path without
/// `.md`, to resolve wiki links the way Obsidian does: case-insensitively,
/// by name or by path.
struct WikiIndex {
    by_stem: BTreeMap<String, Vec<String>>,
    by_path: BTreeMap<String, String>,
}

fn without_md(path: &str) -> &str {
    let cut = path.len().saturating_sub(3);
    match path.get(cut..) {
        Some(ext) if ext.eq_ignore_ascii_case(".md") => &path[..cut],
        _ => path,
    }
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

impl WikiIndex {
    fn new<'a>(paths: impl Iterator<Item = &'a String>) -> Self {
        let mut by_stem: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_path = BTreeMap::new();
        for path in paths {
            let key = without_md(path).to_lowercase();
            let stem = key.rsplit('/').next().unwrap_or(&key).to_string();
            by_stem.entry(stem).or_default().push(path.clone());
            by_path.entry(key).or_insert_with(|| path.clone());
        }
        Self { by_stem, by_path }
    }

    /// The note `[[target]]` in the note at `from` points to.
    fn resolve(&self, from: &str, target: &str) -> Option<String> {
        let key = without_md(target.trim()).to_lowercase();
        if key.is_empty() {
            return None;
        }
        let folder = folder_of(from).to_lowercase();
        if key.contains('/') {
            // A vault path, then a path from the linking note's folder.
            let relative = format!("{folder}/{key}");
            if let Some(path) = self
                .by_path
                .get(&key)
                .or_else(|| self.by_path.get(&relative))
            {
                return Some(path.clone());
            }
        }
        // By name, or by the end of a path: the closest such note.
        let stem = key.rsplit('/').next().unwrap_or(&key);
        let suffix = format!("/{key}");
        self.by_stem
            .get(stem)?
            .iter()
            .filter(|path| {
                let lower = without_md(path).to_lowercase();
                !key.contains('/') || lower.ends_with(&suffix)
            })
            .min_by_key(|path| {
                (
                    folder_of(path).to_lowercase() != folder,
                    path.matches('/').count(),
                    path.as_str(),
                )
            })
            .cloned()
    }
}

/// `#tag` tokens in a note body, outside code: fenced blocks and inline
/// code spans, as for links.
pub(super) fn body_tags(body: &str) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    for line in prose_lines(body) {
        let heading = line.trim_start();
        if heading.starts_with('#') && heading.trim_start_matches('#').starts_with(' ') {
            continue;
        }
        let line = without_code_spans(line);
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
            prose_lines(body)
                .find_map(|line| line.strip_prefix("# "))
                .map(|heading| heading.trim().trim_matches('`').to_string())
        })
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string())
}

pub(super) fn scan(vault: &Path, previous: Option<&Vault>) -> Vault {
    scan_with_limit(vault, MAX_NOTES, previous)
}

/// [`scan`] keeping at most `limit` readable notes; `truncated` when a
/// further readable note exists. Notes of `previous` whose file has the
/// same size and modification time, and had settled before that scan, are
/// reused without reading the file.
fn scan_with_limit(vault: &Path, limit: usize, previous: Option<&Vault>) -> Vault {
    let scanned_at = std::time::SystemTime::now();
    let reusable: std::collections::HashMap<&str, &Note> = previous
        .map(|previous| {
            previous
                .notes
                .iter()
                .filter(|note| {
                    note.stamp
                        .is_some_and(|stamp| stamp.settled_before(previous.scanned_at))
                })
                .map(|note| (note.path.as_str(), note))
                .collect()
        })
        .unwrap_or_default();
    let mut parsed: Vec<Note> = Vec::new();
    let mut skipped: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut truncated = false;
    let _ = walk(vault, 0, &mut |file, name| {
        if !name.ends_with(".md") {
            // `.MD` and the like: not notes here, but reported.
            if without_md(name).len() < name.len() {
                *skipped.entry("extension").or_default() += 1;
            }
            return ControlFlow::Continue(());
        }
        let Some(path) = relative(file, vault) else {
            return ControlFlow::Continue(());
        };
        let stamp = Stamp::of(file);
        let unchanged = reusable
            .get(path.as_str())
            .filter(|note| stamp.is_some() && note.stamp == stamp);
        let note = match unchanged {
            Some(note) => Ok((*note).clone()),
            None => read_note(file).map(|text| parse_note(path, &text, stamp)),
        };
        match note {
            Err(reason) => *skipped.entry(reason).or_default() += 1,
            Ok(_) if parsed.len() == limit => {
                truncated = true;
                return ControlFlow::Break(());
            }
            Ok(note) => parsed.push(note),
        }
        ControlFlow::Continue(())
    });
    let known: BTreeSet<String> = parsed.iter().map(|note| note.path.clone()).collect();
    let wiki_index = WikiIndex::new(known.iter());
    let mut backlinks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut notes = Vec::new();
    for mut note in parsed {
        let mut links: BTreeSet<String> = note
            .markdown
            .iter()
            .filter_map(|target| resolve(&note.path, target))
            .filter(|target| known.contains(target))
            .collect();
        links.extend(
            note.wiki
                .iter()
                .filter_map(|name| wiki_index.resolve(&note.path, name)),
        );
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
        skipped,
        scanned_at,
    }
}

/// A note read from `text`, its links not yet resolved.
fn parse_note(path: String, text: &str, stamp: Option<Stamp>) -> Note {
    let modified = stamp
        .and_then(|stamp| stamp.modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let (frontmatter, body) = split_frontmatter(text);
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
    Note {
        title: title_of(&frontmatter, body, &path),
        path,
        tags: tags.into_iter().collect(),
        frontmatter,
        links: Vec::new(),
        modified,
        bytes: text.len() as u64,
        body: body.into(),
        stamp,
        markdown,
        wiki,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn vault_of(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let file = dir.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        dir
    }

    fn links_of(vault: &Vault, path: &str) -> Vec<String> {
        vault
            .notes
            .iter()
            .find(|note| note.path == path)
            .unwrap()
            .links
            .clone()
    }

    #[test]
    fn a_wiki_link_prefers_the_note_in_the_same_folder() {
        let dir = vault_of(&[
            ("a/Note.md", b"# A"),
            ("b/Note.md", b"# B"),
            ("b/From.md", b"[[Note]]"),
            ("c/From.md", b"[[Note]]"),
            ("deep/er/Note.md", b"# Deep"),
        ]);
        let vault = scan(dir.path(), None);
        assert_eq!(links_of(&vault, "b/From.md"), ["b/Note.md"]);
        // Elsewhere: the shortest path, then the first in order.
        assert_eq!(links_of(&vault, "c/From.md"), ["a/Note.md"]);
    }

    #[test]
    fn wiki_links_accept_paths_extensions_any_case_aliases_and_headings() {
        let dir = vault_of(&[
            ("dir/Target Note.md", b"# T"),
            ("other/Target Note.md", b"# Other"),
            ("s1.md", b"[[dir/Target Note]]"),
            ("s2.md", b"[[dir/Target Note.md]]"),
            ("s3.md", b"[[DIR/target note]]"),
            ("s4.md", b"[[dir/Target Note|Alias]]"),
            ("s5.md", b"[[dir/Target Note#Heading|Alias]]"),
            ("s6.md", b"[[Target Note^block]]"),
            ("other/s7.md", b"[[target note.MD]]"),
        ]);
        let vault = scan(dir.path(), None);
        for source in ["s1.md", "s2.md", "s3.md", "s4.md", "s5.md", "s6.md"] {
            assert_eq!(links_of(&vault, source), ["dir/Target Note.md"], "{source}");
        }
        assert_eq!(links_of(&vault, "other/s7.md"), ["other/Target Note.md"]);
    }

    #[test]
    fn links_in_code_are_ignored_and_destinations_keep_parentheses_and_spaces() {
        let (markdown, wiki) = raw_links(
            "```\n[[Fenced]]\n[x](fenced.md)\n```\n~~~~\n[[Tilde]]\n~~~~\n\
             `[[inline]]` and ``[y](span.md)`` \
             [u](https://example.org/a_(b)) [s](<Target With Space.md> \"title\") [t](c.md \"T\")",
        );
        assert_eq!(
            markdown,
            ["https://example.org/a_(b)", "Target With Space.md", "c.md"]
        );
        assert!(wiki.is_empty(), "{wiki:?}");
        let dir = vault_of(&[
            ("Target With Space.md", b"# T"),
            (
                "from.md",
                b"[s](<Target With Space.md>) [p](Target%20With%20Space.md)",
            ),
        ]);
        assert_eq!(
            links_of(&scan(dir.path(), None), "from.md"),
            ["Target With Space.md"]
        );
    }

    #[test]
    fn frontmatter_reads_crlf_block_lists_and_exact_delimiters_only() {
        let (pairs, body) =
            split_frontmatter("---\r\ntitle: A\r\ntags:\r\n  - x\r\n  - \"y z\"\r\n---\r\n# H\r\n");
        assert_eq!(
            pairs,
            [
                ("title".to_string(), "A".to_string()),
                ("tags".to_string(), "[x, y z]".to_string())
            ]
        );
        assert!(body.starts_with("# H"), "{body:?}");
        let text = "----\ntitle: A\n----\nbody";
        assert_eq!(split_frontmatter(text), (Vec::new(), text));
        let unterminated = "---\ntitle: A\n----\nbody";
        assert_eq!(split_frontmatter(unterminated), (Vec::new(), unterminated));
        let dir = vault_of(&[("n.md", b"---\ntags:\n- one\n- two\n---\ntext")]);
        assert_eq!(scan(dir.path(), None).notes[0].tags, ["one", "two"]);
    }

    #[test]
    fn the_title_ignores_headings_in_code() {
        assert_eq!(
            title_of(&[], "```\n# fake heading\n```\n# Real\n", "x.md"),
            "Real"
        );
    }

    #[test]
    fn the_note_limit_counts_valid_notes_and_skipped_files_are_reported() {
        let large = vec![b'a'; MAX_NOTE_BYTES as usize + 1];
        let dir = vault_of(&[
            ("a.md", b"a"),
            ("b.md", b"b"),
            ("big.md", &large),
            ("c.md", b"c"),
            ("bin.md", &[0xff, 0xfe]),
            ("upper.MD", b"u"),
        ]);
        let exact = scan_with_limit(dir.path(), 3, None);
        assert_eq!(exact.notes.len(), 3);
        assert!(!exact.truncated);
        assert_eq!(
            exact.skipped,
            BTreeMap::from([("extension", 1), ("notUtf8", 1), ("tooLarge", 1)])
        );
        fs::write(dir.path().join("d.md"), "d").unwrap();
        let over = scan_with_limit(dir.path(), 3, None);
        assert_eq!(over.notes.len(), 3);
        assert!(over.truncated);
    }

    /// Everything a scan exposes, to compare two scans.
    fn summary(vault: &Vault) -> String {
        let notes: Vec<_> = vault
            .notes
            .iter()
            .map(|n| {
                (
                    &n.path,
                    &n.title,
                    &n.tags,
                    &n.frontmatter,
                    &n.links,
                    n.bytes,
                    n.modified,
                )
            })
            .collect();
        format!(
            "{notes:?} {:?} {} {:?}",
            vault.backlinks, vault.truncated, vault.skipped
        )
    }

    fn reads() -> usize {
        READS.with(std::cell::Cell::get)
    }

    /// Sets a file's modification time `seconds` in the past.
    fn age(path: &Path, seconds: u64) {
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(past)
            .unwrap();
    }

    #[test]
    fn a_rescan_reads_only_changed_notes_and_equals_a_full_scan() {
        let dir = vault_of(&[
            ("a/Note.md", b"---\ntags: [x]\n---\n# A\n[[b]] [c](../c.md)"),
            ("b.md", b"# B\n[[Note]]"),
            ("c.md", b"# C"),
            ("gone.md", b"# Gone\n[[c]]"),
            ("old name.md", b"# Old name"),
        ]);
        for path in ["a/Note.md", "b.md", "c.md", "gone.md", "old name.md"] {
            age(&dir.path().join(path), 60);
        }
        let first = scan(dir.path(), None);
        let before = reads();
        let same = scan(dir.path(), Some(&first));
        assert_eq!(reads() - before, 0, "an unchanged vault was read again");
        assert_eq!(summary(&same), summary(&first));

        // An edit, a deletion, a rename and a new note that changes how
        // `[[Note]]` in an unchanged note resolves.
        fs::write(dir.path().join("c.md"), "# C edited\n[[b]]").unwrap();
        fs::remove_file(dir.path().join("gone.md")).unwrap();
        fs::rename(
            dir.path().join("old name.md"),
            dir.path().join("new name.md"),
        )
        .unwrap();
        fs::write(dir.path().join("Note.md"), "# Root note").unwrap();
        let before = reads();
        let rescan = scan(dir.path(), Some(&same));
        // c.md, new name.md and Note.md: renamed files are read again.
        assert_eq!(reads() - before, 3);
        assert_eq!(summary(&rescan), summary(&scan(dir.path(), None)));
        assert_eq!(links_of(&rescan, "b.md"), ["Note.md"]);
    }

    #[test]
    fn a_file_rewritten_with_the_same_size_and_time_is_reread_while_it_may_be_racy() {
        let dir = vault_of(&[("n.md", b"# One")]);
        let file = dir.path().join("n.md");
        let first = scan(dir.path(), None);
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        // Same size, same modification time: only the content differs.
        fs::write(&file, "# Two").unwrap();
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let rescan = scan(dir.path(), Some(&first));
        assert_eq!(rescan.notes[0].title, "Two");
    }

    #[test]
    fn body_search_matches_lower_casing_each_line() {
        let bodies = [
            "plain ascii\nThe Needle here\n",
            "Un ÁRBOL grande.\nárbol\n",
            "\u{212A}elvin sign\nkelvin\n",
            "\u{130}stanbul\nİSTANBUL\n",
            "ΟΔΟΣ\nend\r\nCRLF Line\r\n",
            "needle at end needle",
            "",
        ];
        let queries = [
            "needle",
            "the n",
            "árbol",
            "rbol",
            "kelvin",
            "k",
            "i",
            "i\u{307}",
            "οδος",
            "οδοσ",
            "crlf line",
            "end",
            "e",
            "x",
            "needle at end needle",
        ];
        for body in bodies {
            let note = Note {
                path: String::new(),
                title: String::new(),
                tags: Vec::new(),
                frontmatter: Vec::new(),
                links: Vec::new(),
                modified: 0,
                bytes: 0,
                body: body.into(),
                stamp: None,
                markdown: Vec::new(),
                wiki: Vec::new(),
            };
            for query in queries {
                let expected = body
                    .lines()
                    .find(|line| crate::web::api::contains_ci(line, query))
                    .map(str::trim);
                assert_eq!(
                    note.body_line(&query.to_lowercase()),
                    expected,
                    "{body:?} {query:?}"
                );
            }
        }
    }

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
    fn body_tags_skip_code_the_way_links_do() {
        let body = "#kept\n~~~\n#tilde\n~~~\n````\n```\n#nested\n````\nsee `#inline` and #after\n";
        assert_eq!(
            body_tags(body),
            BTreeSet::from(["after".to_string(), "kept".to_string()])
        );
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
