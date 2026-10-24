//! "Revisiones": every decision about duplicate and dead-code findings, in one place.

use std::fmt::Write as _;

use super::render::{
    escape, href, page, pager, review_form, select, state_chip, text_input, utc, Req, Response,
    Section,
};
use super::reviews::{Kind, State};

const PAGE: usize = 50;

struct Row {
    kind: Kind,
    identity: String,
    label: String,
    detail: String,
    state: State,
    at: u64,
}

fn at(state: &State) -> u64 {
    match state {
        State::Decided { at, .. } => at.unwrap_or(0),
        State::NeedsReevaluation { at, .. } => *at,
        State::Pending => 0,
    }
}

pub(super) fn list(req: &Req<'_>) -> Response {
    let site = req.site;
    let tab = match req.param("status") {
        "" => "all",
        other => other,
    };
    let kind_filter = req.param("kind");
    if !matches!(
        tab,
        "all" | "reevaluate" | "confirmed" | "false_positive" | "ignored" | "obsolete"
    ) || !matches!(kind_filter, "" | "duplicate" | "dead")
    {
        return Response::error(400);
    }
    let query = req.param("q").to_lowercase();
    let mut rows: Vec<Row> = Vec::new();
    for (id, group) in site.duplicates.groups.iter().enumerate() {
        let state = site.duplicate_state(id);
        if state == State::Pending {
            continue;
        }
        let names: Vec<String> = group
            .members
            .iter()
            .map(|m| format!("{} ({})", m.symbol_name, m.path))
            .collect();
        rows.push(Row {
            kind: Kind::Duplicate,
            identity: site.duplicate_keys[id].identity.clone(),
            label: format!(
                "{} · {}",
                req.tf("Grupo {0}", &[&(id + 1)]),
                names.join(" ↔ ")
            ),
            detail: href("duplicado", &[("k", &site.duplicate_keys[id].identity)]),
            at: at(&state),
            state,
        });
    }
    for (id, dead) in site.dead.iter().enumerate() {
        let state = site.dead_state(id);
        if state == State::Pending {
            continue;
        }
        rows.push(Row {
            kind: Kind::Dead,
            identity: dead.identity.clone(),
            label: format!(
                "{} ({}:{})",
                dead.entry.name, dead.entry.relative_path, dead.entry.line
            ),
            detail: href("simbolo", &[("k", &dead.identity)]),
            at: at(&state),
            state,
        });
    }
    let current: std::collections::HashSet<(Kind, &str)> = site
        .duplicate_keys
        .iter()
        .map(|k| (Kind::Duplicate, k.identity.as_str()))
        .chain(site.dead.iter().map(|d| (Kind::Dead, d.identity.as_str())))
        .collect();
    let obsolete: Vec<_> = site
        .reviews
        .stored_findings()
        .unwrap_or_else(|error| {
            eprintln!("warning: cannot read review decisions: {error:#}");
            Vec::new()
        })
        .into_iter()
        .filter(|record| !current.contains(&(record.kind, record.identity.as_str())))
        .collect();

    rows.retain(|row| {
        (kind_filter.is_empty() || row.kind.value() == kind_filter)
            && (query.is_empty() || row.label.to_lowercase().contains(&query))
    });
    rows.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.label.cmp(&b.label)));
    let count = |f: &dyn Fn(&State) -> bool| rows.iter().filter(|row| f(&row.state)).count();
    let decided = |wanted: &'static str| {
        count(&|s| matches!(s, State::Decided { status, .. } if status.value() == wanted))
    };
    let tabs = [
        ("all", req.t("Todas las decisiones"), rows.len()),
        (
            "reevaluate",
            req.t("Para reevaluar"),
            count(&|s| matches!(s, State::NeedsReevaluation { .. })),
        ),
        ("confirmed", req.t("Confirmados"), decided("confirmed")),
        (
            "false_positive",
            req.t("Falsos positivos"),
            decided("false_positive"),
        ),
        ("ignored", req.t("Ignorados"), decided("ignored")),
        ("obsolete", req.t("Sin hallazgo actual"), obsolete.len()),
    ];

    let mut body = format!("<p class=\"intro\">{}</p>", req.t("Todas las decisiones que has tomado sobre código duplicado y código muerto. Un <strong>falso positivo</strong> es un error del detector; <strong>ignorar</strong> es aceptar a conciencia un problema real. Las decisiones sobreviven a recargas, reinicios y reindexados mientras el hallazgo siga existiendo."));
    let kind_options = [
        ("", req.t("Todos").to_string()),
        ("duplicate", req.t("Código duplicado").to_string()),
        ("dead", req.t("Código muerto").to_string()),
    ];
    let _ = write!(
        body,
        "<form action=\"revisiones\" method=\"get\" class=\"filters\" role=\"search\" data-filter-form>{}{}<input type=\"hidden\" name=\"status\" value=\"{}\"><button>{}</button></form>",
        text_input("q", req.t("Buscar"), req.param("q"), req.t("nombre o ruta")),
        select("kind", req.t("Tipo de hallazgo"), &kind_options, kind_filter),
        escape(tab),
        req.t("Filtrar"),
    );
    let _ = write!(
        body,
        "<nav class=\"tabs\" aria-label=\"{}\">",
        req.t("Estado")
    );
    for (value, label, total) in &tabs {
        let current = if *value == tab {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            body,
            "<a href=\"{}\"{current}>{label} <span class=\"count\">{total}</span></a>",
            href(
                "revisiones",
                &[
                    ("q", req.param("q")),
                    ("kind", kind_filter),
                    ("status", value)
                ]
            )
        );
    }
    body.push_str("</nav>");
    let paging = [
        ("q", req.param("q")),
        ("kind", kind_filter),
        ("status", tab),
    ];

    if tab == "obsolete" {
        let _ = write!(body, "<p class=\"notice\">{}</p>", req.t("Decisiones guardadas cuyo hallazgo ya no aparece en el análisis actual (el código se borró, se renombró o dejó de detectarse). No se descartan: si el hallazgo vuelve a aparecer, se reconocerá."));
        let list: Vec<_> = obsolete
            .iter()
            .filter(|r| kind_filter.is_empty() || r.kind.value() == kind_filter)
            .collect();
        let (start, pagination) = pager(req, "revisiones", &paging, "page", list.len(), PAGE);
        body.push_str("<ul class=\"decisions\">");
        for record in list.iter().skip(start).take(PAGE) {
            let _ = write!(
                body,
                "<li><span class=\"badge\">{}</span> <span class=\"chip chip-{}\">{}</span> <span class=\"muted\">{} · {}</span>{}<form action=\"review/delete\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"kind\" value=\"{}\"><input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"expected_revision\" value=\"{}\"><button>{}</button></form></li>",
                record.kind.label(req.lang),
                record.status.value(),
                record.status.label(req.lang),
                utc(record.at),
                &record.identity[..12],
                if record.reason.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", escape(&record.reason))
                },
                escape(&site.csrf), record.kind.value(), record.identity, site.reviews.decision_revision(record.kind,&record.identity).unwrap_or(0), req.t("Borrar decisión")
            );
        }
        if list.is_empty() {
            let _ = write!(
                body,
                "<li class=\"empty\">{}</li>",
                req.t("No hay decisiones sin hallazgo actual.")
            );
        }
        body.push_str("</ul>");
        body.push_str(&pagination);
        return page(req, req.t("Revisiones"), Section::Reviews, &body);
    }

    let shown: Vec<&Row> = rows
        .iter()
        .filter(|row| match tab {
            "all" => true,
            "reevaluate" => matches!(row.state, State::NeedsReevaluation { .. }),
            other => matches!(&row.state, State::Decided { status, .. } if status.value() == other),
        })
        .collect();
    let (start, pagination) = pager(req, "revisiones", &paging, "page", shown.len(), PAGE);
    if shown.is_empty() {
        let _ = write!(
            body,
            "<p class=\"empty\">{}</p>",
            req.t("Todavía no hay decisiones en esta vista. Revisa hallazgos en <a href=\"duplicados\">Código duplicado</a> o <a href=\"muerto\">Código muerto</a>.")
        );
    } else {
        body.push_str("<ul class=\"decisions\">");
        for row in shown.iter().skip(start).take(PAGE) {
            let _ = write!(
                body,
                "<li class=\"finding\" data-search=\"{}\"><p><span class=\"badge\">{}</span> <a href=\"{}\">{}</a></p><p>{}</p>{}</li>",
                escape(&row.label.to_lowercase()),
                row.kind.label(req.lang),
                row.detail,
                escape(&row.label),
                state_chip(req, &row.state),
                review_form(req, row.kind, &row.identity, &row.state),
            );
        }
        body.push_str("</ul>");
    }
    body.push_str(&pagination);
    page(req, req.t("Revisiones"), Section::Reviews, &body)
}
