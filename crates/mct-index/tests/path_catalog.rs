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

fn class_of(index: &Index, path: &str) -> (String, String, String) {
    let entry = index
        .catalog_entry(path)
        .unwrap()
        .unwrap_or_else(|| panic!("{path} is not catalogued"));
    (entry.category, entry.format, entry.evidence)
}

fn class(category: &str, format: &str, evidence: &str) -> (String, String, String) {
    (category.into(), format.into(), evidence.into())
}

#[test]
fn a_new_index_catalogues_paths_no_parser_claims() {
    let (_dir, index) = indexed_fixture();
    assert_eq!(
        class_of(&index, "docs/manual.pdf"),
        class("document", "pdf", "extension")
    );
    assert_eq!(
        class_of(&index, "demo/audio/intro.mp3"),
        class("audio", "mp3", "extension")
    );
    assert_eq!(
        class_of(&index, "other/blob.xyz"),
        class("other", "", "unknown")
    );
    let pdf = index.catalog_entry("docs/manual.pdf").unwrap().unwrap();
    assert_eq!(pdf.size_bytes, Some(9));
}

#[test]
fn a_file_with_a_parser_keeps_its_symbols_and_gets_a_catalog_row() {
    let (_dir, index) = indexed_fixture();
    assert_eq!(
        class_of(&index, "brand/logo.svg"),
        class("vector", "svg", "extension")
    );
    assert_eq!(
        class_of(&index, "data/config.json"),
        class("data", "json", "extension")
    );
    assert_eq!(
        class_of(&index, "src/lib.rs"),
        class("source", "rust", "parser")
    );
    let documents: Vec<String> = index
        .find_symbol("document")
        .unwrap()
        .into_iter()
        .map(|s| s.relative_path)
        .collect();
    assert!(
        documents.contains(&"brand/logo.svg".to_string()),
        "{documents:?}"
    );
    assert!(
        documents.contains(&"data/config.json".to_string()),
        "{documents:?}"
    );
}

#[test]
fn reindexing_the_same_tree_keeps_one_row_per_path() {
    let (_dir, mut index) = indexed_fixture();
    let before = index.catalog_entry("img/photo.JPG").unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert!(!report.published, "an unchanged tree must not publish");
    index.reindex(&registry(), true).unwrap();
    assert_eq!(index.catalog_entry("img/photo.JPG").unwrap(), before);
}

#[test]
fn an_index_file_reopened_on_disk_serves_the_catalog() {
    let dir = fixture();
    let db = dir.join(".mct-index/index.sqlite3");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    {
        let mut index = Index::open(&dir, &db, ExcludeSet::for_project(&dir)).unwrap();
        index.reindex(&registry(), false).unwrap();
    }
    let index = Index::open(&dir, &db, ExcludeSet::for_project(&dir)).unwrap();
    assert_eq!(
        class_of(&index, "sheets/budget.xlsx"),
        class("spreadsheet", "xlsx", "extension")
    );
}

fn catalogued(index: &Index, path: &str) -> bool {
    index.catalog_entry(path).unwrap().is_some()
}

#[test]
fn add_rename_and_delete_converge_on_a_full_reindex() {
    let (dir, mut index) = indexed_fixture();
    fs::write(dir.join("docs/new manual.pdf"), b"%PDF-1.7\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(catalogued(&index, "docs/new manual.pdf"));

    // A rename that also changes the extension reclassifies the file.
    fs::rename(
        dir.join("docs/new manual.pdf"),
        dir.join("docs/new manual.epub"),
    )
    .unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(!catalogued(&index, "docs/new manual.pdf"));
    assert_eq!(
        class_of(&index, "docs/new manual.epub"),
        class("ebook", "epub", "extension")
    );

    fs::remove_file(dir.join("docs/new manual.epub")).unwrap();
    fs::remove_dir_all(dir.join("demo")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(!catalogued(&index, "docs/new manual.epub"));
    assert!(!catalogued(&index, "demo/audio/intro.mp3"));
    assert!(catalogued(&index, "docs/manual.pdf"));
}

#[test]
fn watcher_updates_converge_on_add_rename_and_delete() {
    let (dir, mut index) = indexed_fixture();
    let registry = registry();
    fs::write(dir.join("demo/audio/outro.ogg"), b"OggS").unwrap();
    index
        .reindex_paths(&registry, &[dir.join("demo/audio/outro.ogg")])
        .unwrap();
    assert_eq!(
        class_of(&index, "demo/audio/outro.ogg"),
        class("audio", "ogg", "extension")
    );

    // Reported by its new path only, as macOS FSEvents does, into another
    // directory and with another extension.
    fs::create_dir_all(dir.join("archive")).unwrap();
    fs::rename(
        dir.join("demo/audio/outro.ogg"),
        dir.join("archive/outro.opus"),
    )
    .unwrap();
    index
        .reindex_paths(&registry, &[dir.join("archive/outro.opus")])
        .unwrap();
    assert!(!catalogued(&index, "demo/audio/outro.ogg"));
    assert_eq!(
        class_of(&index, "archive/outro.opus"),
        class("audio", "opus", "extension")
    );

    fs::remove_dir_all(dir.join("fonts")).unwrap();
    index
        .reindex_paths(&registry, &[dir.join("fonts")])
        .unwrap();
    assert!(!catalogued(&index, "fonts/Inter.woff2"));
    assert!(catalogued(&index, "archive/outro.opus"));
}

#[test]
fn a_reindex_that_fails_midway_keeps_the_previous_catalog() {
    struct Exploding;
    impl mct_core::LanguageParser for Exploding {
        fn language_id(&self) -> &'static str {
            "boom"
        }
        fn file_extensions(&self) -> &'static [&'static str] {
            &["boom"]
        }
        fn parse(&self, _file: &SourceFile) -> Result<ParsedFile, ParseError> {
            panic!("parser crashed mid-run");
        }
    }
    let (dir, mut index) = indexed_fixture();
    let mut exploding = registry();
    exploding.register(Arc::new(Exploding));
    fs::remove_file(dir.join("docs/manual.pdf")).unwrap();
    fs::write(dir.join("zz.boom"), "x").unwrap();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        index.reindex(&exploding, false)
    }));
    assert!(crashed.is_err());
    // Rolled back: the run never reached the sweep, and nothing it wrote
    // before the crash was kept either.
    assert!(catalogued(&index, "docs/manual.pdf"));
    assert!(catalogued(&index, "sheets/budget.xlsx"));
    assert!(!catalogued(&index, "zz.boom"));
    fs::remove_file(dir.join("zz.boom")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(!catalogued(&index, "docs/manual.pdf"));
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_keeps_its_catalog_rows() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut index) = indexed_fixture();
    let sheets = dir.join("sheets");
    fs::set_permissions(&sheets, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&sheets).is_ok(); // root ignores permissions
    index.reindex(&registry(), false).unwrap();
    fs::set_permissions(&sheets, fs::Permissions::from_mode(0o755)).unwrap();
    if !readable {
        assert!(catalogued(&index, "sheets/budget.xlsx"));
    }
}

#[test]
fn excluded_paths_never_enter_and_newly_excluded_ones_leave() {
    let (dir, mut index) = indexed_fixture();
    for excluded in [
        "node_modules/pkg/logo.svg",
        "secrets/manual.pdf",
        "ignored/logo.svg",
        "slides/keynote.key",
    ] {
        assert!(!catalogued(&index, excluded), "{excluded}");
    }
    fs::write(dir.join(".mctignore"), "ignored/\nbooks/\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(!catalogued(&index, "books/novel.epub"));
    assert!(catalogued(&index, "docs/manual.pdf"));
}

#[test]
fn a_new_parser_reclassifies_unchanged_files_once() {
    let (_dir, mut index) = indexed_fixture();
    assert_eq!(class_of(&index, "other/blob.xyz").0, "other");
    let mut wider = registry();
    wider.register(Arc::new(ToyParser {
        language: "xyz",
        extensions: &["xyz"],
    }));
    index.reindex(&wider, false).unwrap();
    assert_eq!(
        class_of(&index, "other/blob.xyz"),
        class("source", "xyz", "parser")
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_is_catalogued_at_its_target_only() {
    let (dir, mut index) = indexed_fixture();
    std::os::unix::fs::symlink(dir.join("docs/manual.pdf"), dir.join("manual-link.pdf")).unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), dir.join("outside")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(catalogued(&index, "docs/manual.pdf"));
    assert!(!catalogued(&index, "manual-link.pdf"));
    assert!(!catalogued(&index, "outside"));
}
