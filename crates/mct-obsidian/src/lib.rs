//! Writes a generated Obsidian code atlas: a root map, one map per source
//! directory, and one metadata note per indexed source file. Source content
//! is never copied into the vault.
//! Read-only over the index (through `mct-index`'s query API); `mct-index`
//! itself knows nothing about this crate and reaches it only through the
//! post-reindex hook that [`register`] installs.

mod layout;
mod manifest;
mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mct_index::{Index, IndexError, SymbolListEntry};

pub use manifest::{hash_note, Manifest, MANIFEST_FILE};
pub use render::{render_file_note, render_index, render_map, FileEntry, INDEX_NOTE};

/// Vault location relative to the project root.
pub const VAULT_RELATIVE_DIR: &str = ".mct-index/obsidian";

/// Most imports listed per source-file note.
pub const MAX_IMPORT_LINKS: usize = 50;
/// Most symbols listed per file note.
pub const MAX_SYMBOLS: usize = 100;
/// Most entries per directory map page.
pub const MAX_MAP_ENTRIES: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error(transparent)]
    Index(#[from] IndexError),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("refusing to write through a symlink: {0}")]
    Symlink(String),

    #[error("refusing to replace an unmanaged or modified vault file: {0}")]
    Conflict(String),

    #[error("invalid vault manifest: {0}")]
    Manifest(String),

    #[error("`{0}` exists but is not a directory")]
    NotADirectory(String),

    #[error("note path `{0}` would leave the vault")]
    UnsafePath(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

/// What one [`write_vault`] call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultReport {
    pub notes_written: usize,
    pub notes_unchanged: usize,
    pub notes_removed: usize,
}

/// The vault directory for a project rooted at `root`.
pub fn vault_dir(root: &Path) -> PathBuf {
    root.join(VAULT_RELATIVE_DIR)
}

/// Every note's text, keyed by its path inside the vault.
fn build_notes(index: &Index) -> Result<BTreeMap<String, String>> {
    let snapshot = index.navigation_snapshot()?;
    let files: Vec<_> = snapshot
        .files
        .into_iter()
        .filter(|file| file.language != "asset")
        .collect();
    let mut symbols: BTreeMap<String, Vec<SymbolListEntry>> = BTreeMap::new();
    for entry in snapshot.symbols {
        symbols
            .entry(entry.relative_path.clone())
            .or_default()
            .push(entry);
    }

    let file_note_paths =
        layout::file_note_paths(files.iter().map(|file| file.relative_path.as_str()));
    let directory_note_paths =
        layout::directory_note_paths(files.iter().map(|file| file.relative_path.as_str()));
    let mut imports_by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in snapshot.imports {
        imports_by_file
            .entry(edge.from)
            .or_default()
            .insert(edge.to);
    }

    let file_imports: Vec<Vec<String>> = files
        .iter()
        .map(|file| {
            imports_by_file
                .get(&file.relative_path)
                .into_iter()
                .flatten()
                .filter(|target| file_note_paths.contains_key(*target))
                .cloned()
                .collect()
        })
        .collect();
    let entries: Vec<FileEntry<'_>> = files
        .iter()
        .zip(&file_imports)
        .map(|(file, imports)| FileEntry {
            path: &file.relative_path,
            language: &file.language,
            symbols: symbols
                .get(&file.relative_path)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            imports,
        })
        .collect();

    let mut notes = BTreeMap::new();
    for (file, metadata) in entries.iter().zip(&files) {
        let note_path = file_note_paths
            .get(file.path)
            .ok_or_else(|| VaultError::UnsafePath(file.path.to_string()))?;
        notes.insert(
            note_path.clone(),
            render::render_file_note_at(
                file,
                &metadata.content_hash,
                &file_note_paths,
                &directory_note_paths,
                note_path,
            ),
        );
    }

    let mut entries_by_directory: BTreeMap<String, Vec<&FileEntry<'_>>> = BTreeMap::new();
    for entry in &entries {
        let directory = entry.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        entries_by_directory
            .entry(directory.to_string())
            .or_default()
            .push(entry);
    }
    let mut children_by_directory: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for directory in directory_note_paths.keys() {
        let parent = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
        children_by_directory
            .entry(parent.to_string())
            .or_default()
            .push(directory.clone());
    }
    for children in children_by_directory.values_mut() {
        children.sort();
    }

    for (directory, note_path) in &directory_note_paths {
        let files_here = entries_by_directory
            .get(directory)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        let children = children_by_directory
            .get(directory)
            .map(Vec::as_slice)
            .unwrap_or_default();
        add_map_pages(
            &mut notes,
            directory,
            None,
            &files_here,
            children,
            &file_note_paths,
            &directory_note_paths,
            note_path,
        );
    }

    let root_files = entries_by_directory
        .get("")
        .into_iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let root_directories = children_by_directory
        .get("")
        .map(Vec::as_slice)
        .unwrap_or_default();
    let symbol_count: usize = entries.iter().map(|file| file.symbols.len()).sum();
    let summary = format!(
        "{} source files and {symbol_count} symbols. Generated from the SQLite index; source text is not copied.",
        entries.len()
    );
    add_map_pages(
        &mut notes,
        "Code atlas",
        Some(&summary),
        &root_files,
        root_directories,
        &file_note_paths,
        &directory_note_paths,
        INDEX_NOTE,
    );
    Ok(notes)
}

#[allow(clippy::too_many_arguments)]
fn add_map_pages(
    notes: &mut BTreeMap<String, String>,
    title: &str,
    summary: Option<&str>,
    files: &[&FileEntry<'_>],
    children: &[String],
    file_paths: &BTreeMap<String, String>,
    directory_paths: &BTreeMap<String, String>,
    first: &str,
) {
    let total = files.len() + children.len();
    let pages = total.div_ceil(MAX_MAP_ENTRIES).max(1);
    let page_path = |page| {
        if page == 0 {
            first.to_string()
        } else {
            format!("{}-page-{}.md", first.trim_end_matches(".md"), page + 1)
        }
    };
    for page in 0..pages {
        let start = page * MAX_MAP_ENTRIES;
        let end = (start + MAX_MAP_ENTRIES).min(total);
        let dirs = &children[start.min(children.len())..end.min(children.len())];
        let here = &files[start.saturating_sub(children.len())..end.saturating_sub(children.len())];
        let path = page_path(page);
        let mut text = render::render_map_at(
            title,
            summary,
            here,
            dirs,
            file_paths,
            directory_paths,
            &path,
        );
        use std::fmt::Write as _;
        let _ = writeln!(
            text,
            "\nPage {} of {pages}; {total} entries total.\n",
            page + 1
        );
        if page > 0 {
            text.push_str(&render::link(&path, &page_path(page - 1), "Previous page"));
            text.push('\n');
        }
        if page + 1 < pages {
            text.push_str(&render::link(&path, &page_path(page + 1), "Next page"));
            text.push('\n');
        }
        notes.insert(path, text);
    }
}

/// Brings the vault at `out` in line with `index`: rewrites only the notes
/// that changed, drops the ones whose file left the index, and swaps the
/// staged result in with rollback on failure. The portable two-rename swap
/// is not crash-atomic. Call [`vault_dir`] for the conventional `out`.
pub fn write_vault(index: &Index, out: &Path) -> Result<VaultReport> {
    manifest::publish(out, &build_notes(index)?)
}

/// Held by the post-reindex hook while it publishes the vault. A process that
/// is about to exit takes it first, so it never stops in the middle of a
/// publication and leaves the writer lock (`<vault>.lock`) behind.
pub type PublishGuard = std::sync::Arc<std::sync::Mutex<()>>;

/// Registers the hook that regenerates the project's vault after every
/// successful reindex of `index`. A failure is reported as a warning on the
/// reindex, never as its error.
pub fn register(index: &mut Index) {
    register_guarded(index, PublishGuard::default());
}

/// [`register`], publishing only while holding `guard`. The hook waits for the
/// guard instead of failing, so an exiting process that holds it simply wins.
pub fn register_guarded(index: &mut Index, guard: PublishGuard) {
    let out = vault_dir(index.root());
    index.set_post_reindex_hook(Box::new(move |index| {
        // A poisoned guard only means another publisher panicked; the writer
        // lock itself is released by its Drop, so publishing is still safe.
        let _publishing = guard.lock().unwrap_or_else(|e| e.into_inner());
        write_vault(index, &out)
            .map(|_| ())
            .map_err(|err| IndexError::Hook(err.to_string()))
    }));
}
