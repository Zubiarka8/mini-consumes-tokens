//! A fixture project kept as one plain-text bundle and unpacked into a
//! temporary directory when a suite loads.
//!
//! A fixture checked in as source files is indexed along with this
//! repository by anyone running MCT on it, so its names become candidates
//! for this repository's own relations (a Python `open()` would resolve to
//! the fixture's `Loan.open`). No parser reads a `.txt` file, so a bundle is
//! versioned and reviewable but never indexed; the eval indexes the unpacked
//! copy, outside the repository, and removes it when the suite is dropped.
//!
//! Format: a line `==> path <==` starts a file, and every line up to the
//! next such header is its content, byte for byte (so line numbers in
//! labels are the unpacked file's). Lines before the first header are notes
//! and are ignored. A path is relative, `/`-separated, and may not leave
//! the bundle's root.

use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

/// One file of a bundle: its relative path and content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledFile {
    pub path: PathBuf,
    pub content: String,
}

/// The files of a bundle, in order.
pub fn parse(text: &str) -> Result<Vec<BundledFile>> {
    let mut files: Vec<BundledFile> = Vec::new();
    for (number, line) in text.split_inclusive('\n').enumerate() {
        let header = line
            .trim_end_matches(['\n', '\r'])
            .strip_prefix("==> ")
            .and_then(|rest| rest.strip_suffix(" <=="));
        match (header, files.last_mut()) {
            (Some(path), _) => {
                let path = relative_path(path).with_context(|| format!("line {}", number + 1))?;
                if files.iter().any(|file| file.path == path) {
                    bail!("line {}: `{}` is given twice", number + 1, path.display());
                }
                files.push(BundledFile {
                    path,
                    content: String::new(),
                });
            }
            (None, Some(file)) => file.content.push_str(line),
            // A note before the first file.
            (None, None) => {}
        }
    }
    Ok(files)
}

/// Writes every file of `text` under `root`, which must exist.
pub fn unpack(text: &str, root: &Path) -> Result<()> {
    for file in parse(text)? {
        let path = root.join(&file.path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, file.content)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// `path` as a relative path made of plain names only.
fn relative_path(path: &str) -> Result<PathBuf> {
    let parsed = Path::new(path);
    let plain = !path.is_empty()
        && !path.contains('\\')
        && !path.split('/').any(|part| part.is_empty() || part == ".")
        && parsed
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !plain {
        bail!("`{path}` is not a plain relative path");
    }
    Ok(parsed.to_path_buf())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn splits_files_at_their_headers_and_keeps_content_exact() {
        let text = "notes before the first file\n\
                    ==> a/one.rs <==\n\
                    fn one() {}\n\
                    \n\
                    ==> b.py <==\n\
                    def b():\n    pass\n";
        let files = parse(text).unwrap();
        assert_eq!(
            files,
            vec![
                BundledFile {
                    path: "a/one.rs".into(),
                    content: "fn one() {}\n\n".into(),
                },
                BundledFile {
                    path: "b.py".into(),
                    content: "def b():\n    pass\n".into(),
                },
            ]
        );
    }

    #[test]
    fn rejects_a_path_that_leaves_the_root() {
        for path in ["../x.rs", "a/../../x.rs", "/etc/x.rs", "", "a/./b.rs"] {
            let text = format!("==> {path} <==\nx\n");
            assert!(parse(&text).is_err(), "{path:?} was accepted");
        }
    }

    #[test]
    fn rejects_a_file_given_twice() {
        assert!(parse("==> a.rs <==\nx\n==> a.rs <==\ny\n").is_err());
    }

    #[test]
    fn unpacks_every_file_under_the_root() {
        let dir = tempfile::tempdir().unwrap();
        unpack("==> src/a.rs <==\nfn a() {}\n", dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/a.rs")).unwrap(),
            "fn a() {}\n"
        );
    }
}
