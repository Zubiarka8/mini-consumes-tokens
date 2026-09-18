//! Renders query results as compact plain text rather than JSON — this
//! server's whole reason to exist is cutting the tokens an agent spends
//! reading context, so tool output stays as terse as it can while remaining
//! unambiguous.

use mct_index::{IndexStatus, ReindexReport, RelationHit, SymbolHit, SymbolListEntry};

/// One-line note appended after a count header when a result list got cut
/// down to `shown` of `total` entries, optionally starting at `offset` —
/// empty string when nothing was truncated and `offset` is 0, so callers can
/// unconditionally append it. At `offset` 0 this is byte-identical to the
/// pre-pagination message, so tools that never pass a non-zero offset keep
/// their exact existing output.
fn truncation_note(total: usize, offset: usize, shown: usize) -> String {
    let remaining = total.saturating_sub(offset + shown);
    if offset == 0 && remaining == 0 {
        String::new()
    } else if offset == 0 {
        format!(" (showing {shown}, {remaining} omitted — pass a higher `limit` to see the rest)")
    } else {
        format!(
            " (showing {shown} starting at offset {offset}, {remaining} more available — pass `limit`/`offset` to see the rest)"
        )
    }
}

/// Slices `items[offset..offset+limit]`, clamped to bounds — the shared
/// pagination behind every paginated tool's output.
fn paginate<T>(items: &[T], offset: usize, limit: usize) -> &[T] {
    let total = items.len();
    let start = offset.min(total);
    let end = total.min(start.saturating_add(limit));
    &items[start..end]
}

pub fn symbol_hits(name: &str, hits: &[SymbolHit], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbol named `{name}` found in the index.");
    }
    let total = hits.len();
    let shown = paginate(hits, 0, limit);
    let mut out = format!(
        "{total} definition(s) of `{name}`{}:\n",
        truncation_note(total, 0, shown.len())
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
/// mirrors the declaration order of `mct_core::SymbolKind` so output is
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
    let shown = paginate(hits, 0, limit);
    let mut out = format!(
        "{total} symbol(s) under `{path}`{}:\n",
        truncation_note(total, 0, shown.len())
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

/// Languages whose function/type bodies are brace-delimited, so the first
/// unmatched `{` after a declaration reliably marks where the body starts —
/// these get precise body elision in [`file_skeleton`]. Every other
/// language (indentation- or keyword-delimited: Python, Lua, Bash,
/// PowerShell, or anything not listed here) falls back to a declaration-line
/// -only rendering rather than guessing where a body ends.
const BRACE_LANGUAGES: &[&str] = &[
    "rust",
    "go",
    "java",
    "cpp",
    "csharp",
    "javascript_typescript",
    "php",
    "kotlin",
];

/// Placeholder comment for the non-brace fallback, in each language's own
/// comment syntax where practical.
fn non_brace_placeholder(language: &str) -> &'static str {
    match language {
        "lua" => "    -- ...",
        _ => "    # ...",
    }
}

/// Renders a single top-level symbol's collapsed skeleton: its declaration
/// through the opening `{`, then a `// ...` placeholder and closing `}`.
/// The brace search is bounded by the symbol's own extent (`end_line` when
/// known, otherwise just its declaration line) so it can never wander into
/// unrelated code that follows — e.g. an interface method with no body
/// immediately followed by a class that does have one. If no brace turns up
/// within that bound (a body-less declaration, or a language misclassified
/// as brace-delimited), the declaration line is shown as-is rather than
/// guessing.
fn brace_skeleton(entry: &SymbolListEntry, lines: &[&str]) -> String {
    let start_idx = entry.line.saturating_sub(1) as usize;
    let Some(&first_line) = lines.get(start_idx) else {
        return format!("{} {}\n", entry.kind, entry.name);
    };
    let search_end = entry
        .end_line
        .map(|end| end.saturating_sub(1) as usize)
        .unwrap_or(start_idx)
        .min(lines.len().saturating_sub(1))
        .max(start_idx);

    for (offset, &line) in lines[start_idx..=search_end].iter().enumerate() {
        if let Some(col) = line.find('{') {
            let brace_idx = start_idx + offset;
            let mut signature = lines[start_idx..brace_idx].join("\n");
            if !signature.is_empty() {
                signature.push('\n');
            }
            signature.push_str(&line[..=col]);
            return format!("{signature}\n    // ...\n}}\n");
        }
    }
    format!("{first_line}\n")
}

/// Non-brace-language fallback: just the declaration line plus a
/// language-appropriate placeholder comment — precise body-end detection
/// for indentation/keyword-delimited syntax needs real parsing, which this
/// text-slicing tool deliberately doesn't attempt.
fn declaration_only_skeleton(entry: &SymbolListEntry, lines: &[&str]) -> String {
    let start_idx = entry.line.saturating_sub(1) as usize;
    let placeholder = non_brace_placeholder(&entry.language);
    match lines.get(start_idx) {
        Some(line) => format!("{line}\n{placeholder}\n"),
        None => format!("{} {}\n{placeholder}\n", entry.kind, entry.name),
    }
}

fn skeleton_block(entry: &SymbolListEntry, lines: &[&str]) -> String {
    if BRACE_LANGUAGES.contains(&entry.language.as_str()) {
        brace_skeleton(entry, lines)
    } else {
        declaration_only_skeleton(entry, lines)
    }
}

/// Renders a file's top-level symbols (`entries` — callers pass only
/// `parent.is_none()` ones) with bodies collapsed, from `source`'s own
/// text. Nested members are not shown individually: a class/struct/interface
/// collapses to one block regardless of what's inside it — this is a
/// module-shape overview, not a full outline.
pub fn file_skeleton(path: &str, entries: &[SymbolListEntry], source: &str) -> String {
    if entries.is_empty() {
        return format!("No top-level symbols found in `{path}` to build a skeleton from.");
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut out = format!(
        "Skeleton of `{path}` ({} top-level symbol(s), bodies collapsed):\n\n",
        entries.len()
    );
    for entry in entries {
        out.push_str(&skeleton_block(entry, &lines));
        out.push('\n');
    }
    out
}

/// Appended to a relation-hit line when it's more than one hop from the
/// queried symbol (found via multi-hop `depth`); omitted for direct
/// (depth 1) hits, so a caller that never passes `depth > 1` sees output
/// byte-identical to before `depth` existed.
fn depth_tag(depth: u32) -> String {
    if depth > 1 {
        format!(" [depth {depth}]")
    } else {
        String::new()
    }
}

pub fn relation_hits(
    subject: &str,
    verb_label: &str,
    hits: &[RelationHit],
    offset: usize,
    limit: usize,
) -> String {
    if hits.is_empty() {
        return format!("No {verb_label} found for `{subject}`.");
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let mut out = format!(
        "{total} {}{}:\n",
        verb_label,
        truncation_note(total, offset, shown.len())
    );
    for hit in shown {
        out.push_str(&format!(
            "{}:{}:{} [{}] {} --{}--> {}{}\n",
            hit.relative_path,
            hit.line,
            hit.column,
            hit.language,
            hit.from_symbol,
            hit.kind,
            hit.to_name,
            depth_tag(hit.depth)
        ));
    }
    out
}

pub fn impact_analysis(
    symbol: &str,
    callers: &[RelationHit],
    references: &[RelationHit],
    affected_tests: &[&RelationHit],
    offset: usize,
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

    // Each section below is paginated independently against the same
    // `offset`/`limit` — a symbol with hundreds of callers but few tests
    // shouldn't have its test list truncated just because the caller list
    // is huge.
    if !affected_tests.is_empty() {
        let shown = paginate(affected_tests, offset, limit);
        out.push_str(&format!(
            "\nLikely affected tests{}:\n",
            truncation_note(affected_tests.len(), offset, shown.len())
        ));
        for hit in shown {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}{}\n",
                hit.relative_path,
                hit.line,
                hit.column,
                hit.language,
                hit.from_symbol,
                depth_tag(hit.depth)
            ));
        }
    }

    if !callers.is_empty() {
        let shown = paginate(callers, offset, limit);
        out.push_str(&format!(
            "\nDirect callers{}:\n",
            truncation_note(callers.len(), offset, shown.len())
        ));
        for hit in shown {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {}{}\n",
                hit.relative_path,
                hit.line,
                hit.column,
                hit.language,
                hit.from_symbol,
                depth_tag(hit.depth)
            ));
        }
    }

    if !references.is_empty() {
        let shown = paginate(references, offset, limit);
        out.push_str(&format!(
            "\nAll references{}:\n",
            truncation_note(references.len(), offset, shown.len())
        ));
        for hit in shown {
            out.push_str(&format!(
                "  {}:{}:{} [{}] {} --{}--> {}{}\n",
                hit.relative_path,
                hit.line,
                hit.column,
                hit.language,
                hit.from_symbol,
                hit.kind,
                hit.to_name,
                depth_tag(hit.depth)
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

/// Renders `get_project_overview`'s digest: one indented block per module
/// (file), its surfaced top-level symbols with a `(+N more)` suffix when
/// truncated, and — when the caller passed `relations` — each surfaced
/// symbol's top callers as sub-lines. Deliberately not JSON: a smaller,
/// human/agent-skimmable token footprint is the entire point of this tool.
pub fn overview(
    root_path: &str,
    modules: &[crate::server::ModuleDigest],
    max_symbols_per_module: u32,
) -> String {
    if modules.is_empty() {
        return format!("No symbols found under `{root_path}`.");
    }
    let mut out = format!(
        "Project overview of `{root_path}` ({} module(s), up to {max_symbols_per_module} symbol(s) each):\n",
        modules.len()
    );
    for module in modules {
        out.push_str(&format!("\n{}:\n", module.relative_path));
        if module.symbols.is_empty() {
            out.push_str("  (no top-level symbols)\n");
            continue;
        }
        for symbol in &module.symbols {
            let range = line_range(symbol.line, symbol.end_line);
            out.push_str(&format!("  [{}] {} {range}\n", symbol.kind, symbol.name));
            let Some((_, callers)) = module.relations.iter().find(|(name, _)| name == &symbol.name)
            else {
                continue;
            };
            for caller in callers {
                out.push_str(&format!(
                    "      <- {} ({}:{})\n",
                    caller.from_symbol, caller.relative_path, caller.line
                ));
            }
        }
        if module.omitted > 0 {
            out.push_str(&format!("  (+{} more)\n", module.omitted));
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

#[cfg(test)]
mod file_skeleton_tests {
    use super::*;

    fn entry(name: &str, kind: &str, language: &str, line: u32, end_line: Option<u32>) -> SymbolListEntry {
        SymbolListEntry {
            name: name.to_string(),
            kind: kind.to_string(),
            language: language.to_string(),
            relative_path: "f".to_string(),
            line,
            end_line,
            parent: None,
        }
    }

    #[test]
    fn brace_language_collapses_the_body_between_the_opening_and_closing_brace() {
        let source = "pub fn compute() -> i32 {\n    helper()\n}\n";
        let entries = [entry("compute", "function", "rust", 1, Some(3))];
        let out = file_skeleton("f.rs", &entries, source);
        assert!(out.contains("pub fn compute() -> i32 {"), "got: {out}");
        assert!(out.contains("    // ...\n}"), "got: {out}");
        assert!(!out.contains("helper()"), "body must be collapsed: {out}");
    }

    #[test]
    fn missing_end_line_still_finds_a_brace_on_the_declaration_line_itself() {
        // No `end_line` (e.g. a parser that hasn't populated it, or a file
        // not yet reindexed since that column landed): the search must
        // still work when the brace is on the symbol's own first line —
        // it's just bounded to that line instead of the symbol's full extent.
        let source = "fn helper() -> i32 {\n    1\n}\n";
        let entries = [entry("helper", "function", "rust", 1, None)];
        let out = file_skeleton("f.rs", &entries, source);
        assert!(out.contains("fn helper() -> i32 {"), "got: {out}");
        assert!(out.contains("    // ...\n}"), "got: {out}");
    }

    #[test]
    fn missing_end_line_and_no_brace_on_the_declaration_line_falls_back_to_that_line_as_is() {
        // A multi-line signature with no `end_line` to bound the search: we
        // deliberately don't scan forward past the declaration line (that
        // could consume a brace belonging to unrelated later code), so this
        // degrades to showing just the first line rather than guessing.
        let source = "fn long_signature(\n    x: i32,\n) -> i32 {\n    x\n}\n";
        let entries = [entry("long_signature", "function", "rust", 1, None)];
        let out = file_skeleton("f.rs", &entries, source);
        assert!(out.contains("fn long_signature("), "got: {out}");
        assert!(!out.contains("// ..."), "no brace within bound — must not fabricate a body: {out}");
    }

    #[test]
    fn brace_search_never_wanders_past_the_symbols_own_end_line() {
        // A body-less declaration (e.g. an interface method) immediately
        // followed by a class that does have a body — the search for
        // symbol A's brace must not consume symbol B's brace.
        let source = "void noBody();\nclass Next {\n    int x;\n}\n";
        let entries = [entry("noBody", "method", "java", 1, Some(1))];
        let out = file_skeleton("f.java", &entries, source);
        assert!(out.contains("void noBody();"), "got: {out}");
        assert!(!out.contains("class Next"), "must not consume a later symbol's brace: {out}");
    }

    #[test]
    fn non_brace_language_only_shows_the_declaration_line() {
        let source = "def test_compute():\n    assert compute() == 1\n";
        let entries = [entry("test_compute", "function", "python", 1, Some(2))];
        let out = file_skeleton("f.py", &entries, source);
        assert!(out.contains("def test_compute():"), "got: {out}");
        assert!(out.contains("# ..."), "got: {out}");
        assert!(!out.contains("assert compute"), "body must not leak through: {out}");
    }

    #[test]
    fn module_kind_entries_are_the_callers_responsibility_to_exclude() {
        // file_skeleton itself renders whatever it's given — filtering out
        // the synthetic whole-file `module` entry is server.rs's job (see
        // get_file_skeleton), not this function's. Documented here so a
        // future change to that filter doesn't silently regress: passing a
        // module entry through still produces *something* sane, not a panic.
        let source = "pub fn compute() -> i32 {\n    1\n}\n";
        let entries = [entry("f", "module", "rust", 1, Some(3))];
        let out = file_skeleton("f.rs", &entries, source);
        assert!(out.contains("pub fn compute"), "got: {out}");
    }

    #[test]
    fn empty_entries_says_so_instead_of_an_empty_body() {
        let out = file_skeleton("f.rs", &[], "anything");
        assert!(out.contains("No top-level symbols"), "got: {out}");
    }
}
