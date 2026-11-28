//! Heuristic dead-code candidate detection, shared by `mct-mcp-server`'s
//! `find_dead_code` tool and `mct-cli`'s `dead-code` subcommand so the two
//! never drift apart on what counts as "unreferenced".

use std::collections::HashMap;

use crate::{Index, ReferencedSymbols, Result, SymbolListEntry};

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
pub(crate) fn path_looks_like_test(relative_path: &str) -> bool {
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

/// `join_points.kind` of a declared test (`mct_core::JoinKind::Test`).
pub(crate) const TEST_JOIN_KIND: &str = "test";

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

/// Why a symbol is a test: its `declared` syntax (a test join point) first,
/// then the path convention, then the name convention; `None` when nothing
/// says it is one.
pub fn test_evidence(
    declared: Option<&str>,
    name: &str,
    relative_path: &str,
) -> Option<crate::TestEvidence> {
    match declared {
        Some(syntax) => Some(crate::TestEvidence::Declared(syntax.to_string())),
        None if path_looks_like_test(relative_path) => Some(crate::TestEvidence::Path),
        None if name_looks_like_test(name) => Some(crate::TestEvidence::Name),
        None => None,
    }
}

/// Dead-code candidates under `path` (a single file or directory/crate
/// prefix, matching [`Index::list_symbols`]'s semantics — omit for the whole
/// project), optionally narrowed to one `language`: symbols whose kind is in
/// [`DEAD_CODE_KIND_ALLOWLIST`], that aren't a known entry point, don't look
/// like a test, and have zero indexed references anywhere in the project —
/// a function's calls to itself don't count, and a relation resolved to one
/// symbol counts for that symbol only (see [`Index::referenced_symbols`]).
///
/// Heuristic over indexed relations, not real export/dynamic-dispatch
/// analysis — a symbol only referenced via reflection, an FFI boundary, or a
/// build feature this project's parsers don't model will still show up here.
pub fn find_dead_code_candidates(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<SymbolListEntry>> {
    let scan = scan_dead_code(index, path, language)?;
    Ok(scan
        .candidates
        .into_iter()
        .map(|id| scan.symbols[id].clone())
        .collect())
}

fn is_candidate(entry: &SymbolListEntry, referenced: &ReferencedSymbols) -> bool {
    DEAD_CODE_KIND_ALLOWLIST.contains(&entry.kind.as_str())
        && !DEAD_CODE_ENTRY_POINT_NAMES.contains(&entry.name.as_str())
        && !looks_like_test_name(&entry.name, &entry.relative_path)
        && !referenced.contains(&entry.relative_path, entry.line, &entry.name)
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
    /// Declared entry points in scan order, with the symbol position and reason.
    pub declared_entry_points: Vec<(usize, String)>,
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
    let (ids, symbols): (Vec<_>, Vec<_>) = index
        .list_symbols_with_ids(path, language)?
        .into_iter()
        .unzip();
    let mut stmt = index.conn.prepare_cached(
        "SELECT symbol_id, kind, value FROM join_points WHERE role = 'declares'
         ORDER BY symbol_id, kind, value",
    )?;
    let mut declarations = HashMap::<i64, Vec<String>>::new();
    // A declared test is never a candidate, but it is no framework entry
    // point either: listing every test would bury the declarations.
    let mut tests = std::collections::HashSet::new();
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (id, kind, value) = row?;
        if kind == TEST_JOIN_KIND {
            tests.insert(id);
            continue;
        }
        declarations
            .entry(id)
            .or_default()
            .push(format!("possible entry point: {kind} {value}"));
    }
    let referenced = index.referenced_symbols(DEAD_CODE_KIND_ALLOWLIST)?;
    let mut candidates = Vec::new();
    let mut declared_entry_points = Vec::new();
    for (position, (id, symbol)) in ids.into_iter().zip(&symbols).enumerate() {
        if let Some(reasons) = declarations.remove(&id) {
            declared_entry_points.extend(reasons.into_iter().map(|reason| (position, reason)));
        } else if !tests.contains(&id) && is_candidate(symbol, &referenced) {
            candidates.push(position);
        }
    }
    let entry_points = entry_point_notes(index, &symbols, &candidates)?;
    Ok(DeadCodeScan {
        symbols,
        candidates,
        entry_points,
        declared_entry_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_prefers_the_declaration_then_the_path_then_the_name() {
        use crate::TestEvidence::{Declared, Name, Path};
        assert_eq!(
            test_evidence(Some("#[test]"), "renders", "tests/render.rs"),
            Some(Declared("#[test]".into()))
        );
        assert_eq!(
            test_evidence(None, "renders", "tests/render.rs"),
            Some(Path)
        );
        assert_eq!(
            test_evidence(None, "test_render", "src/render.rs"),
            Some(Name)
        );
        // A name that merely contains `test` is no convention.
        assert_eq!(test_evidence(None, "attest_render", "src/render.rs"), None);
        assert_eq!(test_evidence(None, "latest", "src/latest.rs"), None);
    }

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
