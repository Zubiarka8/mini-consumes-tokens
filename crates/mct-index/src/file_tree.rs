//! Directory-tree listing for navigation, without touching the symbol
//! index — a plain filesystem walk pruned by the same [`ExcludeSet`]
//! `reindex` uses, so noise directories (`target/`, `node_modules/`,
//! `.git/`) never show up. Lives here (not `mct-mcp-server`) because it
//! shares `ExcludeSet` and the root-escape guard with the rest of this
//! crate, matching how `indexer.rs` already owns filesystem-walking logic
//! for the symbol walk.

use std::path::Path;

use crate::{ExcludeSet, IndexError, Result};

/// Per-directory cap on listed entries, independent of `depth`. A directory
/// with thousands of generated files (e.g. a build output dir that slipped
/// past `ExcludeSet`) would otherwise blow up a single response; `omitted`
/// reports how many were left out instead of silently dropping them.
const MAX_ENTRIES_PER_DIR: usize = 200;

/// One node of a directory tree, returned by [`crate::Index::file_tree`].
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub is_dir: bool,
    /// Empty for a file, or for a directory whose listing stopped at the
    /// requested depth — see `depth_exhausted` to tell the two apart.
    pub children: Vec<FileTreeNode>,
    /// How many entries exist in this directory beyond `children`, because
    /// [`MAX_ENTRIES_PER_DIR`] was hit. Always 0 for a file.
    pub omitted: usize,
    /// True when this directory has entries that were not listed because
    /// the walk reached the requested `depth` before descending into it —
    /// as opposed to `children` being genuinely empty.
    pub depth_exhausted: bool,
}

pub(crate) fn file_tree(
    root: &Path,
    exclude: &ExcludeSet,
    relative_start: &str,
    depth: u32,
) -> Result<FileTreeNode> {
    let start = root.join(relative_start);
    let canonical = start.canonicalize().map_err(|source| IndexError::Io {
        path: relative_start.to_string(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(IndexError::PathEscapesRoot(relative_start.to_string()));
    }
    if !canonical.is_dir() {
        return Err(IndexError::NotADirectory(relative_start.to_string()));
    }
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(relative_start)
        .to_string();
    Ok(build_node(root, &canonical, name, depth, exclude))
}

fn build_node(
    root: &Path,
    dir: &Path,
    name: String,
    remaining_depth: u32,
    exclude: &ExcludeSet,
) -> FileTreeNode {
    if remaining_depth == 0 {
        let depth_exhausted = std::fs::read_dir(dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
        return FileTreeNode {
            name,
            is_dir: true,
            children: Vec::new(),
            omitted: 0,
            depth_exhausted,
        };
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map(|read| read.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    entries.retain(|entry| {
        to_relative_slash_path(root, &entry.path())
            .map(|relative| !exclude.is_excluded(&relative))
            .unwrap_or(true)
    });
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        b_is_dir
            .cmp(&a_is_dir)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let omitted = entries.len().saturating_sub(MAX_ENTRIES_PER_DIR);
    entries.truncate(MAX_ENTRIES_PER_DIR);

    let children = entries
        .into_iter()
        .map(|entry| {
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                build_node(
                    root,
                    &entry.path(),
                    entry_name,
                    remaining_depth - 1,
                    exclude,
                )
            } else {
                FileTreeNode {
                    name: entry_name,
                    is_dir: false,
                    children: Vec::new(),
                    omitted: 0,
                    depth_exhausted: false,
                }
            }
        })
        .collect();

    FileTreeNode {
        name,
        is_dir: true,
        children,
        omitted,
        depth_exhausted: false,
    }
}

/// Same relative-path derivation `indexer.rs::to_relative_slash_path` uses:
/// strips `root` by path components, never by canonicalizing each entry —
/// this is a read-only listing, not a security boundary, so the extra
/// syscalls `reindex`'s escape guard needs are not worth paying here.
fn to_relative_slash_path(root: &Path, entry_path: &Path) -> Option<String> {
    let rel = entry_path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in rel.components() {
        parts.push(component.as_os_str().to_str()?.to_string());
    }
    Some(parts.join("/"))
}
