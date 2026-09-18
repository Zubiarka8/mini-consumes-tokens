use globset::{Glob, GlobSet, GlobSetBuilder};

/// Patterns excluded from indexing by default because they commonly hold
/// secrets, across the conventions of several ecosystems — not just
/// Node/Python. Opt-out (a user can widen this), never opt-in: a fresh
/// install must never index a `.env` file by accident.
// Directory patterns use `{,/**}` rather than a bare trailing `/**`: in
// globset, "dir/**" matches everything *inside* dir but not dir itself, so a
// filesystem event for the directory's own entry (e.g. its mtime changing
// when a child is written) would otherwise slip past the filter unexcluded.
const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    // generic
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/secrets{,/**}",
    "**/secret{,/**}",
    "**/credentials.json",
    "**/.aws{,/**}",
    // Node / JS
    "**/node_modules{,/**}",
    // PHP (Composer)
    "**/vendor{,/**}",
    // .NET
    "**/appsettings.*.json",
    "**/*.pfx",
    "**/*.snk",
    // Java / Gradle / Maven
    "**/gradle.properties",
    // Go (Viper and similar config libraries)
    "**/*.env.local",
    "**/config/secrets.yaml",
    // general VCS / build output
    "**/.git{,/**}",
    "**/target{,/**}",
    "**/.ccm-index{,/**}",
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

    /// `relative_path` uses forward slashes, matching [`ccm_core::SourceFile::relative_path`].
    pub fn is_excluded(&self, relative_path: &str) -> bool {
        self.set.is_match(relative_path)
    }
}

impl Default for ExcludeSet {
    fn default() -> Self {
        Self::new(&[])
    }
}
