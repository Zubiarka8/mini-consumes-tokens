//! Covers the two halves of the agent-worktree fix: the exclusion patterns
//! for `.claude/` and `.claude-index/` (which kept two full copies of this
//! repo in the index), and the `WalkDir::filter_entry` pruning that stops the
//! walk from descending into an excluded directory at all.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index};

// --- pattern-level -------------------------------------------------------

#[test]
fn claude_agent_worktrees_are_excluded() {
    let set = ExcludeSet::default();
    assert!(set.is_excluded(".claude"));
    assert!(set.is_excluded(".claude/settings.json"));
    assert!(set.is_excluded(".claude/worktrees"));
    assert!(set.is_excluded(".claude/worktrees/agent-a91ea5ed15bdbe91b/crates/mct-index/src/lib.rs"));
    // Nested under a subdirectory, not just at the repo root.
    assert!(set.is_excluded("crates/mct-cli/.claude/worktrees/agent-x/main.rs"));
}

#[test]
fn the_stale_pre_rename_index_directory_is_excluded() {
    let set = ExcludeSet::default();
    assert!(set.is_excluded(".claude-index"));
    assert!(set.is_excluded(".claude-index/index.sqlite3"));
    // The current index directory is still excluded too.
    assert!(set.is_excluded(".mct-index/index.sqlite3"));
}

#[test]
fn the_directory_entry_itself_matches_not_only_its_contents() {
    // The walk prunes by testing the *directory* entry, so every excluded
    // directory must match on its own path, not only on its contents.
    // Regression guard: the previous one-pattern form `**/dir{,/**}` looked
    // like it covered both, but globset drops the empty alternation branch
    // and compiles it to plain `**/dir/**`, so no directory entry ever
    // matched and nothing could be pruned.
    let set = ExcludeSet::default();
    for dir in [
        ".claude",
        ".claude-index",
        "target",
        ".git",
        "node_modules",
        "vendor",
        "secrets",
        ".aws",
        ".mct-index",
    ] {
        assert!(set.is_excluded(dir), "`{dir}` must match as a directory entry");
        assert!(
            set.is_excluded(&format!("{dir}/inner.rs")),
            "`{dir}` must still match its contents"
        );
        assert!(
            set.is_excluded(&format!("crates/nested/{dir}")),
            "`{dir}` must match at any depth, not only at the root"
        );
    }
}

#[test]
fn names_that_merely_contain_claude_are_not_excluded() {
    let set = ExcludeSet::default();
    assert!(!set.is_excluded("src/claude.rs"));
    assert!(!set.is_excluded("myclaude/mod.rs"));
    assert!(!set.is_excluded("crates/claude-api/src/lib.rs"));
    assert!(!set.is_excluded("claude-index/main.rs"));
    assert!(!set.is_excluded(".claude-indexer/main.rs"));
    assert!(!set.is_excluded("docs/.claude.md"));
}

// --- walk-level ----------------------------------------------------------

#[test]
fn an_excluded_directory_is_not_indexed() {
    let dir = tempdir();
    fs::create_dir_all(dir.join(".claude/worktrees/agent-x/src")).unwrap();
    fs::create_dir_all(dir.join(".claude-index")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src/real.fake"), "fn real\n").unwrap();
    fs::write(dir.join(".claude/worktrees/agent-x/src/real.fake"), "fn ghost\n").unwrap();
    fs::write(dir.join(".claude-index/leftover.fake"), "fn leftover\n").unwrap();
    // A manifest and an unsupported-language file inside the pruned subtree:
    // neither may reach `dependencies` or `index_issues` either.
    fs::write(dir.join(".claude/worktrees/agent-x/Cargo.toml"), "[dependencies]\nserde = \"1\"\n")
        .unwrap();
    fs::write(dir.join(".claude/worktrees/agent-x/script.rb"), "puts 'hi'\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_parsed, 1, "only the real source file is parsed");
    assert!(report.issues.is_empty(), "a pruned subtree reports no issues");

    assert_eq!(index.find_symbol("real").unwrap().len(), 1);
    assert!(index.find_symbol("ghost").unwrap().is_empty());
    assert!(index.find_symbol("leftover").unwrap().is_empty());

    let status = index.status().unwrap();
    assert_eq!(status.total_files, 1);
    assert!(status.dependencies.is_empty(), "no manifest inside a pruned subtree");
    assert!(status.unsupported_languages.is_empty());
}

#[test]
fn a_normal_source_tree_is_still_indexed_in_full() {
    // The exclusion must not over-match: names that merely contain "claude"
    // are ordinary source and have to survive the walk.
    let dir = tempdir();
    fs::create_dir_all(dir.join("myclaude")).unwrap();
    fs::create_dir_all(dir.join("crates/claude-api/src")).unwrap();

    fs::write(dir.join("claude.fake"), "fn top_level\n").unwrap();
    fs::write(dir.join("myclaude/helper.fake"), "fn in_myclaude\n").unwrap();
    fs::write(dir.join("crates/claude-api/src/lib.fake"), "fn in_claude_api\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_parsed, 3);
    for name in ["top_level", "in_myclaude", "in_claude_api"] {
        assert_eq!(index.find_symbol(name).unwrap().len(), 1, "`{name}` must be indexed");
    }
}

#[test]
fn a_file_that_becomes_excluded_is_removed_on_the_next_reindex() {
    // The real-world case: a directory already in the index turns into an
    // excluded one (here by being renamed, as an agent worktree effectively
    // was when `.claude` joined the default patterns). The file keeps its own
    // name, so only the exclusion can account for it disappearing.
    let dir = tempdir();
    fs::create_dir_all(dir.join("workspace")).unwrap();
    fs::write(dir.join("workspace/a.fake"), "fn doomed\n").unwrap();
    fs::write(dir.join("keep.fake"), "fn kept\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.find_symbol("doomed").unwrap().len(), 1, "precondition");

    fs::rename(dir.join("workspace"), dir.join(".claude")).unwrap();

    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_removed, 1, "the now-excluded file's row is swept");
    assert!(
        index.find_symbol("doomed").unwrap().is_empty(),
        "a file that becomes excluded must not linger in the index"
    );
    assert_eq!(index.find_symbol("kept").unwrap().len(), 1);
    assert_eq!(index.status().unwrap().total_files, 1);
}

#[test]
fn a_root_directory_whose_name_matches_an_exclusion_is_still_walked() {
    // `filter_entry` must not reject the walk root itself: a project checked
    // out into a directory literally named `target` still has to index.
    let dir = tempdir().join("target");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.fake"), "fn at_root\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_parsed, 1);
    assert_eq!(index.find_symbol("at_root").unwrap().len(), 1);
}

// --- harness -------------------------------------------------------------

/// Toy format: each line is `fn NAME`. A trimmed copy of the parser in
/// `reindex.rs` — these tests only need symbol extraction, not relations.
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
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if parts.next() != Some("fn") {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            }
            parsed.symbols.push(SymbolRecord {
                id: parsed.symbols.len() as u32,
                name: parts.next().unwrap_or_default().to_string(),
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
            });
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

/// A fresh temp directory, matching the convention in `reindex.rs`.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-exclude-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}
