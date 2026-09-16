//! Renders query results as compact plain text rather than JSON — this
//! server's whole reason to exist is cutting the tokens an agent spends
//! reading context, so tool output stays as terse as it can while remaining
//! unambiguous.

use ccm_index::{IndexStatus, ReindexReport, RelationHit, SymbolHit, SymbolListEntry};

/// One-line note appended after a count header when a result list got cut
/// down to `shown` of `total` entries — empty string when nothing was
/// truncated, so callers can unconditionally append it.
fn truncation_note(total: usize, shown: usize) -> String {
    if shown < total {
        format!(
            " (showing {shown}, {} omitted — pass a higher `limit` to see the rest)",
            total - shown
        )
    } else {
        String::new()
    }
}

pub fn symbol_hits(name: &str, hits: &[SymbolHit], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbol named `{name}` found in the index.");
    }
    let total = hits.len();
    let shown = &hits[..total.min(limit)];
    let mut out = format!(
        "{total} definition(s) of `{name}`{}:\n",
        truncation_note(total, shown.len())
    );
    for hit in shown {
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

/// Kind strings in the order they're grouped/displayed by [`list_symbols`],
/// paired with the plural heading printed above each non-empty group —
/// mirrors the declaration order of `ccm_core::SymbolKind` so output is
/// stable regardless of SQL row order within a kind.
const KIND_HEADINGS: &[(&str, &str)] = &[
    ("function", "Functions"),
    ("method", "Methods"),
    ("class", "Classes"),
    ("struct", "Structs"),
    ("interface", "Interfaces"),
    ("enum", "Enums"),
    ("trait", "Traits"),
    ("type_alias", "Type Aliases"),
    ("module", "Modules"),
    ("variable", "Variables"),
    ("constant", "Constants"),
    ("field", "Fields"),
    ("element", "Elements"),
    ("rule", "Rules"),
];

fn kind_heading(kind: &str) -> String {
    KIND_HEADINGS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, heading)| heading.to_string())
        .unwrap_or_else(|| format!("{kind}s"))
}

fn line_range(line: u32, end_line: Option<u32>) -> String {
    match end_line {
        Some(end) if end > line => format!("L{line}-L{end}"),
        _ => format!("L{line}"),
    }
}

/// Renders `list_symbols`' result grouped by kind, in [`KIND_HEADINGS`]
/// order. When `path` names a single file, entries omit the (redundant)
/// file path per line; a directory/crate listing spans multiple files, so
/// each line carries its own `relative_path` to disambiguate.
pub fn list_symbols(path: &str, is_file: bool, hits: &[SymbolListEntry], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbols found under `{path}`.");
    }
    let total = hits.len();
    let shown = &hits[..total.min(limit)];
    let mut out = format!(
        "{total} symbol(s) under `{path}`{}:\n",
        truncation_note(total, shown.len())
    );
    for &(kind, _) in KIND_HEADINGS {
        let group: Vec<&SymbolListEntry> = shown.iter().filter(|h| h.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("{}:\n", kind_heading(kind)));
        let name_width = group.iter().map(|h| h.name.chars().count()).max().unwrap_or(0);
        for hit in &group {
            let range = line_range(hit.line, hit.end_line);
            if is_file {
                out.push_str(&format!("  {:<name_width$}  {range}\n", hit.name));
            } else {
                out.push_str(&format!(
                    "  {:<name_width$}  {}  {range}\n",
                    hit.name, hit.relative_path
                ));
            }
        }
    }
    out
}

pub fn relation_hits(subject: &str, verb_label: &str, hits: &[RelationHit], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No {verb_label} found for `{subject}`.");
    }
    let total = hits.len();
    let shown = &hits[..total.min(limit)];
    let mut out = format!(
        "{total} {}{}:\n",
        verb_label,
        truncation_note(total, shown.len())
    );
    for hit in shown {
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
    limit: usize,
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

    // Each section below is truncated independently against the same
    // `limit` — a symbol with hundreds of callers but few tests shouldn't
    // have its test list truncated just because the caller list is huge.
    if !affected_tests.is_empty() {
        let shown = &affected_tests[..affected_tests.len().min(limit)];
        out.push_str(&format!(
            "\nLikely affected tests{}:\n",
            truncation_note(affected_tests.len(), shown.len())
        ));
        for hit in shown {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}\n",
                hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol
            ));
        }
    }

    if !callers.is_empty() {
        let shown = &callers[..callers.len().min(limit)];
        out.push_str(&format!(
            "\nDirect callers{}:\n",
            truncation_note(callers.len(), shown.len())
        ));
        for hit in shown {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}\n",
                hit.relative_path, hit.line, hit.column, hit.language, hit.from_symbol
            ));
        }
    }

    if !references.is_empty() {
        let shown = &references[..references.len().min(limit)];
        out.push_str(&format!(
            "\nAll references{}:\n",
            truncation_note(references.len(), shown.len())
        ));
        for hit in shown {
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
    if !status.dependencies.is_empty() {
        out.push_str("Dependencies detected:\n");
        for manifest in &status.dependencies {
            out.push_str(&format!(
                "  {} ({}, {} dep(s)):\n",
                manifest.manifest_path,
                manifest.language,
                manifest.dependencies.len()
            ));
            for dep in &manifest.dependencies {
                match &dep.version {
                    Some(version) => out.push_str(&format!("    {} {}\n", dep.name, version)),
                    None => out.push_str(&format!("    {}\n", dep.name)),
                }
            }
        }
    }
    out
}
