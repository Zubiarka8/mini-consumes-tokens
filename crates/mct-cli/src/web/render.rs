use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::Site;

const PAGE_SIZE: usize = 100;
const GROUP_PAGE_SIZE: usize = 20;

pub(super) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    pub location: Option<&'static str>,
}

impl Response {
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
    Response {
        status: 200,
        location: None,
        content_type: "text/html; charset=utf-8",
        body: format!(
            r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title} · {project}</title><link rel="stylesheet" href="style.css"></head><body><header><a class="brand" href="./">MCT / {project}</a><nav aria-label="Project views"><a href="./">Overview</a><a href="atlas">Atlas</a><a href="duplicates">Duplicates</a></nav></header><main>{warning}<h1>{title}</h1>{body}</main><footer>Local snapshot · {time} (Unix seconds). Reindex and restart this command to refresh. SQLite supplies navigation; duplicate analysis checks current source files.</footer></body></html>"#,
            title = escape(title),
            project = escape(&site.project),
            time = site.generated
        ),
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
        "style.css" => Response {
            status: 200,
            location: None,
            content_type: "text/css; charset=utf-8",
            body: include_str!("style.css").to_string(),
        },
        "atlas" => atlas(site, &params),
        "file" => file(site, &params),
        "duplicates" => duplicates(site, &params),
        "group" => group(site, &params),
        "duplicates.toon" => Response {
            status: 200,
            location: None,
            content_type: "text/plain; charset=utf-8",
            body: crate::duplicates::render_toon(&site.duplicates),
        },
        _ => Response::error(404),
    }
}

fn overview(site: &Site) -> Response {
    let mut body = format!(
        r#"<p class="intro">Explore the project, then follow a file into its duplicate candidates.</p><section class="cards"><a class="card" href="atlas"><span class="eyebrow">PROJECT MAP</span><strong>{} files</strong><span>Directories, symbols and resolved imports →</span></a><a class="card" href="duplicates"><span class="eyebrow">CODE REVIEW</span><strong>{} active duplicate groups</strong><span>Candidate groups, verdicts and source locations →</span></a></section><section><h2>Obsidian vault</h2><p>The same index supplies the metadata atlas in <code>.mct-index/obsidian/</code>. Open that folder as a vault in Obsidian. File bodies are retrieved on demand through the existing context tools.</p></section>"#,
        site.navigation.files.len(),
        site.duplicates
            .groups
            .iter()
            .enumerate()
            .filter(|(id, _)| !site.dismissed(*id))
            .count()
    );
    diagnostics(site, &mut body);
    page("Project explorer", site, &body)
}

fn search_form(route: &str, q: &str, hidden: &[(&str, &str)], label: &str) -> String {
    let mut out = format!(
        r#"<form action="{route}" method="get"><label for="q">{label}</label><div class="search"><input id="q" name="q" value="{}" type="search"><button>Search</button></div>"#,
        escape(q)
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

fn atlas(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let q = params.get("q").map(String::as_str).unwrap_or("");
    let directory = params.get("dir").map(String::as_str).unwrap_or("");
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let query = q.to_lowercase();
    // Group the immediate children once; paging counts directories and files.
    let mut children: BTreeMap<String, bool> = BTreeMap::new();
    for file in &site.navigation.files {
        let Some(rest) = file.relative_path.strip_prefix(&prefix) else {
            continue;
        };
        let matches = query.is_empty()
            || file.relative_path.to_lowercase().contains(&query)
            || site
                .symbols
                .get(&file.relative_path)
                .into_iter()
                .flatten()
                .any(|id| {
                    site.navigation.symbols[*id]
                        .name
                        .to_lowercase()
                        .contains(&query)
                });
        if !matches {
            continue;
        }
        let (name, is_dir) = rest
            .split_once('/')
            .map_or((rest, false), |(dir, _)| (dir, true));
        children.insert(format!("{prefix}{name}"), is_dir);
    }
    let mut body = search_form(
        "atlas",
        q,
        &[("dir", directory)],
        "Filter paths and indexed symbol names",
    );
    let _ = write!(
        body,
        "<h2>{}</h2><p><a href=\"atlas\">Project root</a>",
        if directory.is_empty() {
            "/".to_string()
        } else {
            escape(directory)
        }
    );
    if !directory.is_empty() {
        let parent = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
        let _ = write!(
            body,
            " · <a href=\"{}\">Parent directory</a>",
            href("atlas", &[("dir", parent), ("q", q)])
        );
    }
    body.push_str("</p><ul class=\"entries\">");
    let (start, pagination) = pager(
        "atlas",
        &[("dir", directory), ("q", q)],
        "page",
        number(params, "page").unwrap_or(0),
        children.len(),
        PAGE_SIZE,
    );
    for (path, directory) in children.iter().skip(start).take(PAGE_SIZE) {
        if *directory {
            let _ = write!(
                body,
                "<li><a href=\"{}\">{}/</a><span>directory</span></li>",
                href("atlas", &[("dir", path), ("q", q)]),
                escape(path)
            );
        } else {
            let count = site.symbols.get(path).map_or(0, Vec::len);
            let groups = site.active_groups(path);
            let _ = write!(
                body,
                "<li>{}<span>{count} symbols · {groups} groups</span></li>",
                link("file", path, path)
            );
        }
    }
    if children.is_empty() {
        body.push_str("<li>No matching entries.</li>");
    }
    body.push_str("</ul>");
    body.push_str(&pagination);
    page("Code atlas", site, &body)
}

fn file(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let path = params.get("path").map(String::as_str).unwrap_or("");
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
    let mut body = format!("<p class=\"actions\"><a href=\"{}\">Directory map</a> · {} ({count})</p><dl><dt>Path</dt><dd><code>{}</code></dd><dt>Language</dt><dd>{}</dd><dt>Content hash</dt><dd><code>{}</code></dd></dl><p>Metadata only. Retrieve source snippets at these exact locations with the MCP context tools.</p><h2>Symbols</h2><ul>",
        href("atlas", &[("dir", directory)]), link("duplicates", path, "Duplicate groups"), escape(path), escape(&file.language), escape(&file.content_hash));
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

fn diagnostics(site: &Site, body: &mut String) {
    let report = &site.duplicates;
    let _ = write!(body, "<p class=\"notice\">Duplicate groups are candidates for human review, not proof of redundant or equivalent behavior. Policy: {} · minimum {} lines / {} tokens. {} candidates analyzed; {} skipped. Index diagnostics: {} syntax errors / {} read failures.</p>",
        report.options.normalization.as_str(), report.options.min_lines, report.options.min_tokens,
        report.candidates_analyzed, report.skipped.len(), report.index_syntax_errors, report.index_read_failures);
    if !report.complete || !report.has_indexing_record {
        body.push_str("<p class=\"warning\">Partial or unverified analysis: some candidates may be missing. See the complete TOON report for skipped candidates and indexing state.</p>");
    }
    if !report.unsupported_languages.is_empty() {
        let _ = write!(
            body,
            "<p>Languages without duplicate token support: {}.</p>",
            escape(&report.unsupported_languages.join(", "))
        );
    }
    body.push_str("<p><a href=\"duplicates.toon\" download=\"duplicates.toon\">Download full raw duplicate analysis (TOON, includes dismissed groups)</a></p>");
}

fn duplicates(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let path = params.get("path").map(String::as_str).unwrap_or("");
    let q = params.get("q").map(String::as_str).unwrap_or("");
    let view = params.get("view").map(String::as_str).unwrap_or("active");
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
                        member.path.to_lowercase().contains(&query)
                            || member.symbol_name.to_lowercase().contains(&query)
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
    let mut body = format!("<nav aria-label=\"Review status\"><a href=\"{}\">Active ({})</a><a href=\"{}\">Discarded ({dismissed})</a></nav><p>Dismiss unwanted groups as false positives or for another reason. You can restore them from Discarded. Changed code is reviewed again.</p>",
        href("duplicates", &[("path", path), ("q", q), ("view", "active")]), total - dismissed,
        href("duplicates", &[("path", path), ("q", q), ("view", "discarded")]));
    body.push_str(&search_form(
        "duplicates",
        q,
        &[("path", path), ("view", view)],
        "Filter candidate paths and symbol names",
    ));
    if !path.is_empty() {
        let _ = write!(
            body,
            "<p>File: {} · <a href=\"duplicates\">All groups</a></p>",
            link("file", path, path)
        );
    }
    diagnostics(site, &mut body);
    let (start, pagination) = pager(
        "duplicates",
        &[("path", path), ("q", q), ("view", view)],
        "page",
        number(params, "page").unwrap_or(0),
        matches.len(),
        GROUP_PAGE_SIZE,
    );
    for (id, group) in matches.iter().skip(start).take(GROUP_PAGE_SIZE) {
        let _ = write!(body, "<article><h2><a href=\"group?id={id}\">Group {}</a> <span class=\"badge\">{}</span></h2><p>{} · {} members · {} lines · {} tokens</p><ul>", id + 1, escape(group.verdict.level.as_str()), escape(&group.language), group.members.len(), group.line_count, group.token_count);
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
        body.push_str(&review_form(site, *id));
        body.push_str("</article>");
    }
    if matches.is_empty() {
        body.push_str("<p>No matching duplicate groups.</p>");
    }
    body.push_str(&pagination);
    page(
        if discarded {
            "Discarded duplicate candidates"
        } else {
            "Duplicate candidates"
        },
        site,
        &body,
    )
}

fn review_form(site: &Site, id: usize) -> String {
    let key = &site.review_keys[id];
    let controls = if let Some(reason) = site.reviews.reason(key) {
        format!("<p class=\"muted\">Review decision: {}</p><button name=\"action\" value=\"restore\">Restore to active</button>", reason.label())
    } else {
        format!("<label for=\"review-reason-{id}\">Dismiss reason</label><select id=\"review-reason-{id}\" name=\"action\"><option value=\"false-positive\">False positive</option><option value=\"dismissed\">Other / not relevant</option></select><button>Move to discarded</button>")
    };
    format!("<form action=\"review\" method=\"post\" class=\"review\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"key\" value=\"{}\">{controls}</form>", escape(&site.csrf), escape(key))
}

fn group(site: &Site, params: &BTreeMap<String, String>) -> Response {
    let id = number(params, "id").unwrap_or(0);
    let Some(group) = site.duplicates.groups.get(id) else {
        return Response::error(404);
    };
    let mut body = format!("<p><a href=\"duplicates\">All groups</a></p><p>{} · {} members · {} lines · {} tokens</p><h2>Review signals</h2><p>Verdict: <strong>{}</strong></p><ul>", escape(&group.language), group.members.len(), group.line_count, group.token_count, escape(group.verdict.level.as_str()));
    for reason in &group.verdict.reasons {
        let _ = write!(body, "<li>{}</li>", escape(reason.as_str()));
    }
    let _ = write!(
        body,
        "</ul><p>Priority: {} · {}</p><h2>Members</h2>",
        escape(group.priority.level.as_str()),
        escape(&group.priority.reason)
    );
    body.push_str(&review_form(site, id));
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
