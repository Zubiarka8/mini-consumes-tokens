use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Name of the user-editable ignore file at a project's root, read by
/// [`read_ignore_file`]. Gitignore-flavored but deliberately simpler (see
/// that function's docs) and scoped to indexing only, never touching git.
pub const IGNORE_FILE_NAME: &str = ".mctignore";

/// Starter content written by `mct-cli ignore-init` for a project that
/// doesn't have a `.mctignore` yet.
pub const IGNORE_FILE_TEMPLATE: &str = "\
# .mctignore — extra exclusions for mini-consumes-tokens indexing.
#
# One pattern per line. Lines starting with # are comments; blank lines are
# ignored. This only narrows what gets indexed on top of the built-in
# exclusions (secrets, node_modules, target, .git, ...) — it can never widen
# past them.
#
# A bare name with a trailing slash (e.g. \"docs/\") excludes that directory,
# and everything under it, at any depth.
# A path with an internal slash and a trailing slash (e.g. \"tests/fixtures/\")
# is anchored to the project root instead of matching everywhere.
# A bare glob (e.g. \"*.md\") excludes matching files at any depth; a glob
# containing a slash (e.g. \"docs/*.md\") is anchored to the project root.
#
# A line starting with \"!\" re-includes what an earlier line of this file
# excluded (e.g. \"*.md\" then \"!docs/readme.md\"); the last matching line
# wins. As in git, a file inside an excluded directory can't be re-included:
# write \"docs/*\" rather than \"docs/\" to keep \"!docs/readme.md\" working.
# Built-in exclusions can never be re-included.
#
# To also exclude everything the project's own .gitignore excludes (so
# nothing kept out of git ends up in the index either), keep this exact line
# on its own (it is on by default; comment it out to turn it off). Its lines,
# \"!\" included, are read as if written where the directive stands.
@import-gitignore
#
# Examples (uncomment to use):
# *.md
# docs/
# tests/fixtures/
# !docs/readme.md
";

/// Line that, added on its own in `.mctignore`, opts into also excluding
/// everything the project's `.gitignore` excludes — see [`read_ignore_file`].
pub const GITIGNORE_IMPORT_DIRECTIVE: &str = "@import-gitignore";

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
    // Next.js build output and cache: generated, often hundreds of MB, and
    // re-parsed after every `next build` otherwise.
    ".next",
    // Agent / tooling state, not project source. `.claude/worktrees/` holds
    // Claude Code agent worktrees — full checkouts of the project itself, so
    // indexing it files every symbol two or three times over (measured in
    // this repo: 65.9% of all indexed symbols were worktree duplicates).
    // `.claude-index` is this project's original index directory, from
    // before it was renamed `ccm` and then `mct`. `.ccm-index` is that
    // intermediate `ccm` era's index directory. Both can still turn up
    // alongside the current `.mct-index` — e.g. a stale globally-installed
    // `ccm-mcp-server` binary (predating the `ccm -> mct` rename) recreates
    // `.ccm-index` on every run if `.mcp.json` still points at it instead of
    // `mct-mcp-server`.
    ".claude",
    ".claude-index",
    ".ccm-index",
];

/// The paths kept out of the index: the built-in exclusions plus a project's
/// extra patterns.
///
/// A handle: clones share one set of rules, so the indexer's walk and the
/// server's watcher filter can never disagree. A set built by
/// [`ExcludeSet::for_project`] also remembers where its rules came from and
/// can pick up later edits to them ([`ExcludeSet::reload`]), which every
/// clone then sees at once.
#[derive(Clone)]
pub struct ExcludeSet {
    rules: Arc<RwLock<Rules>>,
}

struct Rules {
    set: Option<GlobSet>,
    /// What each glob in `set` does, by index.
    kinds: Vec<RuleKind>,
    /// The extra patterns `set` was built from.
    extra: Vec<String>,
    /// Project root whose ignore files the extra patterns were read from —
    /// `None` for a fixed set ([`ExcludeSet::new`]), which never reloads.
    project_root: Option<PathBuf>,
    /// How many times [`ExcludeSet::reload`] has swapped in new rules.
    generation: u64,
    /// Directories below the root holding a `.gitignore` that
    /// [`GITIGNORE_IMPORT_DIRECTIVE`] imports, found by the last discovering
    /// reload; a cheap reload re-reads only these.
    nested: Vec<String>,
}

impl ExcludeSet {
    /// Builds the default exclude set. `extra_patterns` lets a project widen
    /// (never narrow) it via configuration. The result is fixed:
    /// [`ExcludeSet::reload`] never changes it.
    /// A pattern starting with `!` re-includes what an earlier extra pattern
    /// excluded (never a built-in exclusion).
    pub fn new(extra_patterns: &[String]) -> Self {
        let (set, kinds) = build_set(extra_patterns);
        Self::from_rules(Rules {
            set,
            kinds,
            extra: extra_patterns.to_vec(),
            project_root: None,
            generation: 0,
            nested: Vec::new(),
        })
    }

    /// The exclude set for the project at `root`: the built-in exclusions
    /// plus whatever its ignore files ask for ([`read_ignore_file`]), re-read
    /// by every [`ExcludeSet::reload`] so an edit to them takes effect
    /// without restarting.
    pub fn for_project(root: &Path) -> Self {
        let nested = discover_nested_gitignores(root, &read_ignore_file(root));
        let extra = read_ignore_file_with(root, &nested);
        let (set, kinds) = build_set(&extra);
        Self::from_rules(Rules {
            set,
            kinds,
            extra,
            project_root: Some(root.to_path_buf()),
            generation: 0,
            nested,
        })
    }

    fn from_rules(rules: Rules) -> Self {
        Self {
            rules: Arc::new(RwLock::new(rules)),
        }
    }

    /// Re-reads the project's ignore files (a set built by
    /// [`ExcludeSet::for_project`] only) and swaps in the new rules if they
    /// differ. Returns whether they changed: a path that was, or now is,
    /// excluded produced no useful filesystem event of its own, so only a
    /// full walk brings the index in line with the new rules.
    pub fn reload(&self) -> bool {
        // read_ignore_contents logs non-missing read failures. Retain the
        // previous rules so the watcher's best-effort reload cannot expose
        // excluded content while a rule file is unreadable.
        self.try_reload_discovering().unwrap_or(false)
    }

    /// Checked reload for the MCP query boundary: a read error preserves the
    /// previous rules and rejects the operation. Re-reads the nested
    /// `.gitignore` files the last discovering reload found, without walking
    /// the tree for new ones.
    pub(crate) fn try_reload(&self) -> crate::Result<bool> {
        self.reload_checked(false)
    }

    /// [`ExcludeSet::try_reload`] that also walks the tree for nested
    /// `.gitignore` files created or removed since: for a full reindex, which
    /// walks the tree anyway, and for a rules-file event.
    pub(crate) fn try_reload_discovering(&self) -> crate::Result<bool> {
        self.reload_checked(true)
    }

    fn reload_checked(&self, discover: bool) -> crate::Result<bool> {
        let Some(root) = self.read().project_root.clone() else {
            return Ok(false);
        };
        let nested = if discover {
            discover_nested_gitignores(&root, &read_ignore_file_checked(&root, &[])?)
        } else {
            self.read().nested.clone()
        };
        let extra = read_ignore_file_checked(&root, &nested)?;
        if self.read().extra == extra {
            if discover {
                self.rules
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .nested = nested;
            }
            return Ok(false);
        }
        let (set, kinds) = build_set(&extra);
        let mut rules = self.rules.write().unwrap_or_else(PoisonError::into_inner);
        rules.set = set;
        rules.kinds = kinds;
        rules.extra = extra;
        rules.nested = nested;
        rules.generation = rules.generation.wrapping_add(1);
        Ok(true)
    }

    /// Changes with every [`ExcludeSet::reload`] that swapped in new rules,
    /// through any clone. Whoever reloads, an index compares it with the
    /// generation its last full reindex applied to tell whether it may still
    /// hold rows of paths that the current rules exclude
    /// ([`crate::Index::apply_exclusion_changes`]).
    pub fn generation(&self) -> u64 {
        self.read().generation
    }

    /// Whether `relative_path` (forward slashes, relative to the project
    /// root) is one of the files [`read_ignore_file`] reads — the files whose
    /// edits call for an [`ExcludeSet::reload`]. `.gitignore` counts even
    /// when not imported: whether it is imported is itself a rule that may
    /// have just changed, and reloading an unchanged set is a no-op.
    pub fn is_rules_file(relative_path: &str) -> bool {
        relative_path == IGNORE_FILE_NAME || relative_path == GITIGNORE_FILE_NAME
    }

    /// [`ExcludeSet::is_rules_file`], plus a nested `.gitignore` outside
    /// every excluded directory, which [`GITIGNORE_IMPORT_DIRECTIVE`] may
    /// import. One inside an excluded directory (`node_modules/x/.gitignore`)
    /// can never matter, and package installs write many.
    pub fn is_rules_path(&self, relative_path: &str) -> bool {
        Self::is_rules_file(relative_path)
            || relative_path
                .strip_suffix(GITIGNORE_FILE_NAME)
                .is_some_and(|dir| dir.ends_with('/'))
                && !self.is_excluded(relative_path)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Rules> {
        // A panic while holding the lock can't leave `Rules` half-written:
        // `reload` builds the new set before taking the write lock.
        self.rules.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// `relative_path` uses forward slashes, matching [`mct_core::SourceFile::relative_path`].
    ///
    /// Excluded when it or any ancestor directory is: a built-in pattern
    /// matches it, or the last extra pattern that matches it is not a `!`
    /// re-include. Checking ancestors keeps a single-path check (the
    /// watcher) in line with the walk, which never descends into an
    /// excluded directory — so, as in git, a `!` can't reach inside one.
    pub fn is_excluded(&self, relative_path: &str) -> bool {
        let rules = self.read();
        let Some(set) = rules.set.as_ref() else {
            return true;
        };
        let excludes = |path: &str| {
            let matches = set.matches(path);
            matches.iter().any(|&i| rules.kinds[i] == RuleKind::Builtin)
                || matches
                    .iter()
                    .max()
                    .is_some_and(|&i| rules.kinds[i] == RuleKind::Exclude)
        };
        relative_path
            .match_indices('/')
            .any(|(end, _)| excludes(&relative_path[..end]))
            || excludes(relative_path)
    }
}

/// What one glob in an [`ExcludeSet`] does when it matches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    /// A built-in exclusion: wins over everything.
    Builtin,
    /// An extra pattern.
    Exclude,
    /// A `!` extra pattern, re-including what an earlier extra one excluded.
    Include,
}

/// The project's own `.gitignore`, read when `.mctignore` imports it.
const GITIGNORE_FILE_NAME: &str = ".gitignore";

/// The built-in exclusions plus `extra_patterns` (malformed ones skipped),
/// with the [`RuleKind`] of each glob by index.
fn build_set(extra_patterns: &[String]) -> (Option<GlobSet>, Vec<RuleKind>) {
    let mut builder = GlobSetBuilder::new();
    let mut kinds = Vec::new();
    for pattern in DEFAULT_EXCLUDE_PATTERNS {
        #[allow(clippy::expect_used)]
        // SAFETY: `pattern` is one of the hardcoded literals in
        // `DEFAULT_EXCLUDE_PATTERNS` above, not user input — a malformed
        // literal would be a compile-time-caught bug in this file, never
        // a runtime failure driven by an indexed repo.
        builder.add(Glob::new(pattern).expect("built-in exclude pattern is valid"));
        kinds.push(RuleKind::Builtin);
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
            kinds.push(RuleKind::Builtin);
        }
    }
    for pattern in extra_patterns {
        let (pattern, kind) = match pattern.strip_prefix('!') {
            Some(rest) => (rest, RuleKind::Include),
            None => (pattern.as_str(), RuleKind::Exclude),
        };
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
            kinds.push(kind);
        }
    }
    // Valid globs can still exceed the regex compiler's resource limit.
    // Preserve built-in secret exclusions when custom rules cannot compile.
    match builder.build() {
        Ok(set) => (Some(set), kinds),
        Err(error) => {
            eprintln!("mct: exclusion rules could not compile: {error}; ignoring custom patterns");
            if extra_patterns.is_empty() {
                (None, Vec::new())
            } else {
                build_set(&[])
            }
        }
    }
}

impl Default for ExcludeSet {
    fn default() -> Self {
        Self::new(&[])
    }
}

fn read_ignore_contents(path: &Path) -> std::io::Result<String> {
    let result = crate::read_repository_file(path).and_then(|bytes| {
        String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    });
    if let Err(error) = &result {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "mct: cannot read exclusion rules {}: {error}",
                path.display()
            );
        }
    }
    result
}

/// Unlike the legacy opt-in reader below, a reload must distinguish an
/// absent rule file from a failed read. Discarding rules on a failed read
/// would re-index paths the last valid configuration excluded.
fn read_ignore_file_checked(root: &Path, nested: &[String]) -> crate::Result<Vec<String>> {
    let read_optional = |name: &str| {
        let path = root.join(name);
        match read_ignore_contents(&path) {
            Ok(contents) => Ok(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(source) => Err(crate::IndexError::Io {
                path: path.display().to_string(),
                source,
            }),
        }
    };
    let contents = read_optional(IGNORE_FILE_NAME)?;
    let mut patterns = Vec::new();
    for line in contents.lines() {
        if line.trim() == GITIGNORE_IMPORT_DIRECTIVE {
            push_lines(&read_optional(GITIGNORE_FILE_NAME)?, "", &mut patterns);
            for dir in nested {
                let gitignore = format!("{dir}/{GITIGNORE_FILE_NAME}");
                push_lines(&read_optional(&gitignore)?, dir, &mut patterns);
            }
        } else {
            push_lines(line, "", &mut patterns);
        }
    }
    Ok(patterns)
}

/// [`read_ignore_file`] plus the nested `.gitignore` files of `nested`, read
/// when the import directive is present; unreadable ones are skipped.
fn read_ignore_file_with(root: &Path, nested: &[String]) -> Vec<String> {
    let Ok(contents) = read_ignore_contents(&root.join(IGNORE_FILE_NAME)) else {
        return Vec::new();
    };
    let mut patterns = Vec::new();
    for line in contents.lines() {
        if line.trim() == GITIGNORE_IMPORT_DIRECTIVE {
            if let Ok(gitignore) = read_ignore_contents(&root.join(GITIGNORE_FILE_NAME)) {
                push_lines(&gitignore, "", &mut patterns);
            }
            for dir in nested {
                let path = root.join(dir).join(GITIGNORE_FILE_NAME);
                if let Ok(gitignore) = read_ignore_contents(&path) {
                    push_lines(&gitignore, dir, &mut patterns);
                }
            }
        } else {
            push_lines(line, "", &mut patterns);
        }
    }
    patterns
}

/// The directories below `root` (relative, `/`-separated, sorted so a parent
/// precedes its children and a deeper file's rules win) that hold a
/// `.gitignore`, when `.mctignore` imports `.gitignore` at all. The walk
/// never enters a directory the built-in exclusions or `base` (the rules read
/// so far) exclude, nor follows a symlink.
fn discover_nested_gitignores(root: &Path, base: &[String]) -> Vec<String> {
    let imports = read_ignore_contents(&root.join(IGNORE_FILE_NAME)).is_ok_and(|contents| {
        contents
            .lines()
            .any(|line| line.trim() == GITIGNORE_IMPORT_DIRECTIVE)
    });
    if !imports {
        return Vec::new();
    }
    let probe = ExcludeSet::new(base);
    let relative = |path: &Path| {
        path.strip_prefix(root).ok().and_then(|p| {
            let parts: Option<Vec<&str>> = p.components().map(|c| c.as_os_str().to_str()).collect();
            parts.map(|parts| parts.join("/"))
        })
    };
    let mut found: Vec<String> = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| {
            entry.file_type().is_dir()
                && relative(entry.path()).is_some_and(|rel| !probe.is_excluded(&rel))
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join(GITIGNORE_FILE_NAME).is_file())
        .filter_map(|entry| relative(entry.path()))
        .collect();
    found.sort();
    found
}

/// Reads `<root>/.mctignore` (see [`IGNORE_FILE_NAME`]) and turns each line
/// into a ready-to-use glob pattern for [`ExcludeSet::new`]'s
/// `extra_patterns`. A missing file yields an empty list, not an error — the
/// file is opt-in for the user, not a requirement.
///
/// Parsing rules (a deliberately simplified subset of `.gitignore` syntax):
/// - `#` at the start of a line is a comment; blank lines are skipped.
/// - A line starting with `!` re-includes what an earlier line excluded; its
///   patterns come out prefixed with `!` (see [`ExcludeSet::is_excluded`]
///   for precedence). It can never re-include a built-in exclusion.
/// - A line ending in `/` is a directory: excluded wholesale, same
///   two-pattern-per-directory treatment as [`DEFAULT_EXCLUDE_DIRS`].
/// - Any other line is a glob pattern.
/// - In both cases, a pattern containing a `/` elsewhere than a trailing
///   position is anchored to the project root; one without an internal `/`
///   matches at any depth (prefixed with `**/`) — the same anchoring rule
///   `.gitignore` itself uses, leading `/` included (`/target` is only the
///   root's `target`).
/// - A line that is exactly [`GITIGNORE_IMPORT_DIRECTIVE`] opts into also
///   reading `<root>/.gitignore` and parsing its lines the same way (a
///   missing `.gitignore` is silently a no-op), as if its lines, `!` ones
///   included, stood where the directive does.
pub fn read_ignore_file(root: &Path) -> Vec<String> {
    let Ok(contents) = read_ignore_contents(&root.join(IGNORE_FILE_NAME)) else {
        return Vec::new();
    };

    let mut patterns = Vec::new();
    for line in contents.lines() {
        if line.trim() == GITIGNORE_IMPORT_DIRECTIVE {
            if let Ok(gitignore) = read_ignore_contents(&root.join(GITIGNORE_FILE_NAME)) {
                push_lines(&gitignore, "", &mut patterns);
            }
        } else {
            push_lines(line, "", &mut patterns);
        }
    }
    patterns
}

/// Parses each line of `contents` (comments and blank lines skipped) into
/// `patterns`, a `!` line's patterns prefixed with `!`. `dir` is the
/// directory the file sits in (empty for the root): as in git, its patterns
/// apply only below it.
fn push_lines(contents: &str, dir: &str, patterns: &mut Vec<String>) {
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_prefix('!') {
            Some(rest) if !rest.is_empty() => {
                let start = patterns.len();
                push_line_patterns(rest, dir, patterns);
                for pattern in &mut patterns[start..] {
                    pattern.insert(0, '!');
                }
            }
            Some(_) => {}
            None => push_line_patterns(line, dir, patterns),
        }
    }
}

/// Turns one already-trimmed, non-empty, non-comment ignore-file line into
/// its ready-to-use glob pattern(s), per the anchoring rules documented on
/// [`read_ignore_file`], appending them to `patterns`.
fn push_line_patterns(line: &str, dir: &str, patterns: &mut Vec<String>) {
    let (body, is_dir) = match line.strip_suffix('/') {
        Some(dir) => (dir, true),
        None => (line, false),
    };
    // A leading `/` only anchors (`/target` is `target` at the root);
    // globs match paths relative to the root, which never start with one.
    let anchored = body.contains('/');
    let body = body.strip_prefix('/').unwrap_or(body);
    if body.is_empty() {
        return;
    }
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let base = if anchored {
        format!("{prefix}{body}")
    } else {
        format!("{prefix}**/{body}")
    };
    let inside = is_dir.then(|| format!("{base}/**"));
    patterns.push(base);
    patterns.extend(inside);
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
        let dir =
            std::env::temp_dir().join(format!("mct-ignore-file-test-{tag}-{}", std::process::id()));
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
        fs::write(
            dir.join(IGNORE_FILE_NAME),
            "# a comment\n\n   \n# another\n",
        )
        .unwrap();
        assert_eq!(read_ignore_file(&dir), Vec::<String>::new());
        fs::remove_dir_all(&dir).ok();
    }

    fn assert_ignore_rules(label: &str, pattern: &str, expected: &[&str]) {
        let dir = temp_dir(label);
        fs::write(dir.join(IGNORE_FILE_NAME), pattern).unwrap();
        assert_eq!(read_ignore_file(&dir), expected);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bare_directory_name_expands_to_the_any_depth_pair() {
        assert_ignore_rules("bare-dir", "docs/\n", &["**/docs", "**/docs/**"]);
    }

    #[test]
    fn a_nested_directory_path_is_anchored_to_the_root() {
        assert_ignore_rules(
            "nested-dir",
            "tests/fixtures/\n",
            &["tests/fixtures", "tests/fixtures/**"],
        );
    }

    #[test]
    fn a_bare_glob_matches_at_any_depth() {
        assert_ignore_rules("bare-glob", "*.md\n", &["**/*.md"]);
    }

    #[test]
    fn a_glob_with_a_slash_is_anchored_to_the_root() {
        assert_ignore_rules("anchored-glob", "docs/*.md\n", &["docs/*.md"]);
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

    #[test]
    fn without_the_import_directive_gitignore_is_never_consulted() {
        let dir = temp_dir("gitignore-not-imported");
        fs::write(dir.join(IGNORE_FILE_NAME), "*.md\n").unwrap();
        fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        assert_eq!(read_ignore_file(&dir), vec!["**/*.md".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_import_directive_pulls_in_gitignore_patterns() {
        let dir = temp_dir("gitignore-imported");
        fs::write(
            dir.join(IGNORE_FILE_NAME),
            format!("*.md\n{GITIGNORE_IMPORT_DIRECTIVE}\n"),
        )
        .unwrap();
        fs::write(dir.join(".gitignore"), "*.log\nbuild/\n").unwrap();
        assert_eq!(
            read_ignore_file(&dir),
            vec![
                "**/*.md".to_string(),
                "**/*.log".to_string(),
                "**/build".to_string(),
                "**/build/**".to_string(),
            ]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_gitignore_with_the_directive_present_is_a_silent_no_op() {
        let dir = temp_dir("gitignore-missing");
        fs::write(dir.join(IGNORE_FILE_NAME), GITIGNORE_IMPORT_DIRECTIVE).unwrap();
        assert_eq!(read_ignore_file(&dir), Vec::<String>::new());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gitignore_negation_lines_are_imported_as_re_includes() {
        let dir = temp_dir("gitignore-negation");
        fs::write(dir.join(IGNORE_FILE_NAME), GITIGNORE_IMPORT_DIRECTIVE).unwrap();
        fs::write(dir.join(".gitignore"), "*.log\n!keep.log\n# a comment\n\n").unwrap();
        assert_eq!(
            read_ignore_file(&dir),
            vec!["**/*.log".to_string(), "!**/keep.log".to_string()]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_negation_re_includes_only_what_extra_patterns_excluded() {
        let dir = temp_dir("negation");
        fs::write(
            dir.join(IGNORE_FILE_NAME),
            "*.md\n!docs/readme.md\ndocs/*\n!docs/keep.rs\nlib/\n!lib/a.rs\n!.env\n!\n",
        )
        .unwrap();
        let set = ExcludeSet::new(&read_ignore_file(&dir));
        assert!(set.is_excluded("README.md"));
        // `docs/*` comes after `!docs/readme.md`: the last match wins.
        assert!(set.is_excluded("docs/readme.md"));
        assert!(!set.is_excluded("docs/keep.rs"));
        assert!(set.is_excluded("docs/other.rs"));
        // Inside an excluded directory, as in git: no way back in.
        assert!(set.is_excluded("lib/a.rs"));
        // Built-in exclusions can't be re-included.
        assert!(set.is_excluded(".env"));
        assert!(!set.is_excluded("src/main.rs"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_leading_slash_anchors_to_the_root() {
        let dir = temp_dir("leading-slash");
        fs::write(dir.join(IGNORE_FILE_NAME), "/build/\n/out.rs\n/\n").unwrap();
        assert_eq!(
            read_ignore_file(&dir),
            vec![
                "build".to_string(),
                "build/**".to_string(),
                "out.rs".to_string()
            ]
        );
        let set = ExcludeSet::new(&read_ignore_file(&dir));
        assert!(set.is_excluded("build/a.rs"));
        assert!(set.is_excluded("out.rs"));
        assert!(!set.is_excluded("src/build/b.rs"));
        assert!(!set.is_excluded("src/out.rs"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_negation_after_an_exclusion_keeps_that_one_file() {
        let set = ExcludeSet::new(&["**/*.md".to_string(), "!docs/readme.md".to_string()]);
        assert!(!set.is_excluded("docs/readme.md"));
        assert!(set.is_excluded("docs/other.md"));
    }

    #[test]
    fn next_build_output_is_excluded_by_default() {
        let set = ExcludeSet::default();
        assert!(set.is_excluded(".next"));
        assert!(set.is_excluded("website/.next/static/chunks/app.js"));
        assert!(!set.is_excluded("website/next.config.mjs"));
    }

    /// A website-like layout: the nested `.gitignore` excludes its own
    /// generated output, anchored to its directory.
    fn nested_project(tag: &str, directive: bool) -> std::path::PathBuf {
        let dir = temp_dir(tag);
        let mctignore = if directive {
            format!("{GITIGNORE_IMPORT_DIRECTIVE}\n")
        } else {
            "*.tmp\n".to_string()
        };
        fs::write(dir.join(IGNORE_FILE_NAME), mctignore).unwrap();
        fs::write(dir.join(".gitignore"), "/target\n").unwrap();
        fs::create_dir_all(dir.join("website/out")).unwrap();
        fs::create_dir_all(dir.join("website/src/out")).unwrap();
        fs::write(
            dir.join("website/.gitignore"),
            "/out/\n*.tsbuildinfo\n!keep.tsbuildinfo\n",
        )
        .unwrap();
        // Inside a built-in exclusion: never read.
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("node_modules/pkg/.gitignore"), "*.rs\n").unwrap();
        dir
    }

    #[test]
    fn an_imported_nested_gitignore_applies_below_its_directory_only() {
        let dir = nested_project("nested-imported", true);
        let set = ExcludeSet::for_project(&dir);
        assert!(set.is_excluded("website/out/index.html"));
        assert!(set.is_excluded("website/out"));
        // `/out/` is anchored to website/, as in git.
        assert!(!set.is_excluded("website/src/out/page.ts"));
        assert!(!set.is_excluded("out/main.rs"));
        // An unanchored pattern matches at any depth below website/ only.
        assert!(set.is_excluded("website/a.tsbuildinfo"));
        assert!(set.is_excluded("website/deep/b.tsbuildinfo"));
        assert!(!set.is_excluded("c.tsbuildinfo"));
        // Its negation re-includes within its scope.
        assert!(!set.is_excluded("website/keep.tsbuildinfo"));
        // node_modules/pkg/.gitignore was never imported.
        assert!(!set.is_excluded("src/lib.rs"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_the_directive_a_nested_gitignore_is_ignored() {
        let dir = nested_project("nested-off", false);
        let set = ExcludeSet::for_project(&dir);
        assert!(!set.is_excluded("website/out/index.html"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_new_nested_gitignore_is_found_by_a_discovering_reload_only() {
        let dir = nested_project("nested-new", true);
        let set = ExcludeSet::for_project(&dir);
        fs::create_dir_all(dir.join("docs/build")).unwrap();
        fs::write(dir.join("docs/.gitignore"), "build/\n").unwrap();
        assert!(!set.try_reload().unwrap());
        assert!(!set.is_excluded("docs/build/x.html"));
        assert!(set.try_reload_discovering().unwrap());
        assert!(set.is_excluded("docs/build/x.html"));
        // Known files are then re-read by the cheap reload too.
        fs::write(dir.join("docs/.gitignore"), "").unwrap();
        assert!(set.try_reload().unwrap());
        assert!(!set.is_excluded("docs/build/x.html"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_nested_gitignore_is_a_rules_path_unless_excluded() {
        let set = ExcludeSet::default();
        assert!(set.is_rules_path(".gitignore"));
        assert!(set.is_rules_path(".mctignore"));
        assert!(set.is_rules_path("website/.gitignore"));
        assert!(!set.is_rules_path("node_modules/pkg/.gitignore"));
        assert!(!set.is_rules_path("website/not.gitignore"));
    }
}
