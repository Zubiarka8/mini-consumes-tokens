use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::Site;

const PAGE_SIZE: usize = 100;
const GROUP_PAGE_SIZE: usize = 20;

pub(super) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    pub location: Option<String>,
}

impl Response {
    pub fn ok(content_type: &'static str, body: String) -> Self {
        Self {
            status: 200,
            location: None,
            content_type,
            body,
        }
    }

    pub fn html(body: String) -> Self {
        Self::ok("text/html; charset=utf-8", body)
    }

    pub fn error(status: u16) -> Self {
        Self {
            status,
            location: None,
            content_type: "text/plain; charset=utf-8",
            body: format!("Request rejected ({status}).\n"),
        }
    }
}

pub(super) fn escape(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

fn href(route: &str, params: &[(&str, &str)]) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().copied())
        .finish();
    escape(&format!("{route}?{query}"))
}

fn link(route: &str, path: &str, label: &str) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        href(route, &[("path", path)]),
        escape(label)
    )
}

fn page(title: &str, site: &Site, body: &str) -> Response {
    let warning = site
        .warning
        .as_deref()
        .map(|warning| format!("<p class=\"warning\">{}</p>", escape(warning)))
        .unwrap_or_default();
    Response::html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title} · {project}</title><link rel="stylesheet" href="style.css"></head><body><header><a class="brand" href="./">MCT / {project}</a><nav aria-label="Project views"><a href="./">Overview</a><a href="atlas">Files</a><a href="duplicates">Duplicates</a></nav></header><main>{warning}<h1>{title}</h1>{body}</main><footer>Snapshot taken at Unix time {time}. After reindexing, restart <code>mct-cli web</code> to refresh.</footer></body></html>"#,
        title = escape(title),
        project = escape(&site.project),
        time = site.generated
    ))
}

fn text<'a>(params: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    params.get(key).map_or("", String::as_str)
}

/// Case-insensitive substring test; `query` must already be lowercase.
fn contains(haystack: &str, query: &str) -> bool {
    haystack.to_lowercase().contains(query)
}

/// Query string that restores the duplicate list filters after a review action.
pub(super) fn filter_query(path: &str, q: &str, view: &str) -> String {
    let fields = [("path", path), ("q", q), ("view", view)];
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.into_iter().filter(|(_, value)| !value.is_empty()))
        .finish()
}

/// Where a review action returns to: the list the user was filtering, else the plain list.
pub(super) fn review_location(back: &str, restored: bool) -> String {
    let (mut path, mut q, mut view) = (String::new(), String::new(), String::new());
    for (key, value) in form_urlencoded::parse(back.as_bytes()) {
        match key.as_ref() {
            "path" => path = value.into_owned(),
            "q" => q = value.into_owned(),
            "view" if matches!(value.as_ref(), "active" | "discarded") => view = value.into_owned(),
            _ => {}
        }
    }
    if restored && view.is_empty() {
        view = "discarded".to_string();
    }
    match filter_query(&path, &q, &view) {
        query if query.is_empty() => "duplicates".to_string(),
        query => format!("duplicates?{query}"),
    }
}

fn number(params: &BTreeMap<String, String>, key: &str) -> Result<usize, ()> {
    params
        .get(key)
        .map_or(Ok(0), |value| value.parse().map_err(|_| ()))
}

fn pager(
    route: &str,
    params: &[(&str, &str)],
    key: &str,
    requested: usize,
    total: usize,
    size: usize,
) -> (usize, String) {
    let pages = total.div_ceil(size).max(1);
    let current = requested.min(pages - 1);
    let mut out = format!(
        "<p class=\"pagination\">Page {} of {pages} · {total} items",
        current + 1
    );
    for (label, next) in [
        ("Previous", current.checked_sub(1)),
        ("Next", (current + 1 < pages).then_some(current + 1)),
    ] {
        if let Some(next) = next {
            let next = next.to_string();
            let mut fields = params.to_vec();
            fields.push((key, &next));
            let _ = write!(out, " · <a href=\"{}\">{label}</a>", href(route, &fields));
        }
    }
    out.push_str("</p>");
    (current * size, out)
}

pub(super) fn route(site: &Site, target: &str) -> Response {
    let (route, query) = target.split_once('?').unwrap_or((target, ""));
    let mut params = BTreeMap::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Response::error(400);
        }
    }
    if ["page", "sp", "ip", "rp", "id"]
        .iter()
        .any(|key| number(&params, key).is_err())
    {
        return Response::error(400);
    }
    match route {
        "" => overview(site),
        "style.css" => Response::ok(
            "text/css; charset=utf-8",
            include_str!("style.css").to_string(),
        ),
        "atlas" => atlas(site, &params),
        "file" => file(site, &params),
        "duplicates" => duplicates(site, &params),
        "group" => group(site, &params),
        "duplicates.toon" => Response::ok(
            "text/plain; charset=utf-8",
            crate::duplicates::render_toon(&site.duplicates),
        ),
        _ => Response::error(404),
    }
}

fn overview(site: &Site) -> Response {
    let body = format!(
        r#"<p class="intro">Browse the indexed files of this project, or review code that appears more than once.</p><section class="cards"><a class="card" href="atlas"><span class="eyebrow">FILES</span><strong>{} files</strong><span>Folders, symbols and imports of every file →</span></a><a class="card" href="duplicates"><span class="eyebrow">DUPLICATES</span><strong>{} groups to review</strong><span>Similar code found in several places →</span></a></section><section><h2>Obsidian vault</h2><p>The same index also writes a notes vault to <code>.mct-index/obsidian/</code>. Open that folder as a vault in Obsidian. File bodies are fetched on demand through the context tools.</p></section>"#,
        site.navigation.files.len(),
        site.active_total()
    );
    page("Project explorer", site, &body)
}

fn search_form(
    route: &str,
    q: &str,
    hidden: &[(&str, &str)],
    label: &str,
    placeholder: &str,
) -> String {
    let mut out = format!(
        r#"<form action="{route}" method="get"><label for="q">{label}</label><div class="search"><input id="q" name="q" value="{}" type="search" placeholder="{}"><button>Search</button></div>"#,
        escape(q),
        escape(placeholder)
    );
    for (key, value) in hidden {
        let _ = write!(
            out,
            "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
            escape(key),
            escape(value)
        );
    }
    out.push_str("</form>");
    out
}

struct Entry {
    path: String,
    label: String,
    directory: bool,
    note: String,
}

/// Immediate children of `directory`: folders first, then files.
fn browse_entries(site: &Site, directory: &str) -> Vec<Entry> {
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let mut children: BTreeMap<&str, bool> = BTreeMap::new();
    for file in &site.navigation.files {
        let Some(rest) = file.relative_path.strip_prefix(&prefix) else {
            continue;
        };
        match rest.split_once('/') {
            Some((name, _)) => children.insert(name, true),
            None => children.insert(rest, false),
        };
    }
    let mut entries: Vec<_> = children
        .into_iter()
        .map(|(name, directory)| {
            let path = format!("{prefix}{name}");
            let note = if directory {
                let files = site
                    .navigation
                    .files
                    .iter()
                    .filter(|file| file.relative_path.starts_with(&format!("{path}/")))
                    .count();
                format!("folder · {files} files")
            } else {
                file_note(site, &path)
            };
            Entry {
                path,
                label: name.to_string(),
                directory,
                note,
            }
        })
        .collect();
    entries.sort_by_key(|entry| !entry.directory);
    entries
}

fn file_note(site: &Site, path: &str) -> String {
    format!(
        "{} symbols · {} duplicate groups",
        site.symbols.get(path).map_or(0, Vec::len),
        site.active_groups(path)
    )
}

/// Files under `directory` whose path or symbol names contain `query` (lowercase).
fn search_entries(site: &Site, directory: &str, query: &str) -> Vec<Entry> {
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    site.navigation
        .files
        .iter()
        .filter(|file| file.relative_path.starts_with(&prefix))
        .filter_map(|file| {
            let path = &file.relative_path;
            let mut names: Vec<&str> = site
                .symbols
                .get(path)
                .into_iter()
                .flatten()
                .map(|id| site.navigation.symbols[*id].name.as_str())
                .filter(|name| contains(name, query))
                .collect();
            names.dedup();
            if names.is_empty() && !contains(path, query) {
                return None;
            }
            let matched = if names.is_empty() {
                String::new()
            } else {
                let shown: Vec<_> = names.iter().take(3).copied().collect();
                let more = names.len() - shown.len();
                let more = if more > 0 {
                    format!(" +{more}")
                } else {
                    String::new()
                };
                format!("matches {}{more} · ", shown.join(", "))
            };
            Some(Entry {
                path: path.clone(),
                label: path.clone(),
                directory: false,
                note: format!("{matched}{}", file_note(site, path)),
            })
        })
        .collect()
}

fn breadcrumbs(directory: &str, q: &str) -> String {
    let mut out = format!(
        "<nav class=\"crumbs\" aria-label=\"Folder\"><a href=\"{}\">Project root</a>",
        href("atlas", &[("q", q)])
    );
    let mut path = String::new();
    for part in directory.split('/').filter(|part| !part.is_empty()) {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(part);
        let _ = write!(
            out,
            " / <a href=\"{}\">{}</a>",
            href("atlas", &[("dir", &path), ("q", q)]),
            escape(part)
        );
    }
    out.push_str("</nav>");
    out
}

fn atlas(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let q = text(params, "q").trim();
    let directory = text(params, "dir");
    let query = q.to_lowercase();
    let scope = if directory.is_empty() {
        "the whole project".to_string()
    } else {
        format!("<code>{}</code>", escape(directory))
    };
    let mut body = format!(
        "<p class=\"hint\">Browse folders, or search to list every file whose path or symbol (function, class…) name contains your text. Searching covers {scope}.</p>"
    );
    body.push_str(&search_form(
        "atlas",
        q,
        &[("dir", directory)],
        "Search files and symbols",
        "e.g. parse_request or src/web",
    ));
    body.push_str(&breadcrumbs(directory, q));
    let entries = if query.is_empty() {
        browse_entries(site, directory)
    } else {
        search_entries(site, directory, &query)
    };
    if !query.is_empty() {
        let _ = write!(
            body,
            "<p class=\"result-count\">{} matching files for “{}” in {scope}. <a href=\"{}\">Clear search</a>",
            entries.len(),
            escape(q),
            href("atlas", &[("dir", directory)])
        );
        if !directory.is_empty() {
            let _ = write!(
                body,
                " · <a href=\"{}\">Search the whole project</a>",
                href("atlas", &[("q", q)])
            );
        }
        body.push_str("</p>");
    }
    body.push_str("<ul class=\"entries\">");
    let (start, pagination) = pager(
        "atlas",
        &[("dir", directory), ("q", q)],
        "page",
        number(params, "page").unwrap_or(0),
        entries.len(),
        PAGE_SIZE,
    );
    for entry in entries.iter().skip(start).take(PAGE_SIZE) {
        let name = escape(&entry.label);
        let _ = if entry.directory {
            write!(
                body,
                "<li><a href=\"{}\">{name}/</a><span>{}</span></li>",
                href("atlas", &[("dir", &entry.path), ("q", q)]),
                escape(&entry.note)
            )
        } else {
            write!(
                body,
                "<li>{}<span>{}</span></li>",
                link("file", &entry.path, &entry.label),
                escape(&entry.note)
            )
        };
    }
    if entries.is_empty() {
        body.push_str("<li>No matching entries.</li>");
    }
    body.push_str("</ul>");
    body.push_str(&pagination);
    page("Project files", site, &body)
}

fn file(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let path = text(params, "path");
    let Ok(id) = site
        .navigation
        .files
        .binary_search_by(|file| file.relative_path.as_str().cmp(path))
    else {
        return Response::error(404);
    };
    let file = &site.navigation.files[id];
    let directory = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let count = site.active_groups(path);
    let mut body = format!("<p><a href=\"{}\">← Back to folder</a> · {} ({count})</p><dl><dt>Path</dt><dd><code>{}</code></dd><dt>Language</dt><dd>{}</dd><dt>Content hash</dt><dd><code>{}</code></dd></dl><p>Metadata only. Retrieve source snippets at these exact locations with the MCP context tools.</p><h2>Symbols</h2><ul>",
        href("atlas", &[("dir", directory)]), link("duplicates", path, "Duplicate groups containing this file"), escape(path), escape(&file.language), escape(&file.content_hash));
    let ids = site
        .symbols
        .get(path)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let ip = number(params, "ip").unwrap_or(0).to_string();
    let rp = number(params, "rp").unwrap_or(0).to_string();
    let sp = number(params, "sp").unwrap_or(0).to_string();
    let (start, pagination) = pager(
        "file",
        &[("path", path), ("ip", &ip), ("rp", &rp)],
        "sp",
        number(params, "sp").unwrap_or(0),
        ids.len(),
        PAGE_SIZE,
    );
    for id in ids.iter().skip(start).take(PAGE_SIZE) {
        let symbol = &site.navigation.symbols[*id];
        let _ = write!(
            body,
            "<li id=\"symbol-{}\"><code>{}</code> <span class=\"muted\">{} · {}:{}{}</span></li>",
            id,
            escape(&symbol.name),
            escape(&symbol.kind),
            escape(path),
            symbol.line,
            symbol
                .end_line
                .map(|line| format!("–{line}"))
                .unwrap_or_default()
        );
    }
    body.push_str("</ul>");
    body.push_str(&pagination);
    for (title, key, paths) in [
        ("Resolved imports", "ip", &site.imports),
        ("Imported by", "rp", &site.importers),
    ] {
        let items = paths.get(path).map(Vec::as_slice).unwrap_or_default();
        let other = if key == "ip" {
            ("rp", rp.as_str())
        } else {
            ("ip", ip.as_str())
        };
        let (start, pagination) = pager(
            "file",
            &[("path", path), ("sp", &sp), other],
            key,
            number(params, key).unwrap_or(0),
            items.len(),
            PAGE_SIZE,
        );
        let _ = write!(body, "<h2>{title}</h2><ul>");
        for target in items.iter().skip(start).take(PAGE_SIZE) {
            let _ = write!(body, "<li>{}</li>", link("file", target, target));
        }
        body.push_str("</ul>");
        body.push_str(&pagination);
    }
    page(path, site, &body)
}

fn diagnostics(site: &Site) -> String {
    let report = &site.duplicates;
    let mut out = String::new();
    if !report.complete || !report.has_indexing_record {
        out.push_str("<p class=\"warning\">Partial or unverified analysis: some candidates may be missing. See the complete TOON report for skipped candidates and indexing state.</p>");
    }
    let _ = write!(out, "<details><summary>How these groups were found</summary><p>Groups are candidates for human review, not proof of redundant or equivalent behavior. Policy: {} · minimum {} lines / {} tokens. {} candidates analyzed; {} skipped. Index diagnostics: {} syntax errors / {} read failures.</p>",
        report.options.normalization.as_str(), report.options.min_lines, report.options.min_tokens,
        report.candidates_analyzed, report.skipped.len(), report.index_syntax_errors, report.index_read_failures);
    if !report.unsupported_languages.is_empty() {
        let _ = write!(
            out,
            "<p>Languages without duplicate token support: {}.</p>",
            escape(&report.unsupported_languages.join(", "))
        );
    }
    out.push_str("<p><a href=\"duplicates.toon\" download=\"duplicates.toon\">Download full raw analysis (TOON, includes discarded groups)</a></p></details>");
    out
}

fn group_stats(group: &mct_index::DuplicateGroup) -> String {
    format!(
        "{} · {} members · {} lines · {} tokens",
        escape(&group.language),
        group.members.len(),
        group.line_count,
        group.token_count
    )
}

fn duplicates(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let path = text(params, "path");
    let q = text(params, "q").trim();
    let view = params.get("view").map_or("active", String::as_str);
    if !matches!(view, "active" | "discarded") {
        return Response::error(400);
    }
    let discarded = view == "discarded";
    let query = q.to_lowercase();
    let filtered: Vec<_> = site
        .duplicates
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| {
            (path.is_empty() || group.members.iter().any(|member| member.path == path))
                && (query.is_empty()
                    || group.members.iter().any(|member| {
                        contains(&member.path, &query) || contains(&member.symbol_name, &query)
                    }))
        })
        .collect();
    let dismissed = filtered
        .iter()
        .filter(|(id, _)| site.dismissed(*id))
        .count();
    let total = filtered.len();
    let matches: Vec<_> = filtered
        .into_iter()
        .filter(|(id, _)| site.dismissed(*id) == discarded)
        .collect();
    let tab = |label: &str, target: &str, count: usize| {
        let current = if target == view {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!(
            "<a class=\"tab\" href=\"{}\"{current}>{label} ({count})</a>",
            href("duplicates", &[("path", path), ("q", q), ("view", target)])
        )
    };
    let mut body = format!(
        "<p class=\"hint\">Each group is code that appears in several places. Review it, then discard it if it is not a real problem; you can restore it later. Changed code is reviewed again.</p><nav class=\"tabs\" aria-label=\"Review status\">{}{}</nav>",
        tab("To review", "active", total - dismissed),
        tab("Discarded", "discarded", dismissed)
    );
    body.push_str(&search_form(
        "duplicates",
        q,
        &[("path", path), ("view", view)],
        "Filter groups by file path or symbol name",
        "e.g. parse or src/web",
    ));
    if !path.is_empty() || !q.is_empty() {
        body.push_str("<p class=\"result-count\">Filters:");
        if !path.is_empty() {
            let _ = write!(
                body,
                " file {} <a href=\"{}\">(remove)</a>",
                link("file", path, path),
                href("duplicates", &[("q", q), ("view", view)])
            );
        }
        if !q.is_empty() {
            let _ = write!(
                body,
                " text “{}” <a href=\"{}\">(remove)</a>",
                escape(q),
                href("duplicates", &[("path", path), ("view", view)])
            );
        }
        let _ = write!(
            body,
            " · <a href=\"{}\">Clear all filters</a></p>",
            href("duplicates", &[("view", view)])
        );
    }
    body.push_str(&diagnostics(site));
    let back = filter_query(path, q, view);
    let (start, pagination) = pager(
        "duplicates",
        &[("path", path), ("q", q), ("view", view)],
        "page",
        number(params, "page").unwrap_or(0),
        matches.len(),
        GROUP_PAGE_SIZE,
    );
    for (id, group) in matches.iter().skip(start).take(GROUP_PAGE_SIZE) {
        let _ = write!(body, "<article><h2><a href=\"group?id={id}\">Group {}</a> <span class=\"badge\">{}</span></h2><p>{}</p><ul>", id + 1, escape(group.verdict.level.as_str()), group_stats(group));
        for member in group.members.iter().take(6) {
            let _ = write!(
                body,
                "<li>{} · <code>{}</code></li>",
                link(
                    "file",
                    &member.path,
                    &format!("{}:{}–{}", member.path, member.start_line, member.end_line)
                ),
                escape(&member.symbol_name)
            );
        }
        body.push_str("</ul>");
        if group.members.len() > 6 {
            let _ = write!(
                body,
                "<p>{} more members on the group page.</p>",
                group.members.len() - 6
            );
        }
        body.push_str(&review_form(site, *id, &back));
        body.push_str("</article>");
    }
    if matches.is_empty() {
        body.push_str("<p>No matching duplicate groups.</p>");
    }
    body.push_str(&pagination);
    page(
        if discarded {
            "Discarded duplicate groups"
        } else {
            "Duplicate groups to review"
        },
        site,
        &body,
    )
}

/// `back` is the filter query of the list the form sits on; empty on the group page.
fn review_form(site: &Site, id: usize, back: &str) -> String {
    let key = &site.review_keys[id];
    let controls = if let Some(reason) = site.reviews.reason(key) {
        format!("<p class=\"muted\">Review decision: {}</p><button name=\"action\" value=\"restore\">Restore to review</button>", reason.label())
    } else {
        format!("<label for=\"review-reason-{id}\">Discard reason</label><select id=\"review-reason-{id}\" name=\"action\"><option value=\"false-positive\">False positive</option><option value=\"dismissed\">Other / not relevant</option></select><button>Discard group</button>")
    };
    let back = if back.is_empty() {
        String::new()
    } else {
        format!(
            "<input type=\"hidden\" name=\"back\" value=\"{}\">",
            escape(back)
        )
    };
    format!("<form action=\"review\" method=\"post\" class=\"review\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"key\" value=\"{}\">{back}{controls}</form>", escape(&site.csrf), escape(key))
}

fn group(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let id = number(params, "id").unwrap_or(0);
    let Some(group) = site.duplicates.groups.get(id) else {
        return Response::error(404);
    };
    let mut body = format!("<p><a href=\"duplicates\">← All groups</a></p><p>{}</p><h2>Review signals</h2><p>Verdict: <strong>{}</strong></p><ul>", group_stats(group), escape(group.verdict.level.as_str()));
    for reason in &group.verdict.reasons {
        let _ = write!(body, "<li>{}</li>", escape(reason.as_str()));
    }
    let _ = write!(
        body,
        "</ul><p>Priority: {} · {}</p><h2>Members</h2>",
        escape(group.priority.level.as_str()),
        escape(&group.priority.reason)
    );
    body.push_str(&review_form(site, id, ""));
    let id_text = id.to_string();
    let (start, pagination) = pager(
        "group",
        &[("id", &id_text)],
        "page",
        number(params, "page").unwrap_or(0),
        group.members.len(),
        PAGE_SIZE,
    );
    for member in group.members.iter().skip(start).take(PAGE_SIZE) {
        let _ = write!(
            body,
            "<article><h3>{}</h3><p>{} · {}:{}</p><p>{}</p><pre>{}</pre></article>",
            escape(&member.symbol_name),
            escape(&member.symbol_kind),
            escape(&member.path),
            member.start_line,
            link("file", &member.path, "Open in atlas"),
            escape(&member.excerpt.join("\n"))
        );
    }
    body.push_str(&pagination);
    body.push_str("<details><summary>Full alignment, subgroups and analysis (TOON)</summary><pre>");
    // Reuse the established renderer instead of inventing a second evidence schema.
    let mut evidence = String::new();
    crate::duplicates::render_group(
        &mut evidence,
        id + 1,
        group,
        site.duplicates.options.normalization,
    );
    body.push_str(&escape(&evidence));
    body.push_str("</pre></details>");
    page(&format!("Duplicate group {}", id + 1), site, &body)
}
