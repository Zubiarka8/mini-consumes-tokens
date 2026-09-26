//! Heuristic dead-code candidate detection, shared by `mct-mcp-server`'s
//! `find_dead_code` tool and `mct-cli`'s `dead-code` subcommand so the two
//! never drift apart on what counts as "unreferenced".

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

/// Heuristic for "is this hit a test": still no per-language test
/// framework/attribute detection (Rust's `#[test]`, pytest fixtures, JS
/// `describe`/`it`), so this combines the two signals that are already in
/// the index — the file path, which is by far the stronger one, and the
/// symbol's own name. Simplification, not a promise: a caller relying on it
/// for exhaustive test coverage should be warned.
pub fn looks_like_test_name(name: &str, relative_path: &str) -> bool {
    path_looks_like_test(relative_path) || name_looks_like_test(name)
}

/// Dead-code candidates under `path` (a single file or directory/crate
/// prefix, matching [`Index::list_symbols`]'s semantics — omit for the whole
/// project), optionally narrowed to one `language`: symbols whose kind is in
/// [`DEAD_CODE_KIND_ALLOWLIST`], that aren't a known entry point, don't look
/// like a test, and have zero indexed references anywhere in the project.
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
    let reference_counts = index.reference_counts()?;

    Ok(all_entries
        .into_iter()
        .filter(|e| DEAD_CODE_KIND_ALLOWLIST.contains(&e.kind.as_str()))
        .filter(|e| !DEAD_CODE_ENTRY_POINT_NAMES.contains(&e.name.as_str()))
        .filter(|e| !looks_like_test_name(&e.name, &e.relative_path))
        .filter(|e| !reference_counts.contains_key(e.name.as_str()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
