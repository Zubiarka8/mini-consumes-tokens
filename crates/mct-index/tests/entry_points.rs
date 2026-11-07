//! Possible entry points (issue #273, ADR-027): a parser may declare that a
//! framework could load a symbol by convention. The symbol stays a dead-code
//! candidate; the hint is stored with the symbol, follows it through
//! reindexing, and is shown only while the project fact it depends on holds.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[path = "common/scratch_dir.rs"]
mod scratch_dir;
use scratch_dir::scratch_dir;

use mct_core::{
    EntryPointHint, LanguageRegistry, Location, PackageRequirement, ParseError, ParsedFile,
    SourceFile, SymbolKind, SymbolRecord,
};
use mct_index::{scan_dead_code, ExcludeSet, Index};

/// `class Name` declares a class; a `class Name` line followed by
/// `# hint <manifest> <package> <min major>` attaches a hint to it, and
/// `# hint -` a hint with no requirement.
struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix("class ") {
                parsed.symbols.push(SymbolRecord {
                    id: parsed.symbols.len() as u32,
                    name: name.trim().to_string(),
                    kind: SymbolKind::Class,
                    location: Location {
                        line: line_no as u32 + 1,
                        column: 1,
                        byte_len: line.len() as u32,
                        end_line: Some(line_no as u32 + 1),
                    },
                    parent: None,
                    level: None,
                });
            } else if let Some(rest) = line.strip_prefix("# hint ") {
                let symbol = parsed.symbols.len() as u32 - 1;
                let parts: Vec<_> = rest.split_whitespace().collect();
                let requires = (parts != ["-"]).then(|| PackageRequirement {
                    manifest_path: parts[0].to_string(),
                    package: parts[1].to_string(),
                    min_major: parts[2].parse().unwrap(),
                });
                parsed.entry_points.push(EntryPointHint {
                    symbol,
                    framework: "fake".to_string(),
                    rule: "convention".to_string(),
                    evidence: "matches the fake convention".to_string(),
                    requires,
                });
            }
        }
        Ok(parsed)
    }
}

fn tempdir() -> PathBuf {
    scratch_dir("mct-entry-points")
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn composer(constraint: &str) -> String {
    format!(r#"{{ "require": {{ "laravel/framework": "{constraint}" }} }}"#)
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

/// `(name, notes)` of every dead-code candidate.
fn candidates(index: &Index) -> Vec<(String, Vec<String>)> {
    let scan = scan_dead_code(index, None, None).unwrap();
    scan.candidates
        .iter()
        .map(|&id| {
            (
                scan.symbols[id].name.clone(),
                scan.possible_entry_points(id)
                    .iter()
                    .map(|note| note.describe())
                    .collect(),
            )
        })
        .collect()
}

fn indexed(root: &Path) -> Index {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

const REQUIRES_LARAVEL_11: &str = "# hint composer.json laravel/framework 11\n";

#[test]
fn a_hint_whose_requirement_holds_is_shown_and_the_candidate_stays() {
    let root = tempdir();
    write(&root, "composer.json", &composer("^12.0"));
    write(
        &root,
        "a.fake",
        &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
    );
    let found = candidates(&indexed(&root));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "Ship");
    let [note] = found[0].1.as_slice() else {
        panic!("one note expected: {found:?}")
    };
    assert!(
        note.starts_with("possible entry point (fake, convention)"),
        "{note}"
    );
    assert!(note.contains("Not proven"), "{note}");
}

#[test]
fn a_candidate_without_a_hint_is_listed_without_notes() {
    let root = tempdir();
    write(&root, "composer.json", &composer("^12.0"));
    write(&root, "a.fake", "class Plain\n");
    assert_eq!(candidates(&indexed(&root)), [("Plain".to_string(), vec![])]);
}

#[test]
fn a_hint_is_dropped_when_its_requirement_does_not_hold() {
    for (manifest, label) in [
        (None, "no manifest at all"),
        (
            Some((
                "composer.json",
                r#"{ "require": { "symfony/console": "^7.0" } }"#.to_string(),
            )),
            "package not declared",
        ),
        (
            Some(("composer.json", composer("^10.0"))),
            "major below the checked one",
        ),
        (
            Some(("other/composer.json", composer("^12.0"))),
            "manifest in another directory",
        ),
    ] {
        let root = tempdir();
        if let Some((path, contents)) = manifest {
            write(&root, path, &contents);
        }
        write(
            &root,
            "a.fake",
            &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
        );
        let found = candidates(&indexed(&root));
        assert_eq!(found.len(), 1, "{label}: {found:?}");
        assert!(found[0].1.is_empty(), "{label}: {found:?}");
    }
}

#[test]
fn a_constraint_whose_major_cannot_be_read_still_counts_as_possible() {
    for constraint in ["*", "dev-main"] {
        let root = tempdir();
        write(&root, "composer.json", &composer(constraint));
        write(
            &root,
            "a.fake",
            &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
        );
        let found = candidates(&indexed(&root));
        assert_eq!(found[0].1.len(), 1, "{constraint}: {found:?}");
    }
}

#[test]
fn every_application_of_a_monorepo_is_judged_by_its_own_manifest() {
    let root = tempdir();
    write(&root, "apps/shop/composer.json", &composer("^12.0"));
    write(
        &root,
        "apps/shop/a.fake",
        "class Shop\n# hint apps/shop/composer.json laravel/framework 11\n",
    );
    write(&root, "apps/legacy/composer.json", &composer("^10.0"));
    write(
        &root,
        "apps/legacy/a.fake",
        "class Legacy\n# hint apps/legacy/composer.json laravel/framework 11\n",
    );
    write(
        &root,
        "apps/plain/a.fake",
        "class Plain\n# hint apps/plain/composer.json laravel/framework 11\n",
    );
    let mut found = candidates(&indexed(&root));
    found.sort();
    let shown: Vec<_> = found
        .iter()
        .map(|(name, notes)| (name.as_str(), notes.len()))
        .collect();
    assert_eq!(shown, [("Legacy", 0), ("Plain", 0), ("Shop", 1)]);
}

#[test]
fn a_hint_without_a_requirement_is_always_shown() {
    let root = tempdir();
    write(&root, "a.fake", "class Ship\n# hint -\n");
    assert_eq!(candidates(&indexed(&root))[0].1.len(), 1);
}

#[test]
fn editing_only_the_manifest_changes_whether_the_hint_is_shown() {
    let root = tempdir();
    write(&root, "composer.json", &composer("^12.0"));
    write(
        &root,
        "a.fake",
        &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
    );
    let mut index = indexed(&root);
    assert_eq!(candidates(&index)[0].1.len(), 1);

    write(&root, "composer.json", &composer("^10.0"));
    index.reindex(&registry(), false).unwrap();
    assert!(candidates(&index)[0].1.is_empty());

    write(&root, "composer.json", &composer("^13.0"));
    index.reindex(&registry(), false).unwrap();
    assert_eq!(candidates(&index)[0].1.len(), 1);
}

/// Rows in `symbol_entry_points` of the persisted index at `db`.
fn stored_hints(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row("SELECT COUNT(*) FROM symbol_entry_points", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn editing_the_source_updates_or_removes_the_hint_and_deleting_it_removes_the_row() {
    let root = tempdir();
    write(&root, "composer.json", &composer("^12.0"));
    write(
        &root,
        "a.fake",
        &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
    );
    let db = root.join("index.sqlite3");
    let mut index = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(candidates(&index)[0].1.len(), 1);
    assert_eq!(stored_hints(&db), 1);

    write(&root, "a.fake", "class Ship\n");
    index.reindex(&registry(), false).unwrap();
    assert!(candidates(&index)[0].1.is_empty());
    assert_eq!(stored_hints(&db), 0);

    write(
        &root,
        "a.fake",
        &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
    );
    index.reindex(&registry(), false).unwrap();
    assert_eq!(candidates(&index)[0].1.len(), 1);
    assert_eq!(stored_hints(&db), 1);

    fs::remove_file(root.join("a.fake")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(candidates(&index).is_empty());
    assert_eq!(stored_hints(&db), 0, "the hint must go with its symbol");
}

#[test]
fn a_hint_about_a_symbol_the_file_does_not_declare_is_ignored() {
    struct Wrong;
    impl mct_core::LanguageParser for Wrong {
        fn language_id(&self) -> &'static str {
            "wrong"
        }
        fn file_extensions(&self) -> &'static [&'static str] {
            &["wrong"]
        }
        fn parse(&self, _: &SourceFile) -> Result<ParsedFile, ParseError> {
            Ok(ParsedFile {
                entry_points: vec![EntryPointHint {
                    symbol: 99,
                    framework: "x".into(),
                    rule: "y".into(),
                    evidence: "z".into(),
                    requires: None,
                }],
                ..ParsedFile::default()
            })
        }
    }
    let root = tempdir();
    write(&root, "a.wrong", "anything\n");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(Wrong));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert!(candidates(&index).is_empty());
}

#[test]
fn an_index_from_before_the_table_gains_it_and_reparses_on_a_normal_reindex() {
    let root = tempdir();
    write(&root, "composer.json", &composer("^12.0"));
    write(
        &root,
        "a.fake",
        &format!("class Ship\n{REQUIRES_LARAVEL_11}"),
    );
    let db = root.join("old.sqlite3");
    {
        let mut index = Index::open(&root, &db, ExcludeSet::default()).unwrap();
        index.reindex(&registry(), false).unwrap();
    }
    // Rewind to the schema before the table existed, as an older binary left it.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("DROP TABLE symbol_entry_points;")
        .unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
    drop(conn);

    let mut index = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1, "the migration forces a reparse");
    assert_eq!(candidates(&index)[0].1.len(), 1);
}

#[test]
fn without_any_hint_nothing_is_added_to_the_scan() {
    let root = tempdir();
    write(&root, "a.fake", "class Plain\n");
    let scan = scan_dead_code(&indexed(&root), None, None).unwrap();
    assert!(scan.entry_points.is_empty());
}
