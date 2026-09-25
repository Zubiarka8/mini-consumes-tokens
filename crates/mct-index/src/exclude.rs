use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Name of the user-editable ignore file at a project's root, read by
/// [`read_ignore_file`]. Gitignore-flavored but deliberately simpler (no
/// negation — see that function's docs) and scoped to indexing only, never
/// touching git.
pub const IGNORE_FILE_NAME: &str = ".mctignore";

/// Starter content written by `mct-cli ignore-init` for a project that
/// doesn't have a `.mctignore` yet.
pub const IGNORE_FILE_TEMPLATE: &str = "\
# .mctignore — extra exclusions for mini-consumes-tokens indexing.
#
# One pattern per line. Lines starting with # are comments; blank lines are
# ignored. This only narrows what gets indexed on top of the built-in
# exclusions (secrets, node_modules, target, .git, ...) — it can never widen
# past them, and negation (!pattern) is not supported.
#
# A bare name with a trailing slash (e.g. \"docs/\") excludes that directory,
# and everything under it, at any depth.
# A path with an internal slash and a trailing slash (e.g. \"tests/fixtures/\")
# is anchored to the project root instead of matching everywhere.
# A bare glob (e.g. \"*.md\") excludes matching files at any depth; a glob
# containing a slash (e.g. \"docs/*.md\") is anchored to the project root.
#
# Examples (uncomment to use):
# *.md
# docs/
# tests/fixtures/
";

/// File patterns excluded from indexing by default because they commonly hold
/// secrets, across the conventions of several ecosystems — not just
/// Node/Python. Opt-out (a user can widen this), never opt-in: a fresh
/// install must never index a `.env` file by accident.
///
/// Whole directories go in [`DEFAULT_EXCLUDE_DIRS`] instead.
const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    // generic
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/credentials.json",
    // .NET
    "**/appsettings.*.json",
    "**/*.pfx",
    "**/*.snk",
    // Java / Gradle / Maven
    "**/gradle.properties",
    // Go (Viper and similar config libraries)
    "**/*.env.local",
    "**/config/secrets.yaml",
];

/// Directory names excluded wholesale, at any depth. Each expands to the
/// *pair* `**/<name>` and `**/<name>/**` — see the note in [`ExcludeSet::new`]
/// for why one pattern cannot express both.
const DEFAULT_EXCLUDE_DIRS: &[&str] = &[
    // generic
    "secrets",
    "secret",
    ".aws",
    // Node / JS
    "node_modules",
    // PHP (Composer)
    "vendor",
    // general VCS / build output
    ".git",
    "target",
    ".mct-index",
    // Agent / tooling state, not project source. `.claude/worktrees/` holds
    // Claude Code agent worktrees — full checkouts of the project itself, so
    // indexing it files every symbol two or three times over (measured in
    // this repo: 65.9% of all indexed symbols were worktree duplicates).
    // `.claude-index` is the pre-rename (ccm -> mct) index directory, still
    // sitting in older checkouts alongside the current `.mct-index`.
    ".claude",
    ".claude-index",
];

#[derive(Clone)]
pub struct ExcludeSet {
    set: GlobSet,
}

impl ExcludeSet {
    /// Builds the default exclude set. `extra_patterns` lets a project widen
    /// (never narrow) it via configuration.
    pub fn new(extra_patterns: &[String]) -> Self {
        let mut builder = GlobSetBuilder::new();
        for pattern in DEFAULT_EXCLUDE_PATTERNS {
            #[allow(clippy::expect_used)]
            // SAFETY: `pattern` is one of the hardcoded literals in
            // `DEFAULT_EXCLUDE_PATTERNS` above, not user input — a malformed
            // literal would be a compile-time-caught bug in this file, never
            // a runtime failure driven by an indexed repo.
            builder.add(Glob::new(pattern).expect("built-in exclude pattern is valid"));
        }
        // Two patterns per directory, never one. `**/dir/**` matches what is
        // *inside* the directory but not the directory's own entry, and the
        // seemingly equivalent one-liner `**/dir{,/**}` is not equivalent at
        // all: globset drops the empty alternation branch and compiles it to
        // exactly `**/dir/**`. Matching the entry itself is what lets the
        // indexer's walk prune an excluded directory instead of descending
        // into it, and what stops a filesystem event for the directory's own
        // entry (e.g. its mtime changing when a child is written) from
        // slipping past the watcher's filter unexcluded.
        for dir in DEFAULT_EXCLUDE_DIRS {
            for pattern in [format!("**/{dir}"), format!("**/{dir}/**")] {
                #[allow(clippy::expect_used)]
                // SAFETY: built from a hardcoded literal in
                // `DEFAULT_EXCLUDE_DIRS` above, same reasoning as the loop
                // over `DEFAULT_EXCLUDE_PATTERNS`.
                builder.add(Glob::new(&pattern).expect("built-in exclude pattern is valid"));
            }
        }
        for pattern in extra_patterns {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
            }
        }
        #[allow(clippy::expect_used)]
        // SAFETY: every glob added above came from a literal pattern or was
        // already filtered through `if let Ok(glob)`, so building the set can
        // never fail here.
        let set = builder.build().expect("exclude glob set builds");
        Self { set }
    }

    /// `relative_path` uses forward slashes, matching [`mct_core::SourceFile::relative_path`].
    pub fn is_excluded(&self, relative_path: &str) -> bool {
        self.set.is_match(relative_path)
    }
}

impl Default for ExcludeSet {
    fn default() -> Self {
        Self::new(&[])
    }
}

/// Reads `<root>/.mctignore` (see [`IGNORE_FILE_NAME`]) and turns each line
/// into a ready-to-use glob pattern for [`ExcludeSet::new`]'s
/// `extra_patterns`. A missing file yields an empty list, not an error — the
/// file is opt-in for the user, not a requirement.
///
/// Parsing rules (a deliberately simplified subset of `.gitignore` syntax —
/// no negation, since this set is opt-out only and a `!pattern` would let a
/// project widen back past the built-in exclusions):
/// - `#` at the start of a line is a comment; blank lines are skipped.
/// - A line ending in `/` is a directory: excluded wholesale, same
///   two-pattern-per-directory treatment as [`DEFAULT_EXCLUDE_DIRS`].
/// - Any other line is a glob pattern.
/// - In both cases, a pattern containing a `/` elsewhere than a trailing
///   position is anchored to the project root; one without an internal `/`
///   matches at any depth (prefixed with `**/`) — the same anchoring rule
///   `.gitignore` itself uses.
pub fn read_ignore_file(root: &Path) -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(root.join(IGNORE_FILE_NAME)) else {
        return Vec::new();
    };

    let mut patterns = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(dir) = line.strip_suffix('/') {
            if dir.contains('/') {
                patterns.push(dir.to_string());
                patterns.push(format!("{dir}/**"));
            } else {
                patterns.push(format!("**/{dir}"));
                patterns.push(format!("**/{dir}/**"));
            }
        } else if line.contains('/') {
            patterns.push(line.to_string());
        } else {
            patterns.push(format!("**/{line}"));
        }
    }
    patterns
}

#[cfg(test)]
mod ignore_file_tests {
    // Test code: an unwrap()/expect() here means a broken test precondition,
    // and panicking is the correct behavior — this module only touches
    // temp-dir fixtures this test creates itself, never repo-input content.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mct-ignore-file-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn a_missing_ignore_file_yields_no_patterns() {
        let dir = temp_dir("missing");
        assert_eq!(read_ignore_file(&dir), Vec::<String>::new());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let dir = temp_dir("comments");
        fs::write(dir.join(IGNORE_FILE_NAME), "# a comment\n\n   \n# another\n").unwrap();
        assert_eq!(read_ignore_file(&dir), Vec::<String>::new());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bare_directory_name_expands_to_the_any_depth_pair() {
        let dir = temp_dir("bare-dir");
        fs::write(dir.join(IGNORE_FILE_NAME), "docs/\n").unwrap();
        assert_eq!(read_ignore_file(&dir), vec!["**/docs".to_string(), "**/docs/**".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_nested_directory_path_is_anchored_to_the_root() {
        let dir = temp_dir("nested-dir");
        fs::write(dir.join(IGNORE_FILE_NAME), "tests/fixtures/\n").unwrap();
        assert_eq!(
            read_ignore_file(&dir),
            vec!["tests/fixtures".to_string(), "tests/fixtures/**".to_string()]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bare_glob_matches_at_any_depth() {
        let dir = temp_dir("bare-glob");
        fs::write(dir.join(IGNORE_FILE_NAME), "*.md\n").unwrap();
        assert_eq!(read_ignore_file(&dir), vec!["**/*.md".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_glob_with_a_slash_is_anchored_to_the_root() {
        let dir = temp_dir("anchored-glob");
        fs::write(dir.join(IGNORE_FILE_NAME), "docs/*.md\n").unwrap();
        assert_eq!(read_ignore_file(&dir), vec!["docs/*.md".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resulting_patterns_actually_exclude_matching_paths() {
        let dir = temp_dir("integration");
        fs::write(dir.join(IGNORE_FILE_NAME), "*.md\ndocs/\n").unwrap();
        let set = ExcludeSet::new(&read_ignore_file(&dir));
        assert!(set.is_excluded("README.md"));
        assert!(set.is_excluded("nested/notes.md"));
        assert!(set.is_excluded("docs/guide.txt"));
        assert!(!set.is_excluded("src/main.rs"));
        fs::remove_dir_all(&dir).ok();
    }
}
