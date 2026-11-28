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
use mct_index::{ExcludeSet, FileQueryInput, Index};

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

fn paths_of(page: &mct_index::FilePage) -> Vec<&str> {
    page.entries
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect()
}

fn find(index: &Index, input: FileQueryInput<'_>) -> mct_index::FilePage {
    index.find_files(&input).unwrap()
}

#[test]
fn a_category_and_path_filter_lists_the_demo_audio_in_path_order() {
    let (_dir, index) = indexed_fixture();
    let page = find(
        &index,
        FileQueryInput {
            category: Some("audio"),
            path: Some("demo/"),
            ..Default::default()
        },
    );
    assert_eq!(
        paths_of(&page),
        [
            "demo/audio/clip.opus",
            "demo/audio/intro.mp3",
            "demo/audio/song.flac",
            "demo/audio/theme.mid",
            "demo/audio/voice.WAV",
        ]
    );
    assert_eq!(page.next_offset, None);
}

#[test]
fn a_format_alias_and_a_query_find_every_svg_logo() {
    let (_dir, index) = indexed_fixture();
    for alias in ["svg", "SVG", ".svg"] {
        let page = find(
            &index,
            FileQueryInput {
                format: Some(alias),
                query: Some("LOGO"),
                ..Default::default()
            },
        );
        assert_eq!(
            paths_of(&page),
            ["brand/Logo-Dark.SVG", "brand/logo.svg"],
            "{alias}"
        );
    }
}

#[test]
fn jpg_and_jpeg_name_the_same_format() {
    let (_dir, index) = indexed_fixture();
    let by = |alias| {
        paths_of(&find(
            &index,
            FileQueryInput {
                format: Some(alias),
                ..Default::default()
            },
        ))
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    };
    assert_eq!(by("jpg"), by("JPEG"));
    assert!(by("jpg").contains(&"img/photo.JPG".to_string()));
    assert!(by("jpg").contains(&"img/photo2.jpeg".to_string()));
}

#[test]
fn query_words_all_have_to_match() {
    let (_dir, index) = indexed_fixture();
    let page = find(
        &index,
        FileQueryInput {
            query: Some("demo intro"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&page), ["demo/audio/intro.mp3"]);
}

#[test]
fn unicode_and_spaces_match_case_insensitively() {
    let (_dir, index) = indexed_fixture();
    let page = find(
        &index,
        FileQueryInput {
            query: Some("spec ÜNÏCODE"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&page), ["docs/Spec Ünïcode.docx"]);
}

#[test]
fn pages_follow_on_without_repeats_or_gaps() {
    let (_dir, index) = indexed_fixture();
    let all = find(
        &index,
        FileQueryInput {
            limit: Some(200),
            ..Default::default()
        },
    );
    let mut paged: Vec<String> = Vec::new();
    let mut offset = Some(0);
    while let Some(start) = offset {
        let page = find(
            &index,
            FileQueryInput {
                limit: Some(7),
                offset: Some(start),
                ..Default::default()
            },
        );
        assert_eq!(page.total, all.total);
        paged.extend(page.entries.iter().map(|e| e.relative_path.clone()));
        offset = page.next_offset;
    }
    let expected: Vec<String> = all
        .entries
        .iter()
        .map(|e| e.relative_path.clone())
        .collect();
    assert_eq!(paged, expected);
    let mut sorted = expected.clone();
    sorted.sort();
    assert_eq!(expected, sorted);
}

#[test]
fn a_path_filter_matches_a_whole_segment_only() {
    let (dir, mut index) = indexed_fixture();
    fs::create_dir_all(dir.join("demo-old")).unwrap();
    fs::write(dir.join("demo-old/a.mp3"), b"ID3").unwrap();
    index.reindex(&registry(), false).unwrap();
    let page = find(
        &index,
        FileQueryInput {
            path: Some("demo"),
            category: Some("audio"),
            ..Default::default()
        },
    );
    assert!(
        paths_of(&page).iter().all(|p| p.starts_with("demo/")),
        "{:?}",
        paths_of(&page)
    );
    let file = find(
        &index,
        FileQueryInput {
            path: Some("./docs/manual.pdf"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&file), ["docs/manual.pdf"]);
}

#[test]
fn invalid_filters_are_rejected_with_the_valid_values() {
    let (_dir, index) = indexed_fixture();
    let err = |input: FileQueryInput<'_>| index.find_files(&input).unwrap_err().to_string();
    assert!(err(FileQueryInput {
        category: Some("pictures"),
        ..Default::default()
    })
    .contains("image, vector"));
    assert!(err(FileQueryInput {
        format: Some("nope"),
        ..Default::default()
    })
    .contains("unknown format `nope`"));
    assert!(err(FileQueryInput {
        limit: Some(0),
        ..Default::default()
    })
    .contains("out of range"));
    assert!(err(FileQueryInput {
        limit: Some(201),
        ..Default::default()
    })
    .contains("out of range"));
    assert!(err(FileQueryInput {
        path: Some("../x"),
        ..Default::default()
    })
    .contains("relative"));
    assert!(err(FileQueryInput {
        path: Some("/etc"),
        ..Default::default()
    })
    .contains("relative"));
}

#[test]
fn a_language_id_filters_source_files() {
    let (_dir, index) = indexed_fixture();
    let page = find(
        &index,
        FileQueryInput {
            format: Some("rust"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&page), ["src/lib.rs"]);
    let ts = find(
        &index,
        FileQueryInput {
            format: Some("typescript"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&ts), ["src/app.ts"]);
    let mpeg = find(
        &index,
        FileQueryInput {
            format: Some("ts"),
            ..Default::default()
        },
    );
    assert_eq!(paths_of(&mpeg), ["demo/video/broadcast.ts"]);
}

#[test]
fn an_empty_index_returns_an_empty_page() {
    let dir = scratch_dir::scratch_dir("mct-empty-catalog");
    let index = Index::open_in_memory(&dir, ExcludeSet::for_project(&dir)).unwrap();
    let page = find(&index, FileQueryInput::default());
    assert_eq!(
        (page.total, page.catalogued, page.next_offset),
        (0, 0, None)
    );
    assert!(index
        .find_files(&FileQueryInput {
            format: Some("pdf"),
            ..Default::default()
        })
        .is_ok());
}

#[test]
fn the_status_counts_catalogued_files_by_category() {
    let (_dir, index) = indexed_fixture();
    let status = index.status().unwrap();
    let count = |category: &str| {
        status
            .catalog
            .iter()
            .find(|c| c.category == category)
            .map_or(0, |c| c.files)
    };
    assert_eq!(count("audio"), 5);
    assert!(count("other") >= 2, "{:?}", status.catalog);
    assert!(status.catalog_doubtful >= 1);
    let files: Vec<usize> = status.catalog.iter().map(|c| c.files).collect();
    let mut sorted = files.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(files, sorted);
}

/// Every file of the fixture with the category and format it must get:
/// one or more per family, uppercase and compound suffixes, names
/// without an extension and the doubtful readings.
const EXPECTED: &[(&str, &str, &str)] = &[
    // Walked like any file; a leading dot is not an extension.
    (".mctignore", "other", ""),
    ("LICENSE", "document", "plain-text"),
    ("README", "document", "plain-text"),
    ("archives/backup.7z", "archive", "7z"),
    ("archives/bundle.TGZ", "archive", "tar.gz"),
    ("archives/bundle.tar.gz", "archive", "tar.gz"),
    ("archives/disk.iso", "archive", "iso"),
    ("archives/dist.tar.zst", "archive", "tar.zst"),
    ("archives/source.zip", "archive", "zip"),
    ("bin/app.wasm", "binary", "webassembly"),
    ("bin/tool.exe", "binary", "windows-executable"),
    ("bin/libfoo.so.1.2", "binary", "shared-library"),
    ("bin/native.dll", "binary", "windows-library"),
    ("bin/plugin.jar", "binary", "java-archive"),
    ("bin/setup.msi", "binary", "windows-installer"),
    ("books/comic.cbz", "ebook", "comic-zip"),
    ("books/novel.epub", "ebook", "epub"),
    ("brand/Logo-Dark.SVG", "vector", "svg"),
    ("brand/flow.drawio", "vector", "drawio"),
    ("brand/logo.eps", "vector", "eps"),
    ("brand/logo.svg", "vector", "svg"),
    ("brand/logo.svgz", "vector", "svgz"),
    ("cad/part.step", "model3d", "step"),
    ("cad/plan.dwg", "model3d", "dwg"),
    ("data/analysis.ipynb", "data", "jupyter-notebook"),
    ("data/config.json", "data", "json"),
    ("data/events.jsonl", "data", "json-lines"),
    ("data/feed.xml", "data", "xml"),
    ("data/settings.yaml", "data", "yaml"),
    ("data/table.parquet", "data", "parquet"),
    ("db/analytics.duckdb", "database", "duckdb"),
    ("db/app.sqlite", "database", "sqlite"),
    ("db/legacy.db", "database", "sqlite"),
    ("db/other.db", "database", "db"),
    ("db/schema.sql", "database", "sql"),
    ("demo/audio/clip.opus", "audio", "opus"),
    ("demo/audio/intro.mp3", "audio", "mp3"),
    ("demo/audio/song.flac", "audio", "flac"),
    ("demo/audio/theme.mid", "audio", "midi"),
    ("demo/audio/voice.WAV", "audio", "wav"),
    ("demo/subs/clip.srt", "subtitle", "srt"),
    ("demo/subs/clip.vtt", "subtitle", "webvtt"),
    ("demo/video/broadcast.ts", "video", "mpeg-ts"),
    ("demo/video/clip.mp4", "video", "mp4"),
    ("demo/video/clip.webm", "video", "webm"),
    ("design/mock.psd", "design", "photoshop"),
    ("design/ui.fig", "design", "figma"),
    ("design/ui.sketch", "design", "sketch"),
    ("docs/Spec Ünïcode.docx", "document", "docx"),
    ("docs/graph.dot", "vector", "graphviz"),
    ("docs/legacy.doc", "document", "doc"),
    ("docs/letter.dot", "document", "dot"),
    ("docs/manual.pdf", "document", "pdf"),
    ("docs/notes.rtf", "document", "rtf"),
    ("docs/report.odt", "document", "odt"),
    ("fonts/Inter.otf", "font", "opentype"),
    ("fonts/Inter.ttf", "font", "truetype"),
    ("fonts/Inter.woff2", "font", "woff2"),
    ("fonts/family.ttc", "font", "ttc"),
    ("fonts/notes.ttc", "other", ""),
    ("geo/area.geojson", "geospatial", "geojson"),
    ("geo/elevation.tif", "geospatial", "geotiff"),
    ("geo/places.kml", "geospatial", "kml"),
    ("geo/track.gpx", "geospatial", "gpx"),
    ("img/anim.gif", "image", "gif"),
    ("img/hero.avif", "image", "avif"),
    ("img/hero.webp", "image", "webp"),
    ("img/icon.ico", "image", "ico"),
    ("img/map.tif", "image", "tiff"),
    ("img/phone.HEIC", "image", "heif"),
    ("img/photo.JPG", "image", "jpeg"),
    ("img/photo2.jpeg", "image", "jpeg"),
    ("img/pic.png", "image", "png"),
    ("img/pointer.cur", "image", "cur"),
    ("img/raw/IMG_0001.CR2", "image", "canon-raw"),
    ("img/raw/shot.dng", "image", "dng"),
    ("img/scan.jfif", "image", "jpeg"),
    ("img/sky.exr", "image", "openexr"),
    ("img/studio.hdr", "image", "radiance-hdr"),
    ("img/texture.dds", "image", "dds"),
    ("mail/meeting.ics", "email", "icalendar"),
    ("mail/message.eml", "email", "eml"),
    ("models/build.obj", "binary", "coff-object"),
    ("models/chair.obj", "model3d", "wavefront-obj"),
    ("models/house.blend", "model3d", "blender"),
    ("models/part.stl", "model3d", "stl"),
    ("models/robot.glb", "model3d", "glb"),
    ("models/scene.usdz", "model3d", "usd"),
    ("nested/deeper/still/logo.png", "image", "png"),
    ("other/Makefile.bak", "other", ""),
    ("other/blob.xyz", "other", ""),
    ("sheets/budget.xlsx", "spreadsheet", "xlsx"),
    ("sheets/data.csv", "spreadsheet", "csv"),
    ("sheets/data.tsv", "spreadsheet", "tsv"),
    ("sheets/macro.xlsm", "spreadsheet", "xlsm"),
    ("sheets/old.XLS", "spreadsheet", "xls"),
    ("slides/deck.pptx", "presentation", "pptx"),
    ("slides/talk.odp", "presentation", "odp"),
    ("src/app.ts", "source", "typescript"),
    ("src/lib.rs", "source", "rust"),
];

#[test]
fn every_family_is_classified() {
    let (_dir, index) = indexed_fixture();
    let page = index
        .find_files(&FileQueryInput {
            limit: Some(200),
            ..Default::default()
        })
        .unwrap();
    let mut actual: Vec<(&str, &str, &str)> = page
        .entries
        .iter()
        .map(|e| {
            (
                e.relative_path.as_str(),
                e.category.as_str(),
                e.format.as_str(),
            )
        })
        .collect();
    actual.sort_unstable();
    let mut expected = EXPECTED.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

// Unix only: `set_len` makes a sparse file there; NTFS may allocate it.
#[cfg(unix)]
#[test]
fn a_huge_file_is_catalogued_from_its_metadata_and_a_bounded_prefix() {
    let (dir, mut index) = indexed_fixture();
    // Sparse: 8 GiB on paper, a few blocks on disk. The `.mp4` is never
    // opened; the `.tif` is read for at most SIGNATURE_BYTES.
    let size: u64 = 8 << 30;
    for name in ["demo/video/huge.mp4", "demo/video/huge.tif"] {
        let file = fs::File::create(dir.join(name)).unwrap();
        file.set_len(size).unwrap();
    }
    let started = std::time::Instant::now();
    index.reindex(&registry(), false).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let entry = index.catalog_entry("demo/video/huge.mp4").unwrap().unwrap();
    assert_eq!(
        (entry.category.as_str(), entry.size_bytes),
        ("video", Some(size))
    );
    let tif = index.catalog_entry("demo/video/huge.tif").unwrap().unwrap();
    assert_eq!(
        (tif.category.as_str(), tif.format.as_str()),
        ("image", "tiff")
    );
    assert!(tif.note.is_some(), "no TIFF header: {tif:?}");
}

#[test]
fn a_name_without_an_extension_is_findable_and_explained() {
    let (dir, mut index) = indexed_fixture();
    fs::write(dir.join("other/Dockerfile.unknownthing"), "FROM x").unwrap();
    fs::write(dir.join("other/BLOB"), [0u8, 1, 2]).unwrap();
    index.reindex(&registry(), false).unwrap();
    let blob = index.catalog_entry("other/BLOB").unwrap().unwrap();
    assert_eq!(
        (blob.category.as_str(), blob.evidence.as_str()),
        ("other", "unknown")
    );
    assert_eq!(blob.note.as_deref(), Some("no extension"));
    let page = index
        .find_files(&FileQueryInput {
            query: Some("blob"),
            ..Default::default()
        })
        .unwrap();
    let paths: Vec<&str> = page
        .entries
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect();
    assert_eq!(paths, ["other/BLOB", "other/blob.xyz"]);
}

#[test]
fn an_index_from_before_the_catalog_fills_it_on_a_normal_reindex() {
    let dir = fixture();
    let db = dir.join(".mct-index/index.sqlite3");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    {
        let mut index = Index::open(&dir, &db, ExcludeSet::for_project(&dir)).unwrap();
        index.reindex(&registry(), false).unwrap();
    }
    // Rewind to schema 33, as a binary from before migration 34 left it.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("DROP TABLE path_catalog;").unwrap();
    conn.pragma_update(None, "user_version", 33).unwrap();
    drop(conn);

    let mut index = Index::open(&dir, &db, ExcludeSet::for_project(&dir)).unwrap();
    assert!(
        !catalogued(&index, "docs/manual.pdf"),
        "the migration adds an empty table"
    );
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 0, "no content hash was invalidated");
    assert_eq!(
        class_of(&index, "docs/manual.pdf"),
        class("document", "pdf", "extension")
    );
    assert_eq!(
        class_of(&index, "src/lib.rs"),
        class("source", "rust", "parser")
    );
}
