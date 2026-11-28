//! Path catalog of non-source files (issue #629).
//!
//! `tests/fixtures/path-catalog/` holds one tiny file per format family,
//! with upper-case extensions, nested paths, ambiguous extensions, files
//! without an extension and paths the default exclusions drop. The two
//! files that cannot be checked in (`.mctignore` and a `.exe` are ignored by
//! this repository's `.gitignore`) are written by [`fixture`].
//!
//! The toy registry parses `src/*.ts`/`*.rs` (`function NAME` / `fn NAME`)
//! and claims `.svg` and `.json`, so a file covered by a parser and by the
//! catalog is exercised too.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index};

#[path = "common/scratch_dir.rs"]
mod scratch_dir;

/// A parser for `language` over `extensions`: every `function NAME` or
/// `fn NAME` token pair is a function; a file with none gets one `document`
/// symbol, so a claimed data file (`.svg`, `.json`) still has a symbol.
struct ToyParser {
    language: &'static str,
    extensions: &'static [&'static str],
}

impl mct_core::LanguageParser for ToyParser {
    fn language_id(&self) -> &'static str {
        self.language
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut names: Vec<(String, usize)> = Vec::new();
        for (line_no, line) in file.contents.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            for pair in words.windows(2) {
                if pair[0] == "function" || pair[0] == "fn" {
                    let name = pair[1].split(['(', '<']).next().unwrap_or_default();
                    names.push((name.to_string(), line_no));
                }
            }
        }
        if names.is_empty() {
            names.push(("document".to_string(), 0));
        }
        let symbols = names
            .into_iter()
            .enumerate()
            .map(|(id, (name, line))| SymbolRecord {
                id: id as u32,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: line as u32 + 1,
                    column: 1,
                    byte_len: 1,
                    end_line: None,
                },
                parent: None,
                level: None,
            })
            .collect();
        Ok(ParsedFile {
            symbols,
            ..Default::default()
        })
    }
}

pub fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    for (language, extensions) in [
        ("typescript", &["ts", "mts"][..]),
        ("rust", &["rs"][..]),
        ("markup", &["svg"][..]),
        ("json", &["json"][..]),
    ] {
        registry.register(Arc::new(ToyParser {
            language,
            extensions,
        }));
    }
    registry
}

fn fixture_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/path-catalog")
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A scratch copy of the fixture, plus the files `.gitignore` keeps out of
/// the repository.
pub fn fixture() -> PathBuf {
    let dir = scratch_dir::scratch_dir("mct-path-catalog");
    copy_tree(&fixture_source(), &dir);
    fs::write(dir.join(".mctignore"), "ignored/\n").unwrap();
    fs::write(dir.join("bin/tool.exe"), b"MZ").unwrap();
    dir
}

fn indexed_fixture() -> (PathBuf, Index) {
    let dir = fixture();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::for_project(&dir)).unwrap();
    index.reindex(&registry(), false).unwrap();
    (dir, index)
}

fn indexed_paths(index: &Index) -> Vec<String> {
    index
        .indexed_files()
        .unwrap()
        .into_iter()
        .map(|f| f.relative_path)
        .collect()
}

#[test]
fn source_files_keep_their_symbols() {
    let (_dir, index) = indexed_fixture();
    let paths = indexed_paths(&index);
    assert!(paths.contains(&"src/app.ts".to_string()), "{paths:?}");
    assert!(paths.contains(&"src/lib.rs".to_string()), "{paths:?}");
    let greet = index.find_symbol("greet").unwrap();
    assert_eq!(greet.len(), 1);
    assert_eq!(greet[0].relative_path, "src/app.ts");
    assert_eq!(index.find_symbol("answer").unwrap().len(), 1);
}

/// The baseline before the catalog: only parsed files and the ADR-003
/// asset formats get a `files` row, so most non-source paths are missing.
#[test]
fn baseline_files_rows_cover_only_parsed_files_and_assets() {
    let (_dir, index) = indexed_fixture();
    let paths = indexed_paths(&index);
    for present in [
        "brand/logo.svg",
        "data/config.json",
        "img/photo.JPG",
        "models/robot.glb",
    ] {
        assert!(paths.contains(&present.to_string()), "{present}: {paths:?}");
    }
    // A `.ts` holding MPEG-TS packets has NUL bytes: dropped as binary.
    for missing in [
        "docs/manual.pdf",
        "demo/audio/intro.mp3",
        "sheets/budget.xlsx",
        "demo/video/broadcast.ts",
    ] {
        assert!(!paths.contains(&missing.to_string()), "{missing}");
    }
    for excluded in [
        "node_modules/pkg/logo.svg",
        "secrets/manual.pdf",
        "ignored/logo.svg",
    ] {
        assert!(!paths.contains(&excluded.to_string()), "{excluded}");
    }
}
