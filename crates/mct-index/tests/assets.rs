//! Asset pre-pass (ADR-003 D1, D3, D7): asset files get a `files` row and
//! one `asset` symbol, bad glTF JSON keeps the symbol and records a
//! `syntax_error`, and a reference path resolves only to an in-root file.
//!
//! The toy parser (`.a`) reads `def NAME [loads SPEC]` lines; `loads` is a
//! `references` relation to the asset `SPEC` names, qualified the way a web
//! parser would (`resolve_reference_path`, language `asset`, kind `Asset`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::{
    resolve_reference_path, Location, ParseError, ParsedFile, RelationKind, RelationTarget,
    SourceFile, SymbolKind, SymbolRecord, SymbolRelation,
};
use mct_index::{ExcludeSet, Index, Resolution, UnsupportedKind, MAX_GLTF_JSON_BYTES};

struct LoaderParser;

impl mct_core::LanguageParser for LoaderParser {
    fn language_id(&self) -> &'static str {
        "alpha"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["a"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let location = Location {
                line: line_no as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: None,
            };
            let ["def", name, rest @ ..] = words.as_slice() else {
                continue;
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: name.to_string(),
                kind: SymbolKind::Function,
                location,
                parent: None,
                level: None,
            });
            let ["loads", spec] = rest else { continue };
            let Some(path) = resolve_reference_path(&file.relative_path, spec) else {
                continue;
            };
            let to_name = path.rsplit('/').next().unwrap_or_default().to_string();
            parsed.relation_targets.push(RelationTarget {
                relation: parsed.relations.len(),
                path: Some(path),
                language: Some("asset".to_string()),
                kind: Some(SymbolKind::Asset),
                ..Default::default()
            });
            parsed.relations.push(SymbolRelation {
                from: id,
                kind: RelationKind::References,
                to_name,
                location,
            });
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(LoaderParser));
    registry
}

fn tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-index-assets-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(dir: &Path, path: &str, contents: &[u8]) {
    let file = dir.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, contents).unwrap();
}

/// A sparse file of `len` bytes: big on paper, nothing on disk.
fn write_sparse(dir: &Path, path: &str, len: u64) {
    let file = dir.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::File::create(file).unwrap().set_len(len).unwrap();
}

fn glb(declared_json_len: u32, json: &[u8]) -> Vec<u8> {
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((20 + json.len() as u32).to_le_bytes());
    out.extend(declared_json_len.to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out
}

/// `path kind name` of every asset symbol.
fn asset_symbols(index: &Index) -> Vec<String> {
    let files = index.file_hashes().unwrap();
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    let mut rows: Vec<String> = index
        .symbols_in_files(&paths)
        .unwrap()
        .into_iter()
        .filter(|s| s.language == "asset")
        .map(|s| format!("{} {} {} L{}", s.relative_path, s.kind, s.name, s.line))
        .collect();
    rows.sort();
    rows
}

fn syntax_errors(index: &Index) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = index
        .status()
        .unwrap()
        .syntax_errors
        .into_iter()
        .map(|i| (i.relative_path, i.detail))
        .collect();
    rows.sort();
    rows
}

#[test]
fn every_asset_extension_gets_one_asset_symbol_and_is_hash_skipped() {
    let dir = tempdir();
    let json = br#"{"asset":{"version":"2.0"}}"#;
    write(
        &dir,
        "public/models/robot.glb",
        &glb(json.len() as u32, json),
    );
    write(&dir, "public/models/robot.gltf", json);
    for ext in ["png", "JPG", "jpeg", "webp", "ktx2", "hdr", "exr"] {
        // Not valid image data: images are only stat'ed.
        write(&dir, &format!("img/a.{ext}"), b"\xff\x00garbage");
    }
    write(&dir, "notes.txt", b"not an asset");

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.files_parsed, 9);
    assert_eq!(
        asset_symbols(&index),
        vec![
            "img/a.JPG asset a.JPG L1",
            "img/a.exr asset a.exr L1",
            "img/a.hdr asset a.hdr L1",
            "img/a.jpeg asset a.jpeg L1",
            "img/a.ktx2 asset a.ktx2 L1",
            "img/a.png asset a.png L1",
            "img/a.webp asset a.webp L1",
            "public/models/robot.glb asset robot.glb L1",
            "public/models/robot.gltf asset robot.gltf L1",
        ]
    );
    let status = index.status().unwrap();
    let asset = status
        .languages
        .iter()
        .find(|l| l.language == "asset")
        .unwrap();
    assert_eq!((asset.file_count, asset.symbol_count), (9, 9));

    let again = index.reindex(&registry(), false).unwrap();
    assert_eq!((again.files_parsed, again.files_unchanged), (0, 9));
    let forced = index.reindex(&registry(), true).unwrap();
    assert_eq!(forced.files_parsed, 9);

    fs::remove_file(dir.join("img/a.png")).unwrap();
    let removed = index.reindex(&registry(), false).unwrap();
    assert_eq!(removed.files_removed, 1);
    assert_eq!(asset_symbols(&index).len(), 8);
}

#[test]
fn images_have_no_size_cap() {
    let dir = tempdir();
    // Over the 16 MiB source read limit: an image is never read.
    write_sparse(&dir, "big.hdr", 64 * 1024 * 1024);
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(asset_symbols(&index), vec!["big.hdr asset big.hdr L1"]);
}

#[test]
fn bad_gltf_json_keeps_the_asset_and_records_a_syntax_error() {
    let dir = tempdir();
    let over = MAX_GLTF_JSON_BYTES as u32 + 1;
    let mut oversized = glb(over, b"{}");
    oversized.resize(oversized.len() + over as usize, b' ');
    write(&dir, "oversized.glb", &oversized);
    write(&dir, "truncated.glb", &glb(2, b"{}")[..10]);
    write(&dir, "short-chunk.glb", &glb(50, b"{}"));
    write(&dir, "not-glb.glb", b"PK\x03\x04 a zip, not a glb file");
    write(&dir, "latin1.glb", &glb(3, b"{\xe9}"));
    write(&dir, "malformed.gltf", b"{\"asset\": ");
    write_sparse(&dir, "huge.gltf", MAX_GLTF_JSON_BYTES + 1);

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert!(report
        .issues
        .iter()
        .all(|i| i.kind == UnsupportedKind::SyntaxError));
    assert_eq!(asset_symbols(&index).len(), 7);
    let errors = syntax_errors(&index);
    let detail = |path: &str| {
        errors
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, d)| d.as_str())
            .unwrap_or_else(|| panic!("no syntax_error for {path}: {errors:?}"))
    };
    assert_eq!(
        detail("oversized.glb"),
        "glb: JSON chunk 8388609 B exceeds 8388608 B cap"
    );
    assert_eq!(detail("truncated.glb"), "glb: truncated (10 B)");
    assert_eq!(
        detail("short-chunk.glb"),
        "glb: JSON chunk 50 B exceeds the 22 B file"
    );
    assert_eq!(detail("not-glb.glb"), "glb: bad magic");
    assert_eq!(detail("latin1.glb"), "glb: JSON is not UTF-8");
    assert!(detail("malformed.gltf").starts_with("gltf: "));
    assert_eq!(detail("huge.gltf"), "gltf: 8388609 B exceeds 8388608 B cap");

    // Unchanged: hash-skipped, and the issues stay with their assets.
    let again = index.reindex(&registry(), false).unwrap();
    assert_eq!(again.files_unchanged, 7);
    assert_eq!(syntax_errors(&index).len(), 7);

    // Fixed: the issue goes, the asset stays.
    write(&dir, "malformed.gltf", b"{\"asset\": {}}  ");
    index.reindex(&registry(), true).unwrap();
    assert!(syntax_errors(&index)
        .iter()
        .all(|(p, _)| p != "malformed.gltf"));
    assert_eq!(asset_symbols(&index).len(), 7);
}

#[test]
fn a_reference_resolves_only_to_an_asset_file_inside_the_root() {
    let outside = tempdir();
    write(&outside, "secret.glb", &glb(2, b"{}"));
    let dir = tempdir();
    write(&dir, "public/models/robot.glb", &glb(2, b"{}"));
    write(&dir, "src/textures/wood.png", b"png");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.join("secret.glb"),
        dir.join("public/models/secret.glb"),
    )
    .unwrap();
    write(
        &dir,
        "src/components/Robot.a",
        b"def model loads /models/robot.glb?url\n\
          def wood loads ../textures/wood.png\n\
          def secret loads /models/secret.glb\n\
          def missing loads /models/gone.glb\n\
          def escape loads ../../../robot.glb\n\
          def remote loads https://cdn.example.com/robot.glb\n",
    );

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(asset_symbols(&index)
        .iter()
        .all(|s| !s.contains("secret.glb")));

    let resolution = |from: &str| {
        let mut hits = index
            .find_dependencies_scoped(from, mct_index::QueryScope::default())
            .unwrap();
        assert_eq!(hits.len(), 1, "{from}: {hits:?}");
        let hit = hits.remove(0);
        (hit.resolution, hit.target_id)
    };
    let (model, target) = resolution("model");
    assert_eq!(model, Resolution::Resolved);
    let target = index.symbols_by_ids(&[target.unwrap()]).unwrap();
    assert_eq!(target[0].relative_path, "public/models/robot.glb");
    assert_eq!(resolution("wood").0, Resolution::Resolved);
    #[cfg(unix)]
    assert_eq!(resolution("secret").0, Resolution::Unresolved);
    assert_eq!(resolution("missing").0, Resolution::Unresolved);
    // A path the resolver rejects records no relation at all.
    for from in ["escape", "remote"] {
        assert!(index
            .find_dependencies_scoped(from, mct_index::QueryScope::default())
            .unwrap()
            .is_empty());
    }
}

/// D7: an index written before assets were indexed (same schema, no asset
/// rows although the asset files are on disk) opens with no migration and
/// gains the asset rows on a normal, unforced reindex.
#[test]
fn an_index_from_before_assets_gains_them_on_a_normal_reindex() {
    let dir = tempdir();
    write(&dir, "src/a.a", b"def main\n");
    write(&dir, "public/models/robot.glb", &glb(2, b"{}"));
    let db = tempdir().join("index.sqlite3");
    {
        let mut index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
        index.reindex(&registry(), false).unwrap();
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM files WHERE language = 'asset'", [])
        .unwrap();
    let version = |conn: &rusqlite::Connection| -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    };
    let before = version(&conn);
    // ADR-003 D7: no phase adds a migration. Bump only with a new ADR.
    assert_eq!(before, 10);
    drop(conn);

    let mut index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
    assert!(asset_symbols(&index).is_empty());
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!((report.files_parsed, report.files_unchanged), (1, 1));
    assert_eq!(
        asset_symbols(&index),
        vec!["public/models/robot.glb asset robot.glb L1"]
    );
    drop(index);
    assert_eq!(version(&rusqlite::Connection::open(&db).unwrap()), before);
}
