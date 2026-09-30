//! An `ExcludeSet::for_project` picks up edits to `.mctignore` (and to an
//! imported `.gitignore`) without reopening the index: a rule added later
//! drops the file's symbols and relations, a rule removed later brings it
//! back — through a full reindex (forced or not) or through `reindex_paths`
//! of the edited ignore file, which is all a watcher reports.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use mct_index::{ExcludeSet, Index, IGNORE_FILE_NAME};

/// Toy format: each line is `NAME` or `NAME calls OTHER`.
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
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let location = Location {
                line: line_no as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: Some(line_no as u32 + 1),
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
            if let (Some("calls"), Some(callee)) = (parts.next(), parts.next()) {
                parsed.relations.push(SymbolRelation {
                    from: id,
                    kind: RelationKind::Calls,
                    to_name: callee.to_string(),
                    location,
                });
            }
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-exclude-reload-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A project with `hidden.fake` (whose symbol calls `helper`) and
/// `keep.fake` (defining `helper`), indexed with the project's own rules.
fn project() -> (PathBuf, Index) {
    let dir = tempdir();
    fs::write(
        dir.join("hidden.fake"),
        "audit_hidden_symbol calls helper\n",
    )
    .unwrap();
    fs::write(dir.join("keep.fake"), "helper\n").unwrap();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::for_project(&dir)).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(is_indexed(&index));
    (dir, index)
}

/// Whether `hidden.fake`'s symbol and its call to `helper` are indexed.
fn is_indexed(index: &Index) -> bool {
    let symbol = !index.find_symbol("audit_hidden_symbol").unwrap().is_empty();
    let relation = !index.find_callers("helper").unwrap().is_empty();
    assert_eq!(symbol, relation, "symbol and relation must go together");
    symbol
}

fn write_rules(dir: &Path, contents: &str) -> PathBuf {
    let path = dir.join(IGNORE_FILE_NAME);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn a_forced_reindex_applies_a_rule_added_after_opening() {
    let (dir, mut index) = project();

    write_rules(&dir, "hidden.fake\n");
    index.reindex(&registry(), true).unwrap();
    assert!(!is_indexed(&index), "a new rule must drop the file");
    assert_eq!(index.find_symbol("helper").unwrap().len(), 1);

    fs::remove_file(dir.join(IGNORE_FILE_NAME)).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(
        is_indexed(&index),
        "a removed rule must bring the file back"
    );
}

#[test]
fn reindexing_just_the_edited_ignore_file_applies_its_rules_everywhere() {
    let (dir, mut index) = project();

    let rules = write_rules(&dir, "hidden.fake\n");
    index
        .reindex_paths(&registry(), std::slice::from_ref(&rules))
        .unwrap();
    assert!(!is_indexed(&index));

    // Un-excluding reports no event for `hidden.fake` itself: only the
    // ignore file changed, and that must be enough.
    write_rules(&dir, "# nothing excluded\n");
    index.reindex_paths(&registry(), &[rules]).unwrap();
    assert!(is_indexed(&index));
}

#[test]
fn an_imported_gitignore_edit_applies_too() {
    let (dir, mut index) = project();
    let rules = write_rules(&dir, "@import-gitignore\n");
    index.reindex_paths(&registry(), &[rules]).unwrap();
    assert!(is_indexed(&index), "an empty import excludes nothing");

    let gitignore = dir.join(".gitignore");
    fs::write(&gitignore, "hidden.fake\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&gitignore))
        .unwrap();
    assert!(!is_indexed(&index));

    fs::write(&gitignore, "").unwrap();
    index.reindex_paths(&registry(), &[gitignore]).unwrap();
    assert!(is_indexed(&index));
}

#[test]
fn an_ordinary_change_stays_incremental() {
    let (dir, mut index) = project();
    write_rules(&dir, "# unrelated comment\n");
    let keep = dir.join("keep.fake");
    fs::write(&keep, "helper\nextra\n").unwrap();

    let report = index
        .reindex_paths(&registry(), &[keep, dir.join(IGNORE_FILE_NAME)])
        .unwrap();
    assert_eq!(report.files_parsed, 1);
    // A full walk would have hash-checked `hidden.fake` too.
    assert_eq!(report.files_unchanged, 0);
    assert!(is_indexed(&index));
}

#[test]
fn a_fixed_set_never_reloads_and_clones_share_reloaded_rules() {
    let dir = tempdir();
    write_rules(&dir, "hidden.fake\n");

    let fixed = ExcludeSet::new(&[]);
    assert!(!fixed.reload());
    assert!(!fixed.is_excluded("hidden.fake"));

    let project = ExcludeSet::for_project(&dir);
    let clone = project.clone();
    assert!(project.is_excluded("hidden.fake"));
    assert!(!clone.reload(), "unchanged rules are not a change");

    write_rules(&dir, "other.fake\n");
    assert!(clone.reload());
    assert!(!project.is_excluded("hidden.fake"));
    assert!(project.is_excluded("other.fake"));
}
