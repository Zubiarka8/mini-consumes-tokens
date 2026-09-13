//! Renders query results as compact plain text rather than JSON — this
//! server's whole reason to exist is cutting the tokens an agent spends
//! reading context, so tool output stays as terse as it can while remaining
//! unambiguous.

use ccm_index::{IndexStatus, ReindexReport, RelationHit, SymbolHit};

pub fn symbol_hits(name: &str, hits: &[SymbolHit]) -> String {
    if hits.is_empty() {
        return format!("No symbol named `{name}` found in the index.");
    }
    let mut out = format!("{} definition(s) of `{name}`:\n", hits.len());
    for hit in hits {
        let parent = hit
            .parent
            .as_deref()
            .map(|p| format!(" (in {p})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}:{}:{} [{}] {} {}{}\n",
            hit.relative_path, hit.line, hit.column, hit.language, hit.kind, hit.name, parent
        ));
    }
    out
}

pub fn relation_hits(subject: &str, verb_label: &str, hits: &[RelationHit]) -> String {
    if hits.is_empty() {
        return format!("No {verb_label} found for `{subject}`.");
    }
    let mut out = format!("{} {}:\n", hits.len(), verb_label);
    for hit in hits {
        out.push_str(&format!(
            "{}:{}:{} [{}] {} --{}--> {}\n",
            hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol, hit.kind, hit.to_name
        ));
    }
    out
}

pub fn impact_analysis(
    symbol: &str,
    callers: &[RelationHit],
    references: &[RelationHit],
    affected_tests: &[&RelationHit],
) -> String {
    let mut out = format!("Impact analysis for `{symbol}`:\n");
    out.push_str(&format!("  {} direct caller(s)\n", callers.len()));
    out.push_str(&format!(
        "  {} reference(s) total (calls/imports/extends/implements/plain)\n",
        references.len()
    ));
    out.push_str(&format!(
        "  {} likely affected test(s)\n",
        affected_tests.len()
    ));

    if !affected_tests.is_empty() {
        out.push_str("\nLikely affected tests:\n");
        for hit in affected_tests {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}\n",
                hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol
            ));
        }
    }

    if !callers.is_empty() {
        out.push_str("\nDirect callers:\n");
        for hit in callers {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}\n",
                hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol
            ));
        }
    }

    if !references.is_empty() {
        out.push_str("\nAll references:\n");
        for hit in references {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {} --{}--> {}\n",
                hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol, hit.kind, hit.to_name
            ));
        }
    }

    if callers.is_empty() && references.is_empty() {
        out.push_str("\nNothing else in the index references this symbol.\n");
    }
    out
}

/// Naming-convention heuristic for "is this a test": no per-language test
/// framework/attribute detection yet (e.g. Rust's `#[test]`, pytest fixtures),
/// so this only catches the `test`/`test_`-prefixed-name convention common to
/// both currently-supported languages. Simplification, not a promise — a
/// caller relying on it for exhaustive test coverage should be warned.
pub fn looks_like_test_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("test_") || lower.starts_with("test")
}

pub fn reindex_report(report: &ReindexReport) -> String {
    let mut out = format!(
        "Reindex complete: {} parsed, {} unchanged, {} removed, {} symbols written.\n",
        report.files_parsed, report.files_unchanged, report.files_removed, report.symbols_written
    );
    if !report.issues.is_empty() {
        out.push_str(&format!("{} issue(s):\n", report.issues.len()));
        for issue in &report.issues {
            out.push_str(&format!(
                "  {} [{:?}]: {}\n",
                issue.relative_path, issue.kind, issue.detail
            ));
        }
    }
    out
}

pub fn index_status(status: &IndexStatus) -> String {
    let mut out = format!(
        "{} files indexed, {} symbols total.\n",
        status.total_files, status.total_symbols
    );
    match status.last_indexed_at {
        Some(ts) => out.push_str(&format!("Last indexed at unix timestamp {ts}.\n")),
        None => out.push_str("Never indexed yet.\n"),
    }
    if status.languages.is_empty() {
        out.push_str("No languages indexed yet.\n");
    } else {
        out.push_str("Coverage by language:\n");
        for lang in &status.languages {
            out.push_str(&format!(
                "  {}: {} files, {} symbols\n",
                lang.language, lang.file_count, lang.symbol_count
            ));
        }
    }
    if !status.unsupported_languages.is_empty() {
        out.push_str(&format!(
            "Languages seen but not yet supported: {}\n",
            status.unsupported_languages.join(", ")
        ));
    }
    if !status.syntax_errors.is_empty() {
        out.push_str(&format!("{} file(s) failed to parse:\n", status.syntax_errors.len()));
        for err in &status.syntax_errors {
            out.push_str(&format!("  {}: {}\n", err.relative_path, err.detail));
        }
    }
    out
}
