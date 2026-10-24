//! Bounded source previews for files the index already lists (ADR-018 D5).
//!
//! The caller passes a path that is an entry of the navigation snapshot; this
//! module still refuses anything that resolves outside the project root, any
//! symlink on the way, and anything that is not a regular file.

use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) const MAX_PREVIEW_LINES: usize = 400;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) struct Excerpt {
    /// 1-based number of the first returned line.
    pub first: u32,
    pub lines: Vec<String>,
    /// The requested range was longer than the preview limit.
    pub truncated: bool,
}

fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
    }
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES).then_some(path)
}

pub(super) use mct_index::review_keys::indexed_text;

/// A bounded source page from exactly the indexed revision, never a stale
/// range applied to newly edited content.
pub(super) fn indexed_excerpt(
    root: &Path,
    relative: &str,
    hash: &str,
    start: u32,
    end: u32,
) -> Option<Excerpt> {
    if start == 0 || end < start {
        return None;
    }
    let content = indexed_text(root, relative, hash)?;
    let wanted = (end - start + 1) as usize;
    let lines: Vec<String> = content
        .lines()
        .skip((start - 1) as usize)
        .take(wanted.min(MAX_PREVIEW_LINES))
        .map(str::to_string)
        .collect();
    if lines.len() != wanted.min(MAX_PREVIEW_LINES) {
        return None;
    }
    Some(Excerpt {
        first: start,
        lines,
        truncated: wanted > MAX_PREVIEW_LINES,
    })
}

/// Up to `max` bytes of a regular file below `root`, decoded lossily at a cut
/// edge. Same path rules as [`excerpt`].
pub(super) fn read_text(root: &Path, relative: &str, max: usize) -> Option<String> {
    let path = resolve(root, relative)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(max as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Lines `start..=end` (1-based) of a project file, at most [`MAX_PREVIEW_LINES`].
/// `None` when the file is missing, unreadable, too large or not UTF-8.
pub(super) fn excerpt(root: &Path, relative: &str, start: u32, end: u32) -> Option<Excerpt> {
    let path = resolve(root, relative)?;
    let mut content = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES)
        .read_to_string(&mut content)
        .ok()?;
    let start = start.max(1);
    let end = end.max(start);
    let wanted = (end - start + 1) as usize;
    let lines: Vec<String> = content
        .lines()
        .skip((start - 1) as usize)
        .take(wanted.min(MAX_PREVIEW_LINES))
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(Excerpt {
        first: start,
        truncated: wanted > MAX_PREVIEW_LINES,
        lines,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn returns_the_requested_lines_and_bounds_long_ranges() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        let text: String = (1..=500).map(|n| format!("line {n}\r\n")).collect();
        std::fs::write(root.join("src/a.rs"), text).unwrap();
        let part = excerpt(&root, "src/a.rs", 2, 3).unwrap();
        assert_eq!(
            (part.first, part.lines.as_slice()),
            (2, &["line 2".to_string(), "line 3".to_string()][..])
        );
        assert!(!part.truncated);
        let long = excerpt(&root, "src/a.rs", 1, 500).unwrap();
        assert_eq!(long.lines.len(), MAX_PREVIEW_LINES);
        assert!(long.truncated);
        assert!(excerpt(&root, "src/a.rs", 900, 910).is_none());
    }

    #[test]
    fn refuses_traversal_symlinks_and_non_files() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::write(root.join("secret.txt"), "top secret").unwrap();
        for path in [
            "../secret.txt",
            "dir/../secret.txt",
            "/etc/passwd",
            "dir",
            "dir//x",
            "a\\b",
            "",
            "missing",
        ] {
            assert!(excerpt(&root, path, 1, 1).is_none(), "{path}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("secret.txt"), root.join("dir/link.txt")).unwrap();
            std::os::unix::fs::symlink(root.join("dir"), root.join("alias")).unwrap();
            assert!(excerpt(&root, "dir/link.txt", 1, 1).is_none());
            assert!(excerpt(&root, "alias/link.txt", 1, 1).is_none());
        }
        std::fs::write(root.join("binary.bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(excerpt(&root, "binary.bin", 1, 1).is_none());
    }
}
