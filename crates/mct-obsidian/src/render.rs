//! Deterministic metadata-only notes and bounded navigation maps.
//! Source bodies stay behind the existing context tools.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use mct_index::SymbolListEntry;

/// Name of the vault entry note.
pub const INDEX_NOTE: &str = "index.md";

/// Language of binary assets (images, models): they carry no symbols worth
/// listing.
const ASSET_LANGUAGE: &str = "asset";

/// Indexed file metadata used by navigation maps and file notes.
pub struct FileEntry<'a> {
    pub path: &'a str,
    pub language: &'a str,
    pub symbols: &'a [SymbolListEntry],
    /// Source paths this file imports (resolved, indexed files only).
    pub imports: &'a [String],
}

/// Inline code span: backticks and line breaks inside `text` become plain
/// characters so the span cannot be closed early.
fn code(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .take(160)
        .map(|c| if c.is_control() || c == '`' { ' ' } else { c })
        .collect();
    format!(
        "`{}`",
        cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

pub fn render_index(files: &[FileEntry<'_>]) -> String {
    let listed: Vec<&FileEntry<'_>> = files
        .iter()
        .filter(|f| f.language != ASSET_LANGUAGE && !f.symbols.is_empty())
        .collect();
    let total: usize = listed.iter().map(|f| f.symbols.len()).sum();
    let mut by_dir: BTreeMap<&str, Vec<&FileEntry<'_>>> = BTreeMap::new();
    for file in listed.iter().copied() {
        by_dir.entry(directory(file.path)).or_default().push(file);
    }

    let mut out = String::new();
    out.push_str("# Project index\n\n");
    let _ = writeln!(
        out,
        "{} files, {total} symbols. Generated from the mct index and regenerated on every reindex; \
         it holds no file content, only what is defined where.",
        listed.len()
    );
    for (dir, mut files) in by_dir {
        files.sort_by(|a, b| a.path.cmp(b.path));
        let title = if dir.is_empty() { "(root)" } else { dir };
        let _ = writeln!(out, "\n## {}", code(title));
        for file in files {
            let noun = if file.symbols.len() == 1 {
                "symbol"
            } else {
                "symbols"
            };
            let _ = writeln!(
                out,
                "\n### {} · {} · {} {noun}\n",
                code(file_name(file.path)),
                file.language,
                file.symbols.len()
            );
            let mut imports: Vec<&String> = file.imports.iter().collect();
            imports.sort();
            imports.dedup();
            if !imports.is_empty() {
                let list: Vec<String> = imports.into_iter().map(|p| code(p)).collect();
                let _ = writeln!(out, "Imports: {}\n", list.join(", "));
            }
            let mut symbols: Vec<&SymbolListEntry> = file.symbols.iter().collect();
            symbols.sort_by(|a, b| {
                (a.line, &a.name, &a.kind, &a.parent).cmp(&(b.line, &b.name, &b.kind, &b.parent))
            });
            for symbol in symbols {
                let _ = write!(out, "- *{}* {} · ", symbol.kind, code(&symbol.name));
                match symbol.end_line {
                    Some(end) if end > symbol.line => {
                        let _ = write!(out, "lines {}-{end}", symbol.line);
                    }
                    _ => {
                        let _ = write!(out, "line {}", symbol.line);
                    }
                }
                if let Some(parent) = &symbol.parent {
                    let _ = write!(out, " · in {}", code(parent));
                }
                out.push('\n');
            }
        }
    }
    out
}

/// Renders a hierarchical map-of-content note. The root index and each
/// directory note use the same format, so large repositories do not produce
/// one enormous navigation page.
pub fn render_map(
    title: &str,
    summary: Option<&str>,
    files: &[&FileEntry<'_>],
    child_directories: &[String],
    file_note_paths: &BTreeMap<String, String>,
    directory_note_paths: &BTreeMap<String, String>,
) -> String {
    render_map_at(
        title,
        summary,
        files,
        child_directories,
        file_note_paths,
        directory_note_paths,
        INDEX_NOTE,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_map_at(
    title: &str,
    summary: Option<&str>,
    files: &[&FileEntry<'_>],
    child_directories: &[String],
    file_note_paths: &BTreeMap<String, String>,
    directory_note_paths: &BTreeMap<String, String>,
    current: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", code(title));
    if current != INDEX_NOTE {
        let _ = writeln!(out, "{}\n", link(current, INDEX_NOTE, "Atlas index"));
    }
    if let Some(summary) = summary {
        let _ = writeln!(out, "{summary}\n");
    }

    if !child_directories.is_empty() {
        out.push_str("## Directories\n\n");
        for directory in child_directories {
            if let Some(note) = directory_note_paths.get(directory) {
                let _ = writeln!(out, "- {}", link(current, note, directory));
            }
        }
        out.push('\n');
    }

    let mut sorted_files = files.to_vec();
    sorted_files.sort_by(|a, b| a.path.cmp(b.path));
    if !sorted_files.is_empty() {
        out.push_str("## Files\n\n");
        for file in sorted_files {
            if let Some(note) = file_note_paths.get(file.path) {
                let noun = if file.symbols.len() == 1 {
                    "symbol"
                } else {
                    "symbols"
                };
                let _ = writeln!(
                    out,
                    "- {} · {} · {} {noun}",
                    link(current, note, file.path),
                    code(file.language),
                    file.symbols.len()
                );
            }
        }
    }
    out
}

/// Renders one source-file note without copying the source text. The note is
/// a compact navigation aid; MCP remains the source for code excerpts.
pub fn render_file_note(
    file: &FileEntry<'_>,
    content_hash: &str,
    file_note_paths: &BTreeMap<String, String>,
    imports_truncated: bool,
) -> String {
    let current = file_note_paths
        .get(file.path)
        .map(String::as_str)
        .unwrap_or(INDEX_NOTE);
    let directories = crate::layout::directory_note_paths([file.path]);
    let mut text = render_file_note_at(file, content_hash, file_note_paths, &directories, current);
    if imports_truncated {
        text.push_str("\nAdditional imports omitted by the caller.\n");
    }
    text
}

pub(crate) fn render_file_note_at(
    file: &FileEntry<'_>,
    content_hash: &str,
    file_note_paths: &BTreeMap<String, String>,
    directory_note_paths: &BTreeMap<String, String>,
    current: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---\ntitle: {}", yaml_string(file.path));
    let _ = writeln!(out, "path: {}", yaml_string(file.path));
    let _ = writeln!(out, "language: {}", yaml_string(file.language));
    let _ = writeln!(out, "content_hash: {}", yaml_string(content_hash));
    let _ = writeln!(out, "symbol_count: {}\n---\n", file.symbols.len());
    let _ = writeln!(out, "# {}\n", code(file.path));
    out.push_str(
        "Generated from the SQLite index. Source text is not copied; use the code-context tools to retrieve excerpts.\n",
    );

    let parent = directory_note_paths
        .get(directory(file.path))
        .map(String::as_str)
        .unwrap_or(INDEX_NOTE);
    let _ = writeln!(
        out,
        "\nDirectory: {}",
        link(
            current,
            parent,
            if directory(file.path).is_empty() {
                "Project root"
            } else {
                directory(file.path)
            }
        )
    );
    let mut imports: Vec<&String> = file.imports.iter().collect();
    imports.sort();
    imports.dedup();
    let linked_imports: Vec<(&String, &String)> = imports
        .iter()
        .filter_map(|path| file_note_paths.get(*path).map(|note| (*path, note)))
        .collect();
    if !linked_imports.is_empty() {
        out.push_str("\n## Imports\n\n");
        let omitted = linked_imports.len().saturating_sub(crate::MAX_IMPORT_LINKS);
        for (path, note) in linked_imports.into_iter().take(crate::MAX_IMPORT_LINKS) {
            let _ = writeln!(out, "- {}", link(current, note, path));
        }
        if omitted > 0 {
            let _ = writeln!(
                out,
                "- {omitted} imports omitted (limit: {}).",
                crate::MAX_IMPORT_LINKS
            );
        }
    }

    let mut symbols: Vec<&SymbolListEntry> = file.symbols.iter().collect();
    symbols.sort_by(|a, b| {
        (a.line, &a.name, &a.kind, &a.parent).cmp(&(b.line, &b.name, &b.kind, &b.parent))
    });
    if !symbols.is_empty() {
        out.push_str("\n## Symbols\n\n");
        let omitted = symbols.len().saturating_sub(crate::MAX_SYMBOLS);
        for symbol in symbols.into_iter().take(crate::MAX_SYMBOLS) {
            let _ = write!(
                out,
                "- **{}** {} · ",
                code(&symbol.kind),
                code(&symbol.name)
            );
            match symbol.end_line {
                Some(end) if end > symbol.line => {
                    let _ = write!(out, "lines {}-{end}", symbol.line);
                }
                _ => {
                    let _ = write!(out, "line {}", symbol.line);
                }
            }
            if let Some(parent) = &symbol.parent {
                let _ = write!(out, " · in {}", code(parent));
            }
            out.push('\n');
        }
        if omitted > 0 {
            let _ = writeln!(
                out,
                "- {omitted} symbols omitted (limit: {}).",
                crate::MAX_SYMBOLS
            );
        }
    }
    out
}

fn yaml_string(value: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(quoted, "\\u{:04X}", u32::from(ch));
            }
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// Relative Markdown links work in Obsidian and ordinary Markdown readers.
pub(crate) fn link(current: &str, target: &str, label: &str) -> String {
    let depth = current.matches('/').count();
    let relative = format!("{}{target}", "../".repeat(depth));
    let mut encoded = String::new();
    for byte in relative.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    let label: String = label
        .chars()
        .take(240)
        .flat_map(|ch| {
            if ch.is_control() {
                vec![' ']
            } else if "\\[]`*_<>".contains(ch) {
                vec!['\\', ch]
            } else {
                vec![ch]
            }
        })
        .collect();
    format!("[{label}]({encoded})")
}
