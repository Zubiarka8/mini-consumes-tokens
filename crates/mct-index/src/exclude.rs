use globset::{Glob, GlobSet, GlobSetBuilder};

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
