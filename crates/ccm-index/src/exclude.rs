use globset::{Glob, GlobSet, GlobSetBuilder};

/// Patterns excluded from indexing by default because they commonly hold
/// secrets, across the conventions of several ecosystems — not just
/// Node/Python. Opt-out (a user can widen this), never opt-in: a fresh
/// install must never index a `.env` file by accident.
const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    // generic
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/secrets/**",
    "**/secret/**",
    "**/credentials.json",
    "**/.aws/**",
    // Node / JS
    "**/node_modules/**",
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
    "**/.git/**",
    "**/target/**",
    "**/.claude-index/**",
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
            builder.add(Glob::new(pattern).expect("built-in exclude pattern is valid"));
        }
        for pattern in extra_patterns {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
            }
        }
        Self {
            set: builder.build().expect("exclude glob set builds"),
        }
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
