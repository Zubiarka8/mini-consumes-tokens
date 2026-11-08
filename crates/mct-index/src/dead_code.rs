//! Heuristic dead-code candidate detection, shared by `mct-mcp-server`'s
//! `find_dead_code` tool and `mct-cli`'s `dead-code` subcommand so the two
//! never drift apart on what counts as "unreferenced".

use std::collections::{HashMap, HashSet};

use crate::{Index, Result, SymbolListEntry};

/// Symbol kinds a dead-code scan considers as candidates. Deliberately
/// narrower than a full symbol listing: `method` is excluded because
/// trait/interface implementations are routinely called only through dynamic
/// dispatch, never by name — including it would make every implemented
/// interface method a false positive. `module` is excluded because a file's
/// own synthetic module entry is never itself "referenced".
pub const DEAD_CODE_KIND_ALLOWLIST: &[&str] = &[
    "function",
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "type_alias",
];

/// Symbol names a dead-code scan never flags, regardless of reference count —
/// language entry points invoked by the runtime/toolchain itself, never by an
/// in-repo caller.
pub const DEAD_CODE_ENTRY_POINT_NAMES: &[&str] = &["main"];

/// Directory names that mean "everything below here is a test", across the
/// supported languages' conventions.
const TEST_DIRECTORY_SEGMENTS: &[&str] = &["tests", "test", "__tests__"];

/// True when `relative_path` lands in a test directory, or its file name
/// follows one of the cross-language test file conventions
/// (`*_test.*`, `test_*.*`, `*Test.*`, `*.test.*`, `*.spec.*`).
///
/// Stored `relative_path`s always use `/` as the separator, on every OS, so
/// this splits on `/` only — no platform-specific path handling.
fn path_looks_like_test(relative_path: &str) -> bool {
    let mut segments: Vec<&str> = relative_path.split('/').collect();
    // The last segment is the file name; everything before it is a directory.
    let file_name = segments.pop().unwrap_or_default();
    if segments
        .iter()
        .any(|segment| TEST_DIRECTORY_SEGMENTS.contains(&segment.to_lowercase().as_str()))
    {
        return true;
    }
    file_name_looks_like_test(file_name)
}

fn file_name_looks_like_test(file_name: &str) -> bool {
    // `*Test.*` (Java/C#/Kotlin) is the one convention that needs the
    // original casing: lowercasing it would also match `latest.rs`.
    let stem = file_name.split('.').next().unwrap_or_default();
    if stem.len() > 4 && stem.ends_with("Test") {
        return true;
    }
    let lower = file_name.to_lowercase();
    let lower_stem = lower.split('.').next().unwrap_or_default();
    lower.starts_with("test_")
        || lower_stem == "test"
        || lower_stem.ends_with("_test")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

/// Tightened naming convention, kept only as a secondary signal to
/// [`path_looks_like_test`]. The old `starts_with("test")` form also matched
/// `testimonial`, `tester` and `testament`; an exact `test` or a `test_`
/// prefix is the part that's actually a convention.
fn name_looks_like_test(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "test" || lower.starts_with("test_")
}

/// Heuristic for "is this hit a test": no per-language test framework
/// detection here (pytest fixtures, JS `describe`/`it`), so this combines the
/// two signals that are already in the index — the file path, which is by far
/// the stronger one, and the symbol's own name. (A parser can do better on
/// its own: `mct-lang-rust` records a `#[test]`/`#[tokio::test]`/... attribute
/// as a `References` relation to its function, so an inline Rust test is
/// never an unreferenced dead-code candidate even under `src/`. Like every
/// relation it resolves by name project-wide, so it also hides any other
/// same-named symbol — a dead `fn parse` elsewhere next to a `#[test] fn
/// parse` — which is the general name-resolution limit, not a safety check.)
/// Simplification, not a promise: a caller relying on it for exhaustive test
/// coverage should be warned.
pub fn looks_like_test_name(name: &str, relative_path: &str) -> bool {
    path_looks_like_test(relative_path) || name_looks_like_test(name)
}

/// Dead-code candidates under `path` (a single file or directory/crate
/// prefix, matching [`Index::list_symbols`]'s semantics — omit for the whole
/// project), optionally narrowed to one `language`: symbols whose kind is in
/// [`DEAD_CODE_KIND_ALLOWLIST`], that aren't a known entry point, don't look
/// like a test, and have zero indexed references anywhere in the project —
/// a function's calls to itself don't count (see [`Index::referenced_names`]).
///
/// Heuristic over indexed relations, not real export/dynamic-dispatch
/// analysis — a symbol only referenced via reflection, an FFI boundary, or a
/// build feature this project's parsers don't model will still show up here.
pub fn find_dead_code_candidates(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<SymbolListEntry>> {
    let all_entries = index.list_symbols_all(path, language)?;
    let referenced = index.referenced_names()?;

    Ok(all_entries
        .into_iter()
        .filter(|e| is_candidate(e, &referenced))
        .collect())
}

fn is_candidate(entry: &SymbolListEntry, referenced: &HashSet<String>) -> bool {
    DEAD_CODE_KIND_ALLOWLIST.contains(&entry.kind.as_str())
        && !DEAD_CODE_ENTRY_POINT_NAMES.contains(&entry.name.as_str())
        && !looks_like_test_name(&entry.name, &entry.relative_path)
        && !referenced.contains(entry.name.as_str())
}

/// Why a dead-code candidate might still be loaded: a framework convention
/// its source matches, with nothing in the source naming the symbol. A
/// possibility, never a finding that the framework loads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryPointNote {
    pub framework: String,
    pub rule: String,
    pub evidence: String,
}

impl EntryPointNote {
    /// The one wording every consumer shows, so none of them can drift into
    /// claiming more than the index knows.
    pub fn describe(&self) -> String {
        format!(
            "possible entry point ({}, {}): {}; nothing in the source references it and \
             the framework may load it by convention. Not proven.",
            self.framework, self.rule, self.evidence
        )
    }
}

/// Every symbol in scope, and which of them are dead-code candidates.
#[derive(Clone, Debug, Default)]
pub struct DeadCodeScan {
    /// Every indexed symbol under the scope; a scope always covers whole
    /// files, so this holds every symbol of each candidate's file.
    pub symbols: Vec<SymbolListEntry>,
    /// Indices into `symbols`, ascending: the same candidates, in the same
    /// order, as [`find_dead_code_candidates`].
    pub candidates: Vec<usize>,
    /// Possible entry points of the candidates, by index into `symbols`.
    /// Empty unless a parser declared hints whose project requirement holds.
    pub entry_points: HashMap<usize, Vec<EntryPointNote>>,
}

impl DeadCodeScan {
    /// The possible entry points of the candidate at `symbols[symbol]`.
    pub fn possible_entry_points(&self, symbol: usize) -> &[EntryPointNote] {
        self.entry_points.get(&symbol).map_or(&[], Vec::as_slice)
    }
}

/// The first run of digits of a version constraint (`^12.0` -> 12).
fn declared_major(constraint: &str) -> Option<u32> {
    let digits: String = constraint
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Whether the project fact a hint requires holds: the manifest declares the
/// package, and its constraint does not name a major below the lowest checked
/// one. A constraint whose major cannot be read (`*`, `dev-main`) still
/// counts, because the hint is only ever a possibility.
fn requirement_holds(row: &crate::queries::EntryPointRow) -> bool {
    let Some((_, min_major)) = &row.requires else {
        return true;
    };
    if !row.declared {
        return false;
    }
    match row.declared_version.as_deref().and_then(declared_major) {
        Some(major) => major >= *min_major,
        None => true,
    }
}

fn entry_point_notes(
    index: &Index,
    symbols: &[SymbolListEntry],
    candidates: &[usize],
) -> Result<HashMap<usize, Vec<EntryPointNote>>> {
    let rows = crate::queries::entry_point_rows(&index.conn)?;
    if rows.is_empty() {
        return Ok(HashMap::new());
    }
    let candidate_of: HashMap<(&str, &str, &str, Option<&str>, u32), usize> = candidates
        .iter()
        .map(|&id| {
            let s = &symbols[id];
            (
                (
                    s.relative_path.as_str(),
                    s.name.as_str(),
                    s.kind.as_str(),
                    s.parent.as_deref(),
                    s.line,
                ),
                id,
            )
        })
        .collect();
    let mut notes: HashMap<usize, Vec<EntryPointNote>> = HashMap::new();
    for row in rows.iter().filter(|row| requirement_holds(row)) {
        let key = (
            row.relative_path.as_str(),
            row.name.as_str(),
            row.kind.as_str(),
            row.parent.as_deref(),
            row.line,
        );
        if let Some(&id) = candidate_of.get(&key) {
            notes.entry(id).or_default().push(EntryPointNote {
                framework: row.framework.clone(),
                rule: row.rule.clone(),
                evidence: row.evidence.clone(),
            });
        }
    }
    Ok(notes)
}

/// [`find_dead_code_candidates`] plus the symbols review identities need
/// (`reviews::dead_code_findings`), from the same single query.
pub fn scan_dead_code(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
) -> Result<DeadCodeScan> {
    let symbols = index.list_symbols_all(path, language)?;
    let referenced = index.referenced_names()?;
    let candidates: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, entry)| is_candidate(entry, &referenced))
        .map(|(id, _)| id)
        .collect();
    let entry_points = entry_point_notes(index, &symbols, &candidates)?;
    Ok(DeadCodeScan {
        symbols,
        candidates,
        entry_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_major_is_the_first_number_of_the_constraint() {
        assert_eq!(declared_major("^12.0"), Some(12));
        assert_eq!(declared_major(">=10.0 <13"), Some(10));
        assert_eq!(declared_major("~11.3.1"), Some(11));
        assert_eq!(declared_major("*"), None);
        assert_eq!(declared_major("dev-main"), None);
    }

    #[test]
    fn a_name_merely_starting_with_test_in_a_source_file_is_not_a_test() {
        assert!(!looks_like_test_name("testimonial", "src/lib.rs"));
        assert!(!looks_like_test_name("tester", "src/lib.rs"));
        assert!(!looks_like_test_name("testament", "crates/x/src/model.rs"));
        assert!(!looks_like_test_name("latest", "src/latest.rs"));
    }

    #[test]
    fn anything_under_a_test_directory_is_a_test() {
        assert!(looks_like_test_name(
            "build_server",
            "crates/x/tests/foo.rs"
        ));
        assert!(looks_like_test_name("helper", "src/__tests__/render.js"));
        assert!(looks_like_test_name("setUp", "app/test/AppSpec.kt"));
    }

    #[test]
    fn test_file_name_conventions_are_recognised_across_languages() {
        assert!(looks_like_test_name("user_test", "pkg/user_test.go"));
        assert!(looks_like_test_name("compute", "app/CalculatorTest.java"));
        assert!(looks_like_test_name("renders", "src/Button.test.tsx"));
        assert!(looks_like_test_name("renders", "src/button.spec.js"));
        assert!(looks_like_test_name("check", "scripts/test_helpers.py"));
    }

    #[test]
    fn the_name_convention_still_works_as_a_secondary_signal() {
        assert!(looks_like_test_name("test_compute", "src/lib.rs"));
        assert!(looks_like_test_name("TEST_compute", "src/lib.rs"));
        assert!(looks_like_test_name("test", "src/lib.rs"));
    }
}
