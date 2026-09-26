//! Renders query results as compact plain text rather than JSON — this
//! server's whole reason to exist is cutting the tokens an agent spends
//! reading context, so tool output stays as terse as it can while remaining
//! unambiguous.

use std::collections::BTreeSet;

use mct_index::{
    FileTreeNode, IndexStatus, LiteralHit, ReindexReport, RelationHit, SymbolHit, SymbolListEntry,
};

use crate::toon::encode_table;

/// Maximum bytes any single tool response will render before truncating.
/// `limit` counts rows; this counts the context the caller actually pays
/// for, which is the thing this server exists to protect.
pub const DEFAULT_BYTE_BUDGET: usize = 24_000;

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

/// Note for a list the byte budget cut short. It takes precedence over
/// [`truncation_note`]: when the budget is the binding constraint, raising
/// `limit` does nothing, so the caller has to be pointed at narrowing or
/// paging instead.
fn budget_note(total: usize, shown: usize, budget: usize) -> String {
    format!(
        " (showing {shown} of {total}, truncated at the {budget}-byte response budget — narrow with `path`/`language` or pass `offset` to page)"
    )
}

/// Picks the right header note for a rendered list: the budget wording when
/// the budget dropped entries, otherwise the pre-existing row-limit wording
/// (byte-identical to before this budget existed).
fn list_note(total: usize, offset: usize, shown: usize, body: &BudgetedList) -> String {
    if body.dropped > 0 {
        budget_note(total, body.kept, body.budget)
    } else {
        truncation_note(total, offset, shown)
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

/// Accumulates already-rendered entries up to a byte budget, applied after
/// [`paginate`] has cut by row count.
///
/// Entries are appended whole, so the body can only ever end at an entry
/// boundary — splitting a line, or a multi-byte UTF-8 codepoint, is
/// structurally impossible here rather than something a bounds check has to
/// get right. Once one entry is rejected every later one is counted as
/// dropped, so what survives is always a prefix of the input order.
struct BudgetedList {
    body: String,
    budget: usize,
    kept: usize,
    dropped: usize,
}

impl BudgetedList {
    fn new(budget: usize) -> Self {
        Self {
            body: String::new(),
            budget,
            kept: 0,
            dropped: 0,
        }
    }

    /// Appends `entry`, preceded by `prefix` (a group heading) when one is
    /// given — the heading is written only if the entry itself fits, so a
    /// heading is never stranded above nothing. The first entry is always
    /// accepted, so even a budget smaller than one row returns something
    /// useful instead of an empty body. Returns whether the entry was kept.
    fn push_with_prefix(&mut self, prefix: Option<&str>, entry: &str) -> bool {
        let prefix_len = prefix.map_or(0, str::len);
        let fits = self.body.len() + prefix_len + entry.len() <= self.budget;
        let accept = self.dropped == 0 && (fits || self.kept == 0);
        if !accept {
            self.dropped += 1;
            return false;
        }
        if let Some(prefix) = prefix {
            self.body.push_str(prefix);
        }
        self.body.push_str(entry);
        self.kept += 1;
        true
    }

    fn push(&mut self, entry: &str) -> bool {
        self.push_with_prefix(None, entry)
    }
}

/// The writer-declared depth annotation appended after a symbol's name when
/// its `level` is populated (currently only Markdown ATX headings) —
/// `" H2"`, so an H1 and an H6 read differently even though both share
/// `kind == "element"`. Empty for every symbol with no level.
fn level_suffix(level: Option<u32>) -> String {
    level.map(|l| format!(" H{l}")).unwrap_or_default()
}

pub fn symbol_hits(name: &str, hits: &[SymbolHit], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbol named `{name}` found in the index.");
    }
    let total = hits.len();
    let shown = paginate(hits, 0, limit);
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    for hit in shown {
        let parent = hit
            .parent
            .as_deref()
            .map(|p| format!(" (in {p})"))
            .unwrap_or_default();
        body.push(&format!(
            "{}:{}:{} [{}] {} {}{}{}\n",
            hit.relative_path,
            hit.line,
            hit.column,
            hit.language,
            hit.kind,
            hit.name,
            level_suffix(hit.level),
            parent
        ));
    }
    format!(
        "{total} definition(s) of `{name}`{}:\n{}",
        list_note(total, 0, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`symbol_hits`]: same `limit` semantics (row-count
/// pagination only — see the module doc on why the byte budget isn't
/// re-applied here), one row per definition instead of one labelled line.
pub fn symbol_hits_toon(name: &str, hits: &[SymbolHit], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbol named `{name}` found in the index.");
    }
    let total = hits.len();
    let shown = paginate(hits, 0, limit);
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|hit| {
            vec![
                hit.relative_path.clone(),
                hit.line.to_string(),
                hit.column.to_string(),
                hit.language.clone(),
                hit.kind.clone(),
                hit.name.clone(),
                hit.parent.clone().unwrap_or_default(),
                hit.level.map(|l| l.to_string()).unwrap_or_default(),
            ]
        })
        .collect();
    format!(
        "{total} definition(s) of `{name}`{}:\n{}",
        truncation_note(total, 0, shown.len()),
        encode_table(
            "symbols",
            &["path", "line", "column", "language", "kind", "name", "parent", "level"],
            &rows,
        )
    )
}

/// Up to `max_lines` lines of `source` starting at the 1-based `line`, never
/// past `end_line` when the parser recorded one, each prefixed with its line
/// number. Out-of-range lines (a file edited since the last reindex) are
/// simply dropped, never a panic.
pub fn symbol_snippet(source: &str, line: u32, end_line: Option<u32>, max_lines: usize) -> String {
    let first = line.max(1) as usize;
    let last_by_cap = first.saturating_add(max_lines.saturating_sub(1));
    let last = end_line.map_or(last_by_cap, |end| {
        last_by_cap.min((end as usize).max(first))
    });
    source
        .lines()
        .enumerate()
        .skip(first - 1)
        .take_while(|(i, _)| *i < last)
        .map(|(i, text)| format!("{}| {text}\n", i + 1))
        .collect()
}

fn search_hit_line(hit: &SymbolHit) -> String {
    let parent = hit
        .parent
        .as_deref()
        .map(|p| format!(" (in {p})"))
        .unwrap_or_default();
    format!(
        "{}:{} [{}] {} {}{}{}\n",
        hit.relative_path,
        line_range(hit.line, hit.end_line),
        hit.language,
        hit.kind,
        hit.name,
        level_suffix(hit.level),
        parent
    )
}

/// Renders `search_symbols`' ranked hits, best first, `offset`/`limit`
/// paginated and byte-budgeted like every other list. `snippets` is aligned
/// with the shown page (`hits[offset..][..limit]`); each present snippet is
/// indented under its hit and kept or dropped together with it.
pub fn search_hits(
    query: &str,
    hits: &[SymbolHit],
    offset: usize,
    limit: usize,
    snippets: &[Option<String>],
) -> String {
    if hits.is_empty() {
        return format!("No symbol matches `{query}` in the index.");
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    for (i, hit) in shown.iter().enumerate() {
        let mut entry = search_hit_line(hit);
        if let Some(Some(snippet)) = snippets.get(i) {
            for line in snippet.lines() {
                entry.push_str("    ");
                entry.push_str(line);
                entry.push('\n');
            }
        }
        body.push(&entry);
    }
    format!(
        "{total} match(es) for `{query}`, best first{}:\n{}",
        list_note(total, offset, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`search_hits`]: one row per hit in rank order, with
/// the snippet (empty when not requested) as its last column.
pub fn search_hits_toon(
    query: &str,
    hits: &[SymbolHit],
    offset: usize,
    limit: usize,
    snippets: &[Option<String>],
) -> String {
    if hits.is_empty() {
        return format!("No symbol matches `{query}` in the index.");
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let rows: Vec<Vec<String>> = shown
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            vec![
                hit.relative_path.clone(),
                hit.line.to_string(),
                hit.end_line.map(|l| l.to_string()).unwrap_or_default(),
                hit.language.clone(),
                hit.kind.clone(),
                hit.name.clone(),
                hit.parent.clone().unwrap_or_default(),
                snippets.get(i).cloned().flatten().unwrap_or_default(),
            ]
        })
        .collect();
    format!(
        "{total} match(es) for `{query}`, best first{}:\n{}",
        truncation_note(total, offset, shown.len()),
        encode_table(
            "matches",
            &["path", "line", "end_line", "language", "kind", "name", "parent", "snippet"],
            &rows,
        )
    )
}

/// `hybrid_search`'s exact-phrase section for string literals holding the
/// phrase, one `path:line in kind name "text"` line each (no `in ...` for a
/// literal outside every symbol), `offset`/`limit` paginated and
/// byte-budgeted like [`search_hits`]. Empty when there are none, so a
/// phrase with no literal hit renders exactly as before literals existed.
pub fn literal_hits(phrase: &str, hits: &[LiteralHit], offset: usize, limit: usize) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    for hit in shown {
        let symbol = hit
            .symbol
            .as_ref()
            .map(|(name, kind)| format!(" in {kind} {name}"))
            .unwrap_or_default();
        body.push(&format!(
            "{}:{}{symbol} \"{}\"\n",
            hit.relative_path, hit.line, hit.text
        ));
    }
    format!(
        "{total} string literal(s) holding \"{phrase}\"{}:\n{}",
        list_note(total, offset, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`literal_hits`]. Empty when there are none.
pub fn literal_hits_toon(phrase: &str, hits: &[LiteralHit], offset: usize, limit: usize) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|hit| {
            let (name, kind) = hit.symbol.clone().unwrap_or_default();
            vec![
                hit.relative_path.clone(),
                hit.line.to_string(),
                hit.language.clone(),
                kind,
                name,
                hit.text.clone(),
            ]
        })
        .collect();
    format!(
        "{total} string literal(s) holding \"{phrase}\"{}:\n{}",
        truncation_note(total, offset, shown.len()),
        encode_table(
            "literals",
            &["path", "line", "language", "kind", "symbol", "text"],
            &rows,
        )
    )
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
///
/// Columns are separated by a fixed two spaces rather than padded to the
/// widest name: alignment whitespace is tokens the caller pays for and
/// carries no information.
pub fn list_symbols(path: &str, is_file: bool, hits: &[SymbolListEntry], limit: usize) -> String {
    if hits.is_empty() {
        return format!("No symbols found under `{path}`.");
    }
    let total = hits.len();
    let shown = paginate(hits, 0, limit);
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    for &(kind, _) in KIND_HEADINGS {
        let group: Vec<&SymbolListEntry> = shown.iter().filter(|h| h.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        let heading = format!("{}:\n", kind_heading(kind));
        let mut heading_pending = true;
        for hit in &group {
            let range = line_range(hit.line, hit.end_line);
            let level = level_suffix(hit.level);
            let entry = if is_file {
                format!("  {}{level}  {range}\n", hit.name)
            } else {
                format!("  {}{level}  {}  {range}\n", hit.name, hit.relative_path)
            };
            let prefix = if heading_pending {
                Some(heading.as_str())
            } else {
                None
            };
            if body.push_with_prefix(prefix, &entry) {
                heading_pending = false;
            }
        }
    }
    format!(
        "{total} symbol(s) under `{path}`{}:\n{}",
        list_note(total, 0, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`list_symbols`]: a flat table (no per-kind grouping —
/// `kind` is just another column) instead of headed groups, since TOON's
/// whole point is one header row instead of repeated structure per group.
pub fn list_symbols_toon(
    path: &str,
    is_file: bool,
    hits: &[SymbolListEntry],
    limit: usize,
) -> String {
    if hits.is_empty() {
        return format!("No symbols found under `{path}`.");
    }
    let total = hits.len();
    let shown = paginate(hits, 0, limit);
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|hit| {
            let mut row = vec![hit.kind.clone(), hit.name.clone()];
            if !is_file {
                row.push(hit.relative_path.clone());
            }
            row.push(hit.line.to_string());
            row.push(hit.end_line.map(|e| e.to_string()).unwrap_or_default());
            row.push(hit.level.map(|l| l.to_string()).unwrap_or_default());
            row
        })
        .collect();
    let mut headers = vec!["kind", "name"];
    if !is_file {
        headers.push("path");
    }
    headers.push("line");
    headers.push("end_line");
    headers.push("level");
    format!(
        "{total} symbol(s) under `{path}`{}:\n{}",
        truncation_note(total, 0, shown.len()),
        encode_table("symbols", &headers, &rows)
    )
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

/// Renders `get_file_tree`: an indented directory/file listing, two spaces
/// per nesting level, directories suffixed with `/`. Truncated at
/// [`DEFAULT_BYTE_BUDGET`] like every other list-shaped tool here, cut at
/// the last full line rather than mid-line, so a huge tree can't blow up a
/// single response.
pub fn file_tree(display_path: &str, root: &FileTreeNode, depth: u32) -> String {
    let mut body = String::new();
    render_tree_node(root, 0, &mut body);

    let mut out = format!("File tree of `{display_path}` (depth {depth}):\n");
    if body.len() <= DEFAULT_BYTE_BUDGET {
        out.push_str(&body);
        return out;
    }
    let mut boundary = DEFAULT_BYTE_BUDGET;
    while boundary > 0 && !body.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let cut = body[..boundary].rfind('\n').map(|i| i + 1).unwrap_or(0);
    out.push_str(&body[..cut]);
    out.push_str(&format!(
        "... (truncated at the {DEFAULT_BYTE_BUDGET}-byte response budget — narrow with `path` or lower `depth`)\n"
    ));
    out
}

fn render_tree_node(node: &FileTreeNode, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let suffix = if node.is_dir { "/" } else { "" };
    out.push_str(&format!("{pad}{}{suffix}\n", node.name));
    for child in &node.children {
        render_tree_node(child, indent + 1, out);
    }
    if node.omitted > 0 {
        out.push_str(&format!("{pad}  (+{} more)\n", node.omitted));
    }
    if node.depth_exhausted {
        out.push_str(&format!("{pad}  ...\n"));
    }
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
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    for hit in shown {
        body.push(&format!(
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
    format!(
        "{total} {}{}:\n{}",
        verb_label,
        list_note(total, offset, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`relation_hits`] — backs `find_references`,
/// `find_calls` and `find_callers` alike, same as the text version. `depth`
/// is always a column here (unlike the text version's `depth_tag`, which
/// omits it for depth-1 hits) since a table column can't be conditionally
/// absent per row.
pub fn relation_hits_toon(
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
    let rows: Vec<Vec<String>> = shown.iter().map(relation_hit_row).collect();
    format!(
        "{total} {verb_label}{}:\n{}",
        truncation_note(total, offset, shown.len()),
        encode_table(
            "relations",
            &["path", "line", "column", "language", "from", "kind", "to", "depth"],
            &rows,
        )
    )
}

fn relation_hit_row(hit: &RelationHit) -> Vec<String> {
    vec![
        hit.relative_path.clone(),
        hit.line.to_string(),
        hit.column.to_string(),
        hit.language.clone(),
        hit.from_symbol.clone(),
        hit.kind.clone(),
        hit.to_name.clone(),
        hit.depth.to_string(),
    ]
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
    // is huge. The byte budget is split the same way: only sections that
    // actually have content get a share, so the first section can't eat the
    // whole response and starve the other two.
    let populated = [
        !affected_tests.is_empty(),
        !callers.is_empty(),
        !references.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let section_budget = DEFAULT_BYTE_BUDGET / populated.max(1);

    if !affected_tests.is_empty() {
        let shown = paginate(affected_tests, offset, limit);
        let mut body = BudgetedList::new(section_budget);
        for hit in shown {
            body.push(&format!(
                "  {}:{}:{} [{}] {}{}\n",
                hit.relative_path,
                hit.line,
                hit.column,
                hit.language,
                hit.from_symbol,
                depth_tag(hit.depth)
            ));
        }
        out.push_str(&format!(
            "\nLikely affected tests{}:\n{}",
            list_note(affected_tests.len(), offset, shown.len(), &body),
            body.body
        ));
    }

    if !callers.is_empty() {
        let shown = paginate(callers, offset, limit);
        let mut body = BudgetedList::new(section_budget);
        for hit in shown {
            body.push(&format!(
                "  {}:{}:{} [{}] {}{}\n",
                hit.relative_path,
                hit.line,
                hit.column,
                hit.language,
                hit.from_symbol,
                depth_tag(hit.depth)
            ));
        }
        out.push_str(&format!(
            "\nDirect callers{}:\n{}",
            list_note(callers.len(), offset, shown.len(), &body),
            body.body
        ));
    }

    if !references.is_empty() {
        let shown = paginate(references, offset, limit);
        let mut body = BudgetedList::new(section_budget);
        for hit in shown {
            body.push(&format!(
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
        out.push_str(&format!(
            "\nAll references{}:\n{}",
            list_note(references.len(), offset, shown.len(), &body),
            body.body
        ));
    }

    if callers.is_empty() && references.is_empty() {
        out.push_str("\nNothing else in the index references this symbol.\n");
    }
    out
}

/// TOON rendering of [`impact_analysis`]: the same three sections, each its
/// own TOON table, under the same summary header line the text version
/// opens with (kept as plain text — it's three scalar counts, not a table).
pub fn impact_analysis_toon(
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

    let headers = [
        "path", "line", "column", "language", "from", "kind", "to", "depth",
    ];

    if !affected_tests.is_empty() {
        let shown = paginate(affected_tests, offset, limit);
        let rows: Vec<Vec<String>> = shown.iter().map(|hit| relation_hit_row(hit)).collect();
        out.push_str(&format!(
            "\nLikely affected tests{}:\n{}",
            truncation_note(affected_tests.len(), offset, shown.len()),
            encode_table("tests", &headers, &rows)
        ));
    }

    if !callers.is_empty() {
        let shown = paginate(callers, offset, limit);
        let rows: Vec<Vec<String>> = shown.iter().map(relation_hit_row).collect();
        out.push_str(&format!(
            "\nDirect callers{}:\n{}",
            truncation_note(callers.len(), offset, shown.len()),
            encode_table("callers", &headers, &rows)
        ));
    }

    if !references.is_empty() {
        let shown = paginate(references, offset, limit);
        let rows: Vec<Vec<String>> = shown.iter().map(relation_hit_row).collect();
        out.push_str(&format!(
            "\nAll references{}:\n{}",
            truncation_note(references.len(), offset, shown.len()),
            encode_table("references", &headers, &rows)
        ));
    }

    if callers.is_empty() && references.is_empty() {
        out.push_str("\nNothing else in the index references this symbol.\n");
    }
    out
}

/// How far above a definition [`leading_comment_start`] walks before giving
/// up — a license header glued to the first function isn't its doc comment.
const MAX_LEADING_COMMENT_LINES: usize = 40;

/// Line prefixes that mark a doc comment, attribute or decorator directly
/// above a definition, across the supported languages: `//`/`///`/`//!`,
/// `/* */` blocks and their `*` continuations, `#` comments and `#[...]`
/// attributes, `@` decorators/annotations, `--` (Lua/SQL) and `<!--`.
const LEADING_COMMENT_MARKERS: &[&str] = &["//", "/*", "*", "#", "@", "--", "<!--"];

/// The 1-based line where the comment/attribute block directly above the
/// definition at `line` starts, or `line` itself when there is none. The index
/// stores no doc-comment text, so `build_context_pack` recovers it from the
/// source: every contiguous line above the definition that starts with a
/// [`LEADING_COMMENT_MARKERS`] entry (or is a whole `[Attribute]` line),
/// stopping at the first blank or code line. Language-agnostic on purpose —
/// this crate never branches on a language id.
pub fn leading_comment_start(source: &str, line: u32) -> u32 {
    let lines: Vec<&str> = source.lines().collect();
    let line = line.max(1) as usize;
    let mut start = line;
    while start > 1 && line - start < MAX_LEADING_COMMENT_LINES {
        let above = lines.get(start - 2).map_or("", |l| l.trim());
        let is_comment = LEADING_COMMENT_MARKERS.iter().any(|m| above.starts_with(m))
            || (above.starts_with('[') && above.ends_with(']'));
        if !is_comment {
            break;
        }
        start -= 1;
    }
    start as u32
}

/// Longest signature [`signature_line`] returns before cutting it with `…`.
const MAX_SIGNATURE_CHARS: usize = 120;

/// The trimmed source text of 1-based `line` — a definition's first line,
/// used as its signature — cut at [`MAX_SIGNATURE_CHARS`] on a char boundary.
/// `None` when the line is out of range (the file changed since the last
/// reindex).
pub fn signature_line(source: &str, line: u32) -> Option<String> {
    let text = source.lines().nth((line.max(1) - 1) as usize)?.trim();
    if text.chars().count() <= MAX_SIGNATURE_CHARS {
        return Some(text.to_string());
    }
    let cut: String = text.chars().take(MAX_SIGNATURE_CHARS).collect();
    Some(format!("{cut}…"))
}

/// How many file paths or unresolved callee names `build_context_pack` spells
/// out per footer line before summarising the rest as a count.
const CONTEXT_PACK_MAX_LISTED_NAMES: usize = 12;

fn context_pack_header(pack: &crate::server::ContextPack) -> String {
    let mut out = format!(
        "Context pack for `{}` (depth {}): {} definition(s), {} related symbol(s)",
        pack.symbol,
        pack.depth,
        pack.definitions.len() + pack.omitted_definitions,
        pack.related.len()
    );
    if !pack.external.is_empty() {
        out.push_str(&format!(", {} external call(s)", pack.external.len()));
    }
    out.push('\n');
    if pack.omitted_definitions > 0 {
        out.push_str(&format!(
            "({} more definition(s) of `{}` not shown — narrow with `path`/`language`)\n",
            pack.omitted_definitions, pack.symbol
        ));
    }
    for def in &pack.definitions {
        let hit = &def.hit;
        out.push_str(&format!(
            "\n{}:{} [{}] {} {}\n{}",
            hit.relative_path,
            line_range(hit.line, hit.end_line),
            hit.language,
            hit.kind,
            hit.name,
            def.snippet
        ));
        if def.hidden_lines > 0 {
            out.push_str(&format!(
                "    … {} more line(s) — raise `source_lines` to see them\n",
                def.hidden_lines
            ));
        }
    }
    out
}

/// `label: a, b, c (+N more)` on its own line, or nothing for an empty list.
fn context_pack_name_line(label: &str, names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let shown: Vec<&str> = names
        .iter()
        .take(CONTEXT_PACK_MAX_LISTED_NAMES)
        .map(String::as_str)
        .collect();
    let more = names.len().saturating_sub(shown.len());
    let suffix = if more > 0 {
        format!(" (+{more} more)")
    } else {
        String::new()
    };
    format!("{label}: {}{suffix}\n", shown.join(", "))
}

fn context_pack_footer(pack: &crate::server::ContextPack) -> String {
    let lines =
        context_pack_name_line("Referenced at file level (use/import) by", &pack.file_level)
            + &context_pack_name_line(
                "Called but not defined in the index (std/third-party)",
                &pack.external,
            );
    if lines.is_empty() {
        lines
    } else {
        format!("\n{lines}")
    }
}

/// `lines` column of a related symbol: its definition's line range, plus how
/// many other definitions share its name when it's ambiguous.
fn related_location(related: &crate::server::PackedRelated) -> (String, String) {
    match &related.definition {
        Some(def) => {
            let mut lines = line_range(def.line, def.end_line);
            if related.other_definitions > 0 {
                lines.push_str(&format!(" (+{} more def)", related.other_definitions));
            }
            (def.relative_path.clone(), lines)
        }
        None => (String::new(), String::new()),
    }
}

/// Renders `build_context_pack`: the packed symbol's definition(s) with their
/// doc comment and (capped) source, then every related symbol once — its
/// roles merged into one row however many relations connect it — with the
/// location and one-line signature of its definition, then the called names
/// the index has no definition for. Related rows are `limit`-capped and
/// byte-budgeted against what the definitions already used.
pub fn context_pack(pack: &crate::server::ContextPack, limit: usize) -> String {
    let mut out = context_pack_header(pack);
    if pack.related.is_empty() {
        out.push_str("\nNo related symbols in the index.\n");
    } else {
        let shown = paginate(&pack.related, 0, limit);
        let budget = DEFAULT_BYTE_BUDGET.saturating_sub(out.len()).max(1);
        let mut body = BudgetedList::new(budget);
        for related in shown {
            let (path, lines) = related_location(related);
            let kind = related.definition.as_ref().map_or("", |d| d.kind.as_str());
            let signature = related
                .signature
                .as_deref()
                .map(|s| format!("  | {s}"))
                .unwrap_or_default();
            body.push(&format!(
                "  {} {}{}  {path}:{lines} {kind}{signature}\n",
                related.roles.join(","),
                related.name,
                depth_tag(related.hop),
            ));
        }
        out.push_str(&format!(
            "\nRelated symbols, each listed once{}:\n{}",
            list_note(pack.related.len(), 0, shown.len(), &body),
            body.body
        ));
    }
    out.push_str(&context_pack_footer(pack));
    out
}

/// TOON rendering of [`context_pack`]: the definitions' source stays as-is
/// (it isn't tabular), the related symbols become one TOON table.
pub fn context_pack_toon(pack: &crate::server::ContextPack, limit: usize) -> String {
    let mut out = context_pack_header(pack);
    if pack.related.is_empty() {
        out.push_str("\nNo related symbols in the index.\n");
    } else {
        let shown = paginate(&pack.related, 0, limit);
        let rows: Vec<Vec<String>> = shown
            .iter()
            .map(|related| {
                let (path, lines) = related_location(related);
                vec![
                    related.name.clone(),
                    related.roles.join(" "),
                    related.hop.to_string(),
                    path,
                    lines,
                    related
                        .definition
                        .as_ref()
                        .map(|d| d.kind.clone())
                        .unwrap_or_default(),
                    related.signature.clone().unwrap_or_default(),
                ]
            })
            .collect();
        out.push_str(&format!(
            "\nRelated symbols, each listed once{}:\n{}",
            truncation_note(pack.related.len(), 0, shown.len()),
            encode_table(
                "related",
                &["name", "roles", "hop", "path", "lines", "kind", "signature"],
                &rows,
            )
        ));
    }
    out.push_str(&context_pack_footer(pack));
    out
}

/// Renders `find_dead_code`'s result: candidate symbols with zero indexed
/// references, grouped by file (`hits` already arrives sorted by
/// `relative_path, line` — the same order [`list_symbols`] renders in).
/// Carries a fixed caveat: this is a heuristic over indexed relations, not
/// real export/dynamic-dispatch analysis (see the tool's own description).
pub fn find_dead_code(path: &str, hits: &[SymbolListEntry], offset: usize, limit: usize) -> String {
    if hits.is_empty() {
        return format!("No dead-code candidates found under `{path}`.");
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    let mut last_path: Option<&str> = None;
    for hit in shown {
        let heading = if last_path != Some(hit.relative_path.as_str()) {
            Some(format!("{}:\n", hit.relative_path))
        } else {
            None
        };
        let range = line_range(hit.line, hit.end_line);
        let entry = format!("  {} {} {range}\n", hit.kind, hit.name);
        if body.push_with_prefix(heading.as_deref(), &entry) {
            last_path = Some(hit.relative_path.as_str());
        }
    }
    format!(
        "{total} dead-code candidate(s) under `{path}`{}:\n\
         (heuristic: zero indexed references found for this name anywhere in the project — \
         does not account for dynamic dispatch, reflection, or a genuinely public API with no \
         in-repo caller yet; verify before deleting)\n\n{}",
        list_note(total, offset, shown.len(), &body),
        body.body
    )
}

/// TOON rendering of [`find_dead_code`]. Carries the same caveat text as the
/// header (not a table column — it's one caveat for the whole result, not
/// per-row data).
pub fn find_dead_code_toon(
    path: &str,
    hits: &[SymbolListEntry],
    offset: usize,
    limit: usize,
) -> String {
    if hits.is_empty() {
        return format!("No dead-code candidates found under `{path}`.");
    }
    let total = hits.len();
    let shown = paginate(hits, offset, limit);
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|hit| {
            vec![
                hit.kind.clone(),
                hit.name.clone(),
                hit.relative_path.clone(),
                hit.line.to_string(),
                hit.end_line.map(|e| e.to_string()).unwrap_or_default(),
            ]
        })
        .collect();
    format!(
        "{total} dead-code candidate(s) under `{path}`{}:\n\
         (heuristic: zero indexed references found for this name anywhere in the project — \
         does not account for dynamic dispatch, reflection, or a genuinely public API with no \
         in-repo caller yet; verify before deleting)\n\n{}",
        truncation_note(total, offset, shown.len()),
        encode_table(
            "candidates",
            &["kind", "name", "path", "line", "end_line"],
            &rows
        )
    )
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
///
/// Modules with nothing to surface are left out of the body entirely and
/// only counted in the header: a `(no top-level symbols)` line per empty
/// module is pure cost. Truncation against [`DEFAULT_BYTE_BUDGET`] drops
/// whole modules, never half of one, and the header says how many.
pub fn overview(
    root_path: &str,
    modules: &[crate::server::ModuleDigest],
    max_symbols_per_module: u32,
) -> String {
    if modules.is_empty() {
        return format!("No symbols found under `{root_path}`.");
    }
    let mut body = BudgetedList::new(DEFAULT_BYTE_BUDGET);
    let mut empty_modules = 0usize;
    for module in modules {
        if module.symbols.is_empty() {
            empty_modules += 1;
            continue;
        }
        let mut block = format!("\n{}:\n", module.relative_path);
        for symbol in &module.symbols {
            let range = line_range(symbol.line, symbol.end_line);
            block.push_str(&format!("  [{}] {} {range}\n", symbol.kind, symbol.name));
            let Some((_, callers)) = module
                .relations
                .iter()
                .find(|(name, _)| name == &symbol.name)
            else {
                continue;
            };
            for caller in callers {
                block.push_str(&format!(
                    "      <- {} ({}:{})\n",
                    caller.from_symbol, caller.relative_path, caller.line
                ));
            }
        }
        if module.omitted > 0 {
            block.push_str(&format!("  (+{} more)\n", module.omitted));
        }
        body.push(&block);
    }

    let mut header = format!(
        "Project overview of `{root_path}` ({} module(s), up to {max_symbols_per_module} symbol(s) each",
        modules.len()
    );
    if empty_modules > 0 {
        header.push_str(&format!(
            ", {empty_modules} with no indexed top-level symbols omitted"
        ));
    }
    if body.dropped > 0 {
        header.push_str(&format!(
            ", {} of {} shown — {} more dropped at the {DEFAULT_BYTE_BUDGET}-byte response budget, narrow with `path`",
            body.kept,
            body.kept + body.dropped,
            body.dropped
        ));
    }
    header.push_str("):\n");
    header.push_str(&body.body);
    header
}

/// Renders `get_indexing_status`. `verbose_dependencies` controls only the
/// dependency section: `false` collapses it to a single summary line,
/// `true` keeps the full per-manifest listing. That section measured 97% of
/// this tool's payload on this repo — a staleness check shouldn't cost more
/// than the queries it guards — while every other section answers the
/// tool's actual question and is rendered identically either way.
pub fn index_status(status: &IndexStatus, verbose_dependencies: bool) -> String {
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
        out.push_str(&format!(
            "{} file(s) failed to parse:\n",
            status.syntax_errors.len()
        ));
        for err in &status.syntax_errors {
            out.push_str(&format!("  {}: {}\n", err.relative_path, err.detail));
        }
    }
    if !status.dependencies.is_empty() {
        if verbose_dependencies {
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
        } else {
            let declared: usize = status
                .dependencies
                .iter()
                .map(|manifest| manifest.dependencies.len())
                .sum();
            // Uniqueness is by dependency name: a workspace repeats the same
            // handful of names across every manifest, which is exactly the
            // repetition that made this section 97% of the payload.
            let unique: BTreeSet<&str> = status
                .dependencies
                .iter()
                .flat_map(|manifest| manifest.dependencies.iter())
                .map(|dep| dep.name.as_str())
                .collect();
            out.push_str(&format!(
                "Dependencies: {} manifests, {declared} declared ({} unique external).\n",
                status.dependencies.len(),
                unique.len()
            ));
        }
    }
    out
}

/// Renders `discover_tool_categories`' output: every registered tool's name
/// and description, grouped under the category headings in `categories`, no
/// input schemas. A tool present in `catalog` but not listed in any category
/// still appears, under an `other` heading — a tool added to `server.rs`
/// without a matching `TOOL_CATEGORIES` entry should stay discoverable rather
/// than silently vanish from this listing.
pub fn tool_categories(catalog: &[rmcp::model::Tool], categories: &[(&str, &[&str])]) -> String {
    let mut out = String::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (heading, names) in categories {
        out.push_str(&format!("{heading}:\n"));
        for name in *names {
            seen.insert(name);
            let description = catalog
                .iter()
                .find(|t| t.name == *name)
                .and_then(|t| t.description.as_deref())
                .unwrap_or("(not registered)");
            out.push_str(&format!("  {name} — {description}\n"));
        }
    }
    let uncategorized: Vec<&rmcp::model::Tool> = catalog
        .iter()
        .filter(|t| !seen.contains(t.name.as_ref()))
        .collect();
    if !uncategorized.is_empty() {
        out.push_str("other:\n");
        for tool in uncategorized {
            let description = tool.description.as_deref().unwrap_or("");
            out.push_str(&format!("  {} — {description}\n", tool.name));
        }
    }
    out.push_str("\nCall get_tool_schema with one of these names for its full input schema.\n");
    out
}

/// What happened to one `batch` sub-query.
pub enum BatchOutcome {
    /// The tool's own rendered output, unchanged.
    Ok(String),
    /// The error message a direct call would have returned.
    Err(String),
    /// Not run: the batch's byte budget was already spent.
    Skipped,
}

impl BatchOutcome {
    /// Bytes this outcome contributes to the batch response, for the
    /// caller's budget accounting.
    pub fn len(&self) -> usize {
        match self {
            BatchOutcome::Ok(text) | BatchOutcome::Err(text) => text.len(),
            BatchOutcome::Skipped => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Renders `batch`'s output: one count line, then each sub-query under a
/// `[n] tool` header in request order. Each sub-result is the tool's own
/// output verbatim — a batch only drops the per-call envelope, it never
/// reformats what a tool returns.
pub fn batch(outcomes: &[(String, BatchOutcome)]) -> String {
    let failed = outcomes
        .iter()
        .filter(|(_, o)| matches!(o, BatchOutcome::Err(_)))
        .count();
    let skipped = outcomes
        .iter()
        .filter(|(_, o)| matches!(o, BatchOutcome::Skipped))
        .count();
    let mut out = format!(
        "batch: {} quer{}",
        outcomes.len(),
        if outcomes.len() == 1 { "y" } else { "ies" }
    );
    if failed > 0 {
        out.push_str(&format!(", {failed} failed"));
    }
    if skipped > 0 {
        out.push_str(&format!(
            ", {skipped} skipped (byte budget reached — rerun them in another batch)"
        ));
    }
    out.push('\n');
    for (i, (tool, outcome)) in outcomes.iter().enumerate() {
        let n = i + 1;
        match outcome {
            BatchOutcome::Ok(text) => {
                out.push_str(&format!("\n[{n}] {tool}\n{}\n", text.trim_end()));
            }
            BatchOutcome::Err(message) => {
                out.push_str(&format!("\n[{n}] {tool} error: {message}\n"))
            }
            BatchOutcome::Skipped => out.push_str(&format!("\n[{n}] {tool} skipped\n")),
        }
    }
    out
}

/// Renders `get_tool_schema`'s output for one already-resolved tool: its
/// description and full JSON input schema, pretty-printed. The schema is
/// built by `schemars` from the tool's `Parameters<...>` struct at server
/// startup, so this always reflects what the tool actually accepts.
pub fn tool_schema(tool: &rmcp::model::Tool) -> String {
    let description = tool.description.as_deref().unwrap_or("(no description)");
    let schema = serde_json::to_string_pretty(&*tool.input_schema)
        .unwrap_or_else(|_| "(failed to render input schema)".to_string());
    format!(
        "{}\n\n{description}\n\nInput schema:\n{schema}\n",
        tool.name
    )
}

#[cfg(test)]
mod file_skeleton_tests {
    use super::*;

    fn entry(
        name: &str,
        kind: &str,
        language: &str,
        line: u32,
        end_line: Option<u32>,
    ) -> SymbolListEntry {
        SymbolListEntry {
            name: name.to_string(),
            kind: kind.to_string(),
            language: language.to_string(),
            relative_path: "f".to_string(),
            line,
            end_line,
            parent: None,
            level: None,
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
        assert!(
            !out.contains("// ..."),
            "no brace within bound — must not fabricate a body: {out}"
        );
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
        assert!(
            !out.contains("class Next"),
            "must not consume a later symbol's brace: {out}"
        );
    }

    #[test]
    fn non_brace_language_only_shows_the_declaration_line() {
        let source = "def test_compute():\n    assert compute() == 1\n";
        let entries = [entry("test_compute", "function", "python", 1, Some(2))];
        let out = file_skeleton("f.py", &entries, source);
        assert!(out.contains("def test_compute():"), "got: {out}");
        assert!(out.contains("# ..."), "got: {out}");
        assert!(
            !out.contains("assert compute"),
            "body must not leak through: {out}"
        );
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

#[cfg(test)]
mod file_tree_tests {
    use super::*;

    fn dir(name: &str, children: Vec<FileTreeNode>) -> FileTreeNode {
        FileTreeNode {
            name: name.to_string(),
            is_dir: true,
            children,
            omitted: 0,
            depth_exhausted: false,
        }
    }

    fn file(name: &str) -> FileTreeNode {
        FileTreeNode {
            name: name.to_string(),
            is_dir: false,
            children: Vec::new(),
            omitted: 0,
            depth_exhausted: false,
        }
    }

    #[test]
    fn directories_are_suffixed_and_nesting_is_indented() {
        let root = dir(
            ".",
            vec![dir("src", vec![file("lib.rs")]), file("Cargo.toml")],
        );
        let out = file_tree(".", &root, 3);
        assert!(out.contains("./\n"), "got: {out}");
        assert!(out.contains("  src/\n"), "got: {out}");
        assert!(out.contains("    lib.rs\n"), "got: {out}");
        assert!(out.contains("  Cargo.toml\n"), "got: {out}");
    }

    #[test]
    fn omitted_and_depth_exhausted_are_each_noted_once() {
        let capped = FileTreeNode {
            name: "many".to_string(),
            is_dir: true,
            children: vec![file("a")],
            omitted: 5,
            depth_exhausted: false,
        };
        let stopped = FileTreeNode {
            name: "deep".to_string(),
            is_dir: true,
            children: Vec::new(),
            omitted: 0,
            depth_exhausted: true,
        };
        let root = dir(".", vec![capped, stopped]);
        let out = file_tree(".", &root, 1);
        assert!(out.contains("(+5 more)"), "got: {out}");
        assert!(out.contains("deep/\n    ...\n"), "got: {out}");
    }

    #[test]
    fn an_oversized_tree_is_truncated_at_a_line_boundary() {
        let children: Vec<FileTreeNode> = (0..5000)
            .map(|i| file(&format!("file_{i:05}.txt")))
            .collect();
        let root = dir(".", children);
        let out = file_tree(".", &root, 1);
        assert!(
            out.ends_with("byte response budget — narrow with `path` or lower `depth`)\n"),
            "must end with the truncation note, not mid-entry: {out}"
        );
        assert!(
            out.len() < 5000 * 20,
            "must actually be capped well below the full tree: {} bytes",
            out.len()
        );
    }
}

#[cfg(test)]
mod budget_tests {
    // Test code: a panic! here means a broken test precondition, and
    // panicking is the correct behavior — this module only touches
    // fixtures the test builds itself, never repo-input content.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn relation(from: &str, path: &str, line: u32, column: u32) -> RelationHit {
        RelationHit {
            kind: "calls".to_string(),
            from_symbol: from.to_string(),
            to_name: "target".to_string(),
            language: "rust".to_string(),
            relative_path: path.to_string(),
            line,
            column,
            depth: 1,
        }
    }

    /// Splits a rendered response into its (always-emitted) header line and
    /// the body the byte budget actually governs.
    fn body_of(out: &str) -> &str {
        out.split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or_default()
    }

    #[test]
    fn a_relation_result_under_budget_is_byte_identical_to_the_pre_budget_rendering() {
        // The regression guard that matters: nothing omitted, offset 0 —
        // output must be exactly what this formatter produced before a byte
        // budget existed, character for character.
        let hits = [
            relation("alpha", "src/a.rs", 3, 5),
            relation("beta", "src/b.rs", 7, 2),
        ];
        let out = relation_hits("target", "caller(s) of this function", &hits, 0, 50);
        assert_eq!(
            out,
            "2 caller(s) of this function:\n\
             src/a.rs:3:5 [rust] alpha --calls--> target\n\
             src/b.rs:7:2 [rust] beta --calls--> target\n"
        );
    }

    #[test]
    fn a_symbol_result_under_budget_is_byte_identical_to_the_pre_budget_rendering() {
        let hits = [SymbolHit {
            name: "compute".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            relative_path: "src/lib.rs".to_string(),
            line: 10,
            column: 1,
            parent: None,
            end_line: Some(12),
            level: None,
        }];
        let out = symbol_hits("compute", &hits, 50);
        assert_eq!(
            out,
            "1 definition(s) of `compute`:\nsrc/lib.rs:10:1 [rust] function compute\n"
        );
    }

    /// Mirrors `relation_hits`' per-entry rendering so the budget tests can
    /// assert the kept body is *exactly* the first N entries — the check
    /// that proves truncation never lands mid-line or mid-codepoint.
    fn rendered_entry(hit: &RelationHit) -> String {
        format!(
            "{}:{}:{} [{}] {} --{}--> {}\n",
            hit.relative_path,
            hit.line,
            hit.column,
            hit.language,
            hit.from_symbol,
            hit.kind,
            hit.to_name
        )
    }

    fn many_relations(
        count: usize,
        name: impl Fn(usize) -> String,
        path: impl Fn(usize) -> String,
    ) -> Vec<RelationHit> {
        (0..count)
            .map(|i| relation(&name(i), &path(i), 42, 7))
            .collect()
    }

    #[test]
    fn a_relation_result_over_budget_truncates_at_a_line_boundary_and_says_so() {
        let hits = many_relations(
            600,
            |i| format!("caller_number_{i:04}"),
            |i| format!("crates/some-crate/src/deeply/nested/module_{i:04}.rs"),
        );
        // `limit` is deliberately high enough that row-count pagination
        // cannot be what truncates here — only the byte budget can.
        let out = relation_hits("target", "caller(s) of this function", &hits, 0, 1000);
        let body = body_of(&out);

        assert!(
            body.len() <= DEFAULT_BYTE_BUDGET,
            "body was {} bytes, budget is {DEFAULT_BYTE_BUDGET}",
            body.len()
        );
        let kept = body.lines().count();
        assert!(
            kept > 0 && kept < 600,
            "expected a partial result, got {kept}"
        );

        let expected: String = hits[..kept].iter().map(rendered_entry).collect();
        assert_eq!(
            body, expected,
            "body must be exactly the first {kept} whole entries"
        );

        assert!(
            out.starts_with("600 caller(s) of this function (showing "),
            "got: {out}"
        );
        assert!(
            out.contains(&format!(
                "(showing {kept} of 600, truncated at the {DEFAULT_BYTE_BUDGET}-byte response budget"
            )),
            "got: {out}"
        );
        assert!(
            out.contains("narrow with `path`/`language` or pass `offset` to page"),
            "got: {out}"
        );
    }

    #[test]
    fn multi_byte_content_near_the_budget_boundary_is_never_split_mid_codepoint() {
        // Every entry carries multi-byte UTF-8 in both the symbol name and
        // the path, so a naive byte-offset cut would land inside a
        // codepoint for most budgets.
        let hits = many_relations(
            600,
            |i| format!("función_ñáéíóú_{i:04}"),
            |i| format!("crates/日本語パッケージ/src/módulo_{i:04}.rs"),
        );
        let out = relation_hits("target", "caller(s) of this function", &hits, 0, 1000);
        let body = body_of(&out);

        assert!(
            body.len() <= DEFAULT_BYTE_BUDGET,
            "got {} bytes",
            body.len()
        );
        let kept = body.lines().count();
        assert!(
            kept > 0 && kept < 600,
            "expected a partial result, got {kept}"
        );

        let expected: String = hits[..kept].iter().map(rendered_entry).collect();
        assert_eq!(
            body, expected,
            "truncation must fall on a whole-entry boundary"
        );
        // A String cannot hold invalid UTF-8, so the real risk is a
        // *logically* truncated name; assert the last one is complete.
        assert!(
            body.ends_with(&rendered_entry(&hits[kept - 1])),
            "last entry must be whole: {body:?}"
        );
    }

    #[test]
    fn list_symbols_over_budget_never_strands_a_group_heading() {
        let hits: Vec<SymbolListEntry> = (0..800)
            .map(|i| SymbolListEntry {
                name: format!("symbol_with_a_fairly_long_name_{i:04}"),
                // Alternating kinds so several group headings are in play.
                kind: if i % 2 == 0 { "function" } else { "struct" }.to_string(),
                language: "rust".to_string(),
                relative_path: format!("crates/some-crate/src/nested/module_{i:04}.rs"),
                line: 1,
                end_line: Some(9),
                parent: None,
                level: None,
            })
            .collect();
        let out = list_symbols("crates", false, &hits, 1000);
        let body = body_of(&out);

        assert!(
            body.len() <= DEFAULT_BYTE_BUDGET,
            "got {} bytes",
            body.len()
        );
        assert!(out.contains("response budget"), "got: {out}");
        // Functions come first in KIND_HEADINGS, so the budget runs out
        // before the Structs group — its heading must not be emitted alone.
        for (idx, line) in body.lines().enumerate() {
            if line.ends_with(':') {
                assert!(
                    body.lines()
                        .nth(idx + 1)
                        .is_some_and(|next| next.starts_with("  ")),
                    "heading `{line}` has no entries under it: {body}"
                );
            }
        }
    }

    #[test]
    fn list_symbols_separates_columns_with_two_spaces_instead_of_padding() {
        let entry = |name: &str, kind: &str, line: u32, end_line: Option<u32>| SymbolListEntry {
            name: name.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            relative_path: "src/lib.rs".to_string(),
            line,
            end_line,
            parent: None,
            level: None,
        };
        let hits = [
            entry("compute", "function", 10, Some(12)),
            entry("a_much_longer_function_name", "function", 20, None),
            entry("Thing", "struct", 1, Some(5)),
        ];
        let out = list_symbols("src/lib.rs", true, &hits, 50);
        assert_eq!(
            out,
            "3 symbol(s) under `src/lib.rs`:\n\
             Functions:\n\
             \x20 compute  L10-L12\n\
             \x20 a_much_longer_function_name  L20\n\
             Structs:\n\
             \x20 Thing  L1-L5\n"
        );
    }

    #[test]
    fn impact_analysis_splits_the_budget_across_the_sections_that_have_content() {
        let make = |prefix: &str| {
            many_relations(
                400,
                |i| format!("{prefix}_caller_number_{i:04}"),
                |i| format!("crates/some-crate/src/deeply/nested/module_{i:04}.rs"),
            )
        };
        let callers = make("c");
        let references = make("r");
        let tests: Vec<&RelationHit> = callers.iter().take(400).collect();

        let out = impact_analysis("target", &callers, &references, &tests, 0, 1000);

        // All three sections must survive: none may eat the whole budget.
        assert!(out.contains("\nLikely affected tests ("), "got: {out}");
        assert!(out.contains("\nDirect callers ("), "got: {out}");
        assert!(out.contains("\nAll references ("), "got: {out}");
        assert_eq!(out.matches("response budget").count(), 3, "got: {out}");

        for section in ["Likely affected tests", "Direct callers", "All references"] {
            let Some(start) = out.find(&format!("\n{section} (")) else {
                panic!("missing section {section}");
            };
            let rest = &out[start + 1..];
            let section_len = rest.find("\n\n").unwrap_or(rest.len());
            assert!(
                section_len <= DEFAULT_BYTE_BUDGET / 3 + 256,
                "section `{section}` was {section_len} bytes, over its third of the budget"
            );
        }
    }
}

#[cfg(test)]
mod index_status_tests {
    use super::*;
    use mct_index::{DependencyInfo, LanguageCoverage, ManifestDependencies};

    /// `syntax_errors` is left empty because `UnsupportedKind` isn't
    /// re-exported from `mct-index`, so it can't be constructed from here;
    /// that section's rendering is untouched by this change either way.
    fn sample_status() -> IndexStatus {
        IndexStatus {
            languages: vec![LanguageCoverage {
                language: "rust".to_string(),
                file_count: 2,
                symbol_count: 10,
            }],
            total_files: 2,
            total_symbols: 10,
            last_indexed_at: Some(1_700_000_000),
            unsupported_languages: vec!["lua".to_string()],
            syntax_errors: Vec::new(),
            dependencies: vec![
                ManifestDependencies {
                    manifest_path: "Cargo.toml".to_string(),
                    language: "rust".to_string(),
                    dependencies: vec![
                        DependencyInfo {
                            name: "serde".to_string(),
                            version: Some("1.0".to_string()),
                        },
                        DependencyInfo {
                            name: "rusqlite".to_string(),
                            version: Some("0.31".to_string()),
                        },
                    ],
                },
                ManifestDependencies {
                    manifest_path: "crates/x/Cargo.toml".to_string(),
                    language: "rust".to_string(),
                    dependencies: vec![DependencyInfo {
                        name: "serde".to_string(),
                        version: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn the_default_summarises_dependencies_to_one_line() {
        let out = index_status(&sample_status(), false);
        assert!(
            out.contains("Dependencies: 2 manifests, 3 declared (2 unique external).\n"),
            "got: {out}"
        );
        assert!(
            !out.contains("serde"),
            "no manifest detail by default: {out}"
        );
        assert!(!out.contains("Dependencies detected"), "got: {out}");
    }

    #[test]
    fn verbose_keeps_the_full_per_manifest_listing() {
        let out = index_status(&sample_status(), true);
        assert!(out.contains("Dependencies detected:\n"), "got: {out}");
        assert!(
            out.contains("  Cargo.toml (rust, 2 dep(s)):\n"),
            "got: {out}"
        );
        assert!(out.contains("    serde 1.0\n"), "got: {out}");
        assert!(out.contains("    rusqlite 0.31\n"), "got: {out}");
        // A path/workspace dependency has no version and must still render.
        assert!(
            out.contains("  crates/x/Cargo.toml (rust, 1 dep(s)):\n"),
            "got: {out}"
        );
    }

    #[test]
    fn every_non_dependency_section_is_identical_in_both_modes() {
        let status = sample_status();
        let summary = index_status(&status, false);
        let verbose = index_status(&status, true);
        let head = |text: &str| {
            text.find("Dependencies")
                .map(|idx| text[..idx].to_string())
                .unwrap_or_default()
        };
        assert_eq!(head(&summary), head(&verbose));
        assert_eq!(
            head(&summary),
            "2 files indexed, 10 symbols total.\n\
             Last indexed at unix timestamp 1700000000.\n\
             Coverage by language:\n\
             \x20 rust: 2 files, 10 symbols\n\
             Languages seen but not yet supported: lua\n"
        );
    }

    #[test]
    fn a_status_with_no_manifests_renders_no_dependency_section_at_all() {
        let mut status = sample_status();
        status.dependencies.clear();
        assert_eq!(index_status(&status, false), index_status(&status, true));
        assert!(!index_status(&status, false).contains("Dependencies"));
    }
}

#[cfg(test)]
mod overview_tests {
    use super::*;
    use crate::server::ModuleDigest;

    fn symbol(name: &str, line: u32, end_line: Option<u32>) -> SymbolListEntry {
        SymbolListEntry {
            name: name.to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            relative_path: "src/lib.rs".to_string(),
            line,
            end_line,
            parent: None,
            level: None,
        }
    }

    fn module(path: &str, symbols: Vec<SymbolListEntry>) -> ModuleDigest {
        ModuleDigest {
            relative_path: path.to_string(),
            symbols,
            omitted: 0,
            relations: Vec::new(),
        }
    }

    #[test]
    fn an_overview_with_no_empty_modules_is_byte_identical_to_the_pre_budget_rendering() {
        let modules = [module("src/lib.rs", vec![symbol("compute", 10, Some(12))])];
        assert_eq!(
            overview("src", &modules, 8),
            "Project overview of `src` (1 module(s), up to 8 symbol(s) each):\n\
             \n\
             src/lib.rs:\n\
             \x20 [function] compute L10-L12\n"
        );
    }

    #[test]
    fn modules_with_nothing_to_surface_are_counted_in_the_header_not_printed() {
        let modules = [
            module("docs/00-index.md", Vec::new()),
            module("docs/glossary.md", Vec::new()),
            module("src/lib.rs", vec![symbol("compute", 10, Some(12))]),
        ];
        let out = overview("docs", &modules, 8);
        assert_eq!(
            out,
            "Project overview of `docs` (3 module(s), up to 8 symbol(s) each, 2 with no indexed top-level symbols omitted):\n\
             \n\
             src/lib.rs:\n\
             \x20 [function] compute L10-L12\n"
        );
        assert!(
            !out.contains("no top-level symbols)"),
            "noise line must be gone: {out}"
        );
        assert!(
            !out.contains("docs/glossary.md"),
            "empty module must not be listed: {out}"
        );
    }

    #[test]
    fn an_overview_over_budget_drops_whole_modules_and_reports_how_many() {
        let modules: Vec<ModuleDigest> = (0..400)
            .map(|i| ModuleDigest {
                relative_path: format!("crates/some-crate/src/nested/module_{i:04}.rs"),
                symbols: (0..4)
                    .map(|j| symbol(&format!("function_number_{i:04}_{j}"), 1, Some(9)))
                    .collect(),
                omitted: 0,
                relations: Vec::new(),
            })
            .collect();
        let out = overview("crates", &modules, 8);
        let body = out
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or_default();

        assert!(
            body.len() <= DEFAULT_BYTE_BUDGET,
            "got {} bytes",
            body.len()
        );
        assert!(out.contains("response budget"), "got: {out}");
        // Whole modules only: every module header present must be followed
        // by its full set of 4 symbol lines.
        let kept = body.matches("crates/some-crate/src/nested/module_").count();
        assert!(
            kept > 0 && kept < 400,
            "expected a partial result, got {kept}"
        );
        assert_eq!(
            body.matches("  [function] function_number_").count(),
            kept * 4
        );
        assert!(
            out.contains(&format!(
                ", {kept} of 400 shown — {} more dropped",
                400 - kept
            )),
            "got header: {}",
            out.lines().next().unwrap_or_default()
        );
    }
}

#[cfg(test)]
mod dead_code_tests {
    use super::*;

    fn entry(name: &str, kind: &str, path: &str, line: u32) -> SymbolListEntry {
        SymbolListEntry {
            name: name.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            relative_path: path.to_string(),
            line,
            end_line: None,
            parent: None,
            level: None,
        }
    }

    #[test]
    fn no_candidates_says_so_instead_of_an_empty_body() {
        assert_eq!(
            find_dead_code("src", &[], 0, 50),
            "No dead-code candidates found under `src`."
        );
    }

    #[test]
    fn groups_consecutive_hits_by_file_with_one_heading_each() {
        let hits = [
            entry("unused_a", "function", "src/lib.rs", 3),
            entry("UnusedB", "struct", "src/lib.rs", 20),
            entry("unused_c", "function", "src/other.rs", 5),
        ];
        let out = find_dead_code("src", &hits, 0, 50);
        assert_eq!(out.matches("src/lib.rs:\n").count(), 1);
        assert_eq!(out.matches("src/other.rs:\n").count(), 1);
        assert!(out.contains("  function unused_a L3\n"));
        assert!(out.contains("  struct UnusedB L20\n"));
        assert!(out.contains("  function unused_c L5\n"));
        assert!(out.contains("3 dead-code candidate(s)"));
        assert!(out.contains("heuristic"));
    }

    #[test]
    fn pagination_note_matches_the_other_list_tools() {
        let hits: Vec<SymbolListEntry> = (0..5)
            .map(|i| entry(&format!("unused_{i}"), "function", "src/lib.rs", i + 1))
            .collect();
        let out = find_dead_code("src", &hits, 0, 2);
        assert!(out.contains("(showing 2, 3 omitted"), "got: {out}");
    }
}

#[cfg(test)]
mod context_pack_tests {
    use super::{leading_comment_start, signature_line};

    #[test]
    fn doc_comments_and_attributes_directly_above_are_included() {
        let source = "use x;\n\n/// Does a thing.\n/// Carefully.\n#[inline]\npub fn thing() {}\n";
        assert_eq!(leading_comment_start(source, 6), 3);
    }

    #[test]
    fn a_blank_line_ends_the_comment_block() {
        let source = "// file header\n\nfn thing() {}\n";
        assert_eq!(leading_comment_start(source, 3), 3);
    }

    #[test]
    fn other_languages_comment_and_decorator_markers_count_too() {
        let source = "# Loads it.\n@cache\ndef load():\n    pass\n";
        assert_eq!(leading_comment_start(source, 3), 1);
        let source = "/**\n * Loads it.\n */\n[Obsolete]\npublic void Load() {}\n";
        assert_eq!(leading_comment_start(source, 5), 1);
    }

    #[test]
    fn a_first_line_or_out_of_range_definition_never_panics() {
        assert_eq!(leading_comment_start("fn a() {}\n", 1), 1);
        assert_eq!(leading_comment_start("fn a() {}\n", 0), 1);
        assert_eq!(leading_comment_start("", 9), 9);
        assert_eq!(signature_line("fn a() {}\n", 5), None);
    }

    #[test]
    fn a_long_signature_is_cut_on_a_char_boundary() {
        let source = format!("fn {}() {{}}\n", "é".repeat(200));
        let signature = signature_line(&source, 1).unwrap_or_default();
        assert!(signature.ends_with('…'), "{signature}");
        assert_eq!(signature.chars().count(), 121);
    }
}
