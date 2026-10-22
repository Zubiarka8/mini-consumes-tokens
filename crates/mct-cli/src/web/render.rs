//! Shared page shell, helpers and the route table. Views live in `view_*.rs`.

use std::collections::BTreeMap;
use std::fmt::{Display, Write as _};

use super::i18n::{t, tf, Lang};
use super::reviews::{Kind, State};
use super::{view_dead, view_duplicates, view_obsidian, view_reviews, view_status, Site};

pub(super) const PAGE_SIZE: usize = 100;
pub(super) const GROUP_PAGE_SIZE: usize = 20;

pub(super) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    pub location: Option<String>,
    /// Remember this interface language in the browser (the `mct_lang` cookie).
    pub set_lang: Option<Lang>,
}

impl Response {
    pub fn error(status: u16) -> Self {
        Self {
            status,
            location: None,
            set_lang: None,
            content_type: "text/plain; charset=utf-8",
            body: format!("Request rejected / Solicitud rechazada ({status}).\n"),
        }
    }
    pub fn redirect(location: String) -> Self {
        Self {
            status: 303,
            content_type: "text/plain; charset=utf-8",
            body: String::new(),
            location: Some(location),
            set_lang: None,
        }
    }
    pub fn text(content_type: &'static str, body: String) -> Self {
        Self {
            status: 200,
            location: None,
            set_lang: None,
            content_type,
            body,
        }
    }
}

/// One request: the immutable snapshot, validated query parameters, the
/// interface language and the relative target (the `next` of review forms).
pub(super) struct Req<'a> {
    pub site: &'a Site,
    pub params: BTreeMap<String, String>,
    pub target: &'a str,
    pub lang: Lang,
}

impl Req<'_> {
    pub fn param(&self, key: &str) -> &str {
        self.params.get(key).map_or("", String::as_str)
    }
    pub fn number(&self, key: &str) -> usize {
        self.params
            .get(key)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    }
    pub fn t(&self, spanish: &'static str) -> &'static str {
        t(self.lang, spanish)
    }
    pub fn tf(&self, spanish: &'static str, args: &[&dyn Display]) -> String {
        tf(self.lang, spanish, args)
    }
}

pub(super) fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A relative URL with a query, already HTML-escaped for use in an attribute.
pub(super) fn href(route: &str, params: &[(&str, &str)]) -> String {
    let params: Vec<_> = params
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .collect();
    if params.is_empty() {
        return escape(route);
    }
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|pair| **pair))
        .finish();
    escape(&format!("{route}?{query}"))
}

pub(super) fn link(route: &str, params: &[(&str, &str)], label: &str) -> String {
    format!("<a href=\"{}\">{}</a>", href(route, params), escape(label))
}

/// A redirect target a form may carry: a known route and a tame query.
pub(super) fn safe_next(next: &str) -> Option<String> {
    let (route, query) = next.split_once('?').unwrap_or((next, ""));
    let allowed = [
        "duplicados",
        "duplicado",
        "muerto",
        "simbolo",
        "revisiones",
        "archivo",
        "obsidian",
        "explorar",
        "estado",
        "ajustes",
        "notas",
        "nota",
        "grafo",
    ];
    (allowed.contains(&route)
        && next.len() <= 400
        && query
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"=&%+-_.~".contains(&byte)))
    .then(|| next.to_string())
}

/// `1 700 000 000` as `2023-11-14 22:13 UTC`.
pub(super) fn utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Duplicates,
    Dead,
    Obsidian,
    Reviews,
    Status,
    Settings,
}

pub(super) fn page(req: &Req<'_>, title: &str, active: Section, body: &str) -> Response {
    let site = req.site;
    let counts = site.counts();
    let warning = site
        .warning
        .as_ref()
        .map(|error| {
            format!(
                "<p class=\"warning\" role=\"alert\">{}</p>",
                req.tf(
                    "No se pudo regenerar la bóveda de Obsidian: {0}",
                    &[&escape(error)]
                )
            )
        })
        .unwrap_or_default();
    let item = |section: Section, route: &str, label: &str, count: usize, class: &str| {
        let current = if section == active {
            " aria-current=\"page\""
        } else {
            ""
        };
        let badge = if count > 0 {
            format!(
                " <span class=\"count {class}\" title=\"{}\">{count}</span>",
                req.t("Pendientes")
            )
        } else {
            String::new()
        };
        format!("<a href=\"{route}\"{current}>{label}{badge}</a>")
    };
    let main = [
        item(
            Section::Duplicates,
            "duplicados",
            req.t("Código duplicado"),
            counts.duplicates_pending,
            "",
        ),
        item(
            Section::Dead,
            "muerto",
            req.t("Código muerto"),
            counts.dead_pending,
            "",
        ),
        item(Section::Obsidian, "obsidian", "Obsidian", 0, ""),
        item(
            Section::Reviews,
            "revisiones",
            req.t("Revisiones"),
            counts.reevaluate,
            "count-warn",
        ),
    ]
    .concat();
    let aux = [
        item(Section::Status, "estado", req.t("Estado"), 0, ""),
        item(Section::Settings, "ajustes", req.t("Ajustes"), 0, ""),
    ]
    .concat();
    let next = escape(&safe_next(req.target).unwrap_or_default());
    let mut switch = format!(
        "<form action=\"lang\" method=\"post\" class=\"lang\" aria-label=\"{}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"next\" value=\"{next}\">",
        req.t("Idioma de la interfaz"),
        escape(&site.csrf)
    );
    for (code, label) in [("es", "ES"), ("en", "EN")] {
        let current = if code == req.lang.code() {
            " aria-current=\"true\""
        } else {
            ""
        };
        let _ = write!(
            switch,
            "<button name=\"lang\" value=\"{code}\" class=\"secondary\" lang=\"{code}\"{current}>{label}</button>"
        );
    }
    switch.push_str("</form>");
    Response::text(
        "text/html; charset=utf-8",
        format!(
            r##"<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark light"><title>{title} · {project}</title><link rel="stylesheet" href="style.css"><script src="app.js" defer></script></head><body><a class="skip" href="#contenido">{skip}</a><header class="top"><a class="brand" href="duplicados">MCT <span>{project}</span></a><nav aria-label="{sections}">{main}</nav><nav class="aux" aria-label="{project_label}">{aux}</nav>{switch}</header><main id="contenido">{warning}<h1>{title}</h1>{body}</main><footer>{footer}</footer></body></html>"##,
            lang = req.lang.code(),
            title = escape(title),
            project = escape(&site.project),
            skip = req.t("Saltar al contenido"),
            sections = req.t("Secciones"),
            project_label = req.t("Proyecto"),
            footer = req.tf(
                "Instantánea local de {0}. Tras reindexar, usa «Recargar análisis» en Estado. Las decisiones de revisión se guardan en <code>.mct-index/reviews.toon</code>.",
                &[&utc(site.generated)]
            ),
        ),
    )
}

pub(super) fn pager(
    req: &Req<'_>,
    route: &str,
    params: &[(&str, &str)],
    key: &str,
    total: usize,
    size: usize,
) -> (usize, String) {
    let pages = total.div_ceil(size).max(1);
    let current = req.number(key).min(pages - 1);
    let mut out = format!(
        "<p class=\"pagination\">{}",
        req.tf(
            "Página {0} de {1} · {2} elementos",
            &[&(current + 1), &pages, &total]
        )
    );
    for (label, next) in [
        (req.t("Anterior"), current.checked_sub(1)),
        (
            req.t("Siguiente"),
            (current + 1 < pages).then_some(current + 1),
        ),
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

/// `<select>` with a fixed list of `(value, label)`; `selected` is matched exactly.
pub(super) fn select(
    name: &str,
    label: &str,
    options: &[(&str, String)],
    selected: &str,
) -> String {
    let mut out = format!("<label>{}<select name=\"{}\">", escape(label), escape(name));
    for (value, text) in options {
        let mark = if *value == selected { " selected" } else { "" };
        let _ = write!(
            out,
            "<option value=\"{}\"{mark}>{}</option>",
            escape(value),
            escape(text)
        );
    }
    out.push_str("</select></label>");
    out
}

pub(super) fn text_input(name: &str, label: &str, value: &str, placeholder: &str) -> String {
    format!(
        "<label>{}<input name=\"{}\" value=\"{}\" placeholder=\"{}\" type=\"search\"></label>",
        escape(label),
        escape(name),
        escape(value),
        escape(placeholder)
    )
}

/// Status tab values, in display order.
pub(super) const STATUS_TABS: [&str; 6] = [
    "pending",
    "reevaluate",
    "confirmed",
    "false_positive",
    "ignored",
    "all",
];

fn tab_label(lang: Lang, tab: &str) -> &'static str {
    match tab {
        "pending" => t(lang, "Pendientes"),
        "reevaluate" => t(lang, "Para reevaluar"),
        "confirmed" => t(lang, "Confirmados"),
        "false_positive" => t(lang, "Falsos positivos"),
        "ignored" => t(lang, "Ignorados"),
        _ => t(lang, "Todos"),
    }
}

pub(super) fn status_matches(filter: &str, state: &State) -> bool {
    match (filter, state) {
        ("all", _)
        | ("pending", State::Pending)
        | ("reevaluate", State::NeedsReevaluation { .. }) => true,
        (wanted, State::Decided { status, .. }) => status.value() == wanted,
        _ => false,
    }
}

/// Counts per [`STATUS_TABS`] entry.
pub(super) fn status_counts<'a>(states: impl Iterator<Item = &'a State>) -> [usize; 6] {
    let mut counts = [0; 6];
    for state in states {
        for (slot, tab) in STATUS_TABS.iter().enumerate() {
            if status_matches(tab, state) {
                counts[slot] += 1;
            }
        }
    }
    counts
}

pub(super) fn status_tabs(
    req: &Req<'_>,
    route: &str,
    base: &[(&str, &str)],
    active: &str,
    counts: &[usize; 6],
) -> String {
    let mut out = format!(
        "<nav class=\"tabs\" aria-label=\"{}\">",
        req.t("Estado de la revisión")
    );
    for (slot, tab) in STATUS_TABS.iter().enumerate() {
        let mut params = base.to_vec();
        params.push(("status", tab));
        let current = if *tab == active {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            out,
            "<a href=\"{}\"{current}>{} <span class=\"count\">{}</span></a>",
            href(route, &params),
            tab_label(req.lang, tab),
            counts[slot]
        );
    }
    out.push_str("</nav>");
    out
}

pub(super) fn state_chip(req: &Req<'_>, state: &State) -> String {
    match state {
        State::Pending => format!(
            "<span class=\"chip chip-pending\">{}</span>",
            req.t("Pendiente de revisar")
        ),
        State::Decided { status, at, .. } => format!(
            "<span class=\"chip chip-{}\">{}</span> <span class=\"muted\">{}</span>",
            status.value(),
            status.label(req.lang),
            at.map_or_else(|| req.t("(decisión anterior, sin fecha)").to_string(), utc)
        ),
        State::NeedsReevaluation { status, at, .. } => format!(
            "<span class=\"chip chip-reeval\">{}</span> <span class=\"muted\">{}</span>",
            req.t("Reevaluar"),
            req.tf(
                "El código cambió desde «{0}» ({1})",
                &[&status.label(req.lang), &utc(*at)]
            )
        ),
    }
}

/// Decision form shared by duplicate, dead-code and review pages.
pub(super) fn review_form(req: &Req<'_>, kind: Kind, identity: &str, state: &State) -> String {
    let mut out = format!(
        "<form action=\"review\" method=\"post\" class=\"review\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"kind\" value=\"{}\"><input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"next\" value=\"{}\">",
        escape(&req.site.csrf),
        kind.value(),
        escape(identity),
        escape(&safe_next(req.target).unwrap_or_default())
    );
    match state {
        State::Pending | State::NeedsReevaluation { .. } => {
            let _ = write!(
                out,
                "<input class=\"reason\" name=\"reason\" maxlength=\"500\" placeholder=\"{}\" aria-label=\"{}\"><button name=\"action\" value=\"confirmed\" title=\"{}\">{}</button><button name=\"action\" value=\"false_positive\" title=\"{}\" class=\"secondary\">{}</button><button name=\"action\" value=\"ignored\" title=\"{}\" class=\"secondary\">{}</button>",
                req.t("Motivo (opcional)"),
                req.t("Motivo de la decisión"),
                req.t("Es un problema real"),
                req.t("Confirmar hallazgo"),
                req.t("El detector se equivoca"),
                req.t("Falso positivo"),
                req.t("Es real, pero se acepta a propósito"),
                req.t("Ignorar a propósito"),
            );
        }
        State::Decided { reason, .. } => {
            if !reason.is_empty() {
                let _ = write!(
                    out,
                    "<span class=\"muted\">{}</span>",
                    req.tf("Motivo: {0}", &[&escape(reason)])
                );
            }
            let _ = write!(
                out,
                "<button name=\"action\" value=\"restore\" class=\"secondary\">{}</button>",
                req.t("Restaurar a pendiente")
            );
        }
    }
    out.push_str("</form>");
    out
}

pub(super) fn route(site: &Site, target: &str, lang: Lang) -> Response {
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
    if ["page", "ip", "rp", "sp", "a", "b", "context"]
        .iter()
        .any(|key| {
            params
                .get(*key)
                .is_some_and(|value| value.parse::<usize>().is_err())
        })
    {
        return Response::error(400);
    }
    let req = Req {
        site,
        params,
        target,
        lang,
    };
    let legacy = |new: &str| {
        Response::redirect(if query.is_empty() {
            new.to_string()
        } else {
            format!("{new}?{query}")
        })
    };
    match route {
        "" => Response::redirect("duplicados".to_string()),
        "style.css" => Response::text("text/css; charset=utf-8", include_str!("style.css").into()),
        "app.js" => Response::text(
            "text/javascript; charset=utf-8",
            include_str!("app.js").into(),
        ),
        "duplicados" => view_duplicates::list(&req),
        "duplicado" => view_duplicates::detail(&req),
        "duplicados.toon" => Response::text(
            "text/plain; charset=utf-8",
            crate::duplicates::render_toon(&site.duplicates),
        ),
        "muerto" => view_dead::list(&req),
        "simbolo" => view_dead::detail(&req),
        "obsidian" => view_obsidian::dashboard(&req),
        "explorar" => view_obsidian::explorer(&req),
        "archivo" => view_obsidian::file(&req),
        "notas" => view_obsidian::notes(&req),
        "nota" => view_obsidian::note(&req),
        "grafo" => view_obsidian::graph(&req),
        "revisiones" => view_reviews::list(&req),
        "estado" => view_status::status(&req),
        "ajustes" => view_status::settings(&req),
        // Links from before ADR-018 keep working.
        "duplicates" | "group" => Response::redirect("duplicados".to_string()),
        "atlas" => legacy("explorar"),
        "file" => legacy("archivo"),
        _ => Response::error(404),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn dates_are_formatted_in_utc() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(utc(951_782_400 + 3600 * 5 + 60 * 7), "2000-02-29 05:07 UTC");
    }

    #[test]
    fn next_targets_are_known_routes_with_tame_queries() {
        for ok in [
            "duplicados",
            "muerto?lang=rust&q=a%20b",
            "duplicado?k=abc",
            "estado",
        ] {
            assert_eq!(safe_next(ok).as_deref(), Some(ok));
        }
        for bad in [
            "//evil.example",
            "https://evil.example",
            "style.css",
            "duplicados?x=<script>",
            "duplicados\r\nSet-Cookie: a=b",
            "/duplicados",
            "..%2f",
        ] {
            assert_eq!(safe_next(bad), None, "{bad}");
        }
    }
}
