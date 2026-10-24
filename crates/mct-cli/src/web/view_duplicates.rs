//! "Código duplicado": the existing duplicate report, reviewed in the browser.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use mct_index::{DifferenceKind, DuplicateGroup, IdentifierPattern, VerdictLevel, VerdictReason};

use super::i18n::{t, Lang};
use super::render::{
    escape, href, link, page, pager, review_form, select, state_chip, status_counts,
    status_matches, status_tabs, text_input, utc, Req, Response, Section, GROUP_PAGE_SIZE,
};
use super::reviews::{Kind, State};
use super::source;

fn verdict_label(lang: Lang, level: VerdictLevel) -> &'static str {
    match level {
        VerdictLevel::Real => t(lang, "Coincidencia clara"),
        VerdictLevel::Suspicious => t(lang, "Requiere revisión"),
        VerdictLevel::FalsePositive => t(lang, "Probable falso positivo"),
    }
}

fn verdict_help(lang: Lang, level: VerdictLevel) -> &'static str {
    match level {
        VerdictLevel::Real => t(lang, "Los fragmentos solo difieren en nombres. Sigue siendo una heurística: no prueba que se comporten igual."),
        VerdictLevel::Suspicious => t(lang, "Hay diferencias que el detector no sabe juzgar; decide tú si el parecido es real."),
        VerdictLevel::FalsePositive => t(lang, "La evidencia indica que no son duplicados."),
    }
}

fn reason_label(lang: Lang, reason: VerdictReason) -> &'static str {
    match reason {
        VerdictReason::IdenticalTokens => t(lang, "Todos los identificadores alineados tienen el mismo texto."),
        VerdictReason::ConsistentRenaming => t(lang, "Los identificadores solo cambian por un renombrado uno a uno."),
        VerdictReason::SameContext => t(lang, "Decoradores y construcciones envolventes son iguales."),
        VerdictReason::DocstringsDiffer => t(lang, "Las documentaciones difieren (no se considera sospechoso)."),
        VerdictReason::AlignmentUnavailable => t(lang, "No se pudo alinear los fragmentos token a token."),
        VerdictReason::IdentifierPatternDiffers => t(lang, "Los identificadores no se corresponden uno a uno."),
        VerdictReason::UndeclaredNamesDiffer => t(lang, "Cambian nombres que no son parámetros propios: pueden ser variables locales renombradas o referencias a elementos distintos."),
        VerdictReason::MixedRoleDifferences => t(lang, "En alguna posición, los fragmentos usan el mismo texto con distinto papel sintáctico."),
        VerdictReason::MemberNamesDiffer => t(lang, "Cambian nombres de miembros o campos."),
        VerdictReason::CalleeNamesDiffer => t(lang, "Llaman a funciones distintas."),
        VerdictReason::TypeNamesDiffer => t(lang, "Cambian nombres de tipos."),
        VerdictReason::PrimitiveTypeSpellingsDiffer => t(lang, "Cambia la escritura de tipos primitivos."),
        VerdictReason::LiteralCategoriesDiffer => t(lang, "Cambia la categoría de algún literal (por ejemplo número frente a texto)."),
        VerdictReason::LiteralValuesDiffer => t(lang, "Cambian valores literales."),
        VerdictReason::InterpolatedStringsDiffer => t(lang, "Cambian cadenas con interpolación."),
        VerdictReason::ContextUnavailable => t(lang, "No se pudo comparar el contexto (decoradores y construcciones envolventes)."),
        VerdictReason::DecoratorsDiffer => t(lang, "Los decoradores difieren."),
        VerdictReason::EnclosingContextDiffers => t(lang, "El contexto envolvente difiere."),
        VerdictReason::CallOnlyBodies => t(lang, "Los cuerpos son solo llamadas planas: coincidencia poco informativa."),
        VerdictReason::CallShapeMixed => t(lang, "Solo algunos cuerpos son llamadas planas."),
        VerdictReason::BodyShapeNotEvaluated => t(lang, "No se evaluó la forma del cuerpo."),
    }
}

fn difference_label(lang: Lang, kind: DifferenceKind) -> &'static str {
    match kind {
        DifferenceKind::Declaration => t(lang, "Nombre declarado"),
        DifferenceKind::Parameter => t(lang, "Parámetro"),
        DifferenceKind::Name => t(lang, "Nombre"),
        DifferenceKind::Member => t(lang, "Miembro"),
        DifferenceKind::Callee => t(lang, "Función llamada"),
        DifferenceKind::Type => t(lang, "Tipo"),
        DifferenceKind::LiteralValue => t(lang, "Valor literal"),
        DifferenceKind::LiteralCategory => t(lang, "Categoría de literal"),
        DifferenceKind::InterpolatedString => t(lang, "Cadena interpolada"),
        DifferenceKind::Docstring => t(lang, "Documentación"),
        DifferenceKind::MixedRole => t(lang, "Papel distinto"),
    }
}

struct Filter<'a> {
    query: String,
    language: &'a str,
    directory: &'a str,
    verdict: &'a str,
    path: &'a str,
}

fn matches(group: &DuplicateGroup, filter: &Filter<'_>) -> bool {
    (filter.language.is_empty() || group.language == filter.language)
        && (filter.verdict.is_empty()
            || group
                .verdict
                .level
                .as_str()
                .eq_ignore_ascii_case(filter.verdict))
        && (filter.path.is_empty() || group.members.iter().any(|m| m.path == filter.path))
        && (filter.directory.is_empty()
            || group.members.iter().any(|m| {
                m.path == filter.directory
                    || m.path
                        .starts_with(&format!("{}/", filter.directory.trim_end_matches('/')))
            }))
        && (filter.query.is_empty()
            || group.members.iter().any(|m| {
                m.path.to_lowercase().contains(&filter.query)
                    || m.symbol_name.to_lowercase().contains(&filter.query)
            }))
}

fn notice(req: &Req<'_>) -> String {
    let report = &req.site.duplicates;
    let mut out = format!(
        "<p class=\"notice\">{}</p>",
        req.tf(
            "Un grupo es un candidato para revisión humana, no una prueba de redundancia ni de comportamiento equivalente. Política: normalización «{0}», mínimo {1} líneas y {2} tokens. {3} candidatos analizados; {4} omitidos. Diagnósticos del índice: {5} errores de sintaxis y {6} fallos de lectura.",
            &[
                &escape(report.options.normalization.as_str()),
                &report.options.min_lines,
                &report.options.min_tokens,
                &report.candidates_analyzed,
                &report.skipped.len(),
                &report.index_syntax_errors,
                &report.index_read_failures,
            ]
        )
    );
    if !report.complete || !report.has_indexing_record {
        let _ = write!(out, "<p class=\"warning\">{}</p>", req.t("Análisis parcial o sin verificar: pueden faltar candidatos. El informe TOON completo detalla los omitidos y el estado del índice."));
    }
    if !report.unsupported_languages.is_empty() {
        let _ = write!(
            out,
            "<p class=\"muted\">{}</p>",
            req.tf(
                "Lenguajes sin soporte de duplicados: {0}.",
                &[&escape(&report.unsupported_languages.join(", "))]
            )
        );
    }
    out
}

pub(super) fn list(req: &Req<'_>) -> Response {
    let site = req.site;
    let status = match req.param("status") {
        "" => "pending",
        other => other,
    };
    if !matches!(
        status,
        "pending" | "reevaluate" | "confirmed" | "false_positive" | "ignored" | "all"
    ) || !matches!(
        req.param("verdict"),
        "" | "real" | "suspicious" | "false_positive"
    ) {
        return Response::error(400);
    }
    let filter = Filter {
        query: req.param("q").to_lowercase(),
        language: req.param("lang"),
        directory: req.param("dir"),
        verdict: req.param("verdict"),
        path: req.param("path"),
    };
    let filtered: Vec<usize> = (0..site.duplicates.groups.len())
        .filter(|id| matches(&site.duplicates.groups[*id], &filter))
        .collect();
    let states: Vec<State> = filtered
        .iter()
        .map(|id| site.duplicate_state(*id))
        .collect();
    let counts = status_counts(states.iter());
    let shown: Vec<(usize, &State)> = filtered
        .iter()
        .copied()
        .zip(&states)
        .filter(|(_, state)| status_matches(status, state))
        .collect();

    let languages: BTreeSet<_> = site
        .duplicates
        .groups
        .iter()
        .map(|group| group.language.as_str())
        .collect();
    let mut language_options = vec![("", req.t("Todos").to_string())];
    language_options.extend(
        languages
            .into_iter()
            .map(|language| (language, language.to_string())),
    );
    let verdict_options = [
        ("", req.t("Todos").to_string()),
        (
            "real",
            verdict_label(req.lang, VerdictLevel::Real).to_string(),
        ),
        (
            "suspicious",
            verdict_label(req.lang, VerdictLevel::Suspicious).to_string(),
        ),
    ];
    let mut body = format!("<p class=\"intro\">{}</p>", req.t("Fragmentos de código que el detector considera casi idénticos. Abre un grupo para compararlos lado a lado y decide qué hacer."));
    let _ = write!(
        body,
        "<form action=\"duplicados\" method=\"get\" class=\"filters\" role=\"search\" data-filter-form>{}{}{}{}<input type=\"hidden\" name=\"status\" value=\"{}\"><button>{}</button></form>",
        text_input("q", req.t("Buscar"), req.param("q"), req.t("ruta o nombre de función")),
        select("lang", req.t("Lenguaje"), &language_options, filter.language),
        text_input("dir", req.t("Carpeta"), filter.directory, req.t("p. ej. crates/mct-index")),
        select("verdict", req.t("Evidencia"), &verdict_options, filter.verdict),
        escape(status),
        req.t("Filtrar"),
    );
    let base = [
        ("q", req.param("q")),
        ("lang", filter.language),
        ("dir", filter.directory),
        ("verdict", filter.verdict),
        ("path", filter.path),
    ];
    body.push_str(&status_tabs(req, "duplicados", &base, status, &counts));
    if !filter.path.is_empty() {
        let _ = write!(
            body,
            "<p>{} · {}</p>",
            req.tf(
                "Archivo: {0}",
                &[&link("archivo", &[("path", filter.path)], filter.path)]
            ),
            link("duplicados", &[], req.t("Quitar filtro"))
        );
    }
    body.push_str(&notice(req));
    let mut paging: Vec<(&str, &str)> = base.to_vec();
    paging.push(("status", status));
    let (start, pagination) = pager(
        req,
        "duplicados",
        &paging,
        "page",
        shown.len(),
        GROUP_PAGE_SIZE,
    );
    for (id, state) in shown.iter().skip(start).take(GROUP_PAGE_SIZE) {
        let group = &site.duplicate_keys[*id];
        let data = &site.duplicates.groups[*id];
        let _ = write!(
            body,
            "<article class=\"finding\" data-search=\"{}\"><h2><a href=\"{}\">{}</a> <span class=\"badge\">{}</span> <span class=\"badge\">{}</span></h2><p class=\"meta\">{} <span class=\"muted\">{}</span></p><ul class=\"members\">",
            escape(
                &data
                    .members
                    .iter()
                    .map(|m| format!("{} {}", m.path, m.symbol_name))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
            ),
            href("duplicado", &[("k", &group.identity)]),
            req.tf("Grupo {0}", &[&(id + 1)]),
            escape(&data.language),
            verdict_label(req.lang, data.verdict.level),
            req.tf(
                "{0} miembros · {1} líneas · {2} tokens",
                &[&data.members.len(), &data.line_count, &data.token_count]
            ),
            req.t("(medidos por el detector, normalizados)"),
        );
        for member in data.members.iter().take(4) {
            let _ = write!(
                body,
                "<li>{} <code>{}</code></li>",
                link(
                    "archivo",
                    &[
                        ("path", &member.path),
                        ("from", &member.start_line.to_string()),
                        ("to", &member.end_line.to_string())
                    ],
                    &format!("{}:{}–{}", member.path, member.start_line, member.end_line)
                ),
                escape(&member.symbol_name)
            );
        }
        if data.members.len() > 4 {
            let _ = write!(
                body,
                "<li class=\"muted\">{}</li>",
                req.tf("y {0} más en el detalle", &[&(data.members.len() - 4)])
            );
        }
        let _ = write!(
            body,
            "</ul><p>{}</p>{}</article>",
            state_chip(req, state),
            review_form(req, Kind::Duplicate, &group.identity, state)
        );
    }
    if shown.is_empty() {
        let _ = write!(
            body,
            "<p class=\"empty\">{}",
            req.t("No hay grupos con estos filtros.")
        );
        if status == "pending" && counts[5] > 0 {
            let _ = write!(
                body,
                " {}",
                req.t("Hay otros ya revisados: cambia de pestaña para verlos.")
            );
        } else if counts[5] == 0 && site.duplicates.groups.is_empty() {
            let _ = write!(
                body,
                " {}",
                req.t("El detector no encontró duplicados en este proyecto.")
            );
        }
        body.push_str("</p>");
    }
    body.push_str(&pagination);
    let _ = write!(
        body,
        "<p class=\"muted\"><a href=\"duplicados.toon\" download=\"duplicados.toon\">{}</a></p>",
        req.t("Descargar el análisis completo en bruto (TOON, incluye grupos ya revisados)")
    );
    page(req, req.t("Código duplicado"), Section::Duplicates, &body)
}

fn pane(req: &Req<'_>, group: &DuplicateGroup, index: usize, side: &str) -> String {
    let member = &group.members[index];
    let marked: BTreeSet<u32> = group
        .evidence
        .differences
        .iter()
        .filter_map(|difference| difference.lines.get(index).copied())
        .collect();
    let mut out = format!(
        "<section class=\"pane\" aria-label=\"{}\"><h3>{} <span class=\"muted\">{}</span></h3><p class=\"meta\">{} · {}:{}–{} · {}</p>",
        escape(side),
        escape(&member.symbol_name),
        escape(&member.symbol_kind),
        link(
            "archivo",
            &[
                ("path", &member.path),
                ("from", &member.start_line.to_string()),
                ("to", &member.end_line.to_string())
            ],
            req.t("Abrir el archivo")
        ),
        escape(&member.path),
        member.start_line,
        member.end_line,
        req.tf(
            "{0} líneas con código · {1} tokens",
            &[&member.line_count, &member.token_count]
        ),
    );
    match source::excerpt(
        &req.site.root,
        &member.path,
        member.start_line,
        member.end_line,
    ) {
        Some(excerpt) => {
            let _ = write!(
                out,
                "<div class=\"code\" role=\"region\" tabindex=\"0\" aria-label=\"{}\">",
                req.t("Código")
            );
            for (offset, line) in excerpt.lines.iter().enumerate() {
                let number = excerpt.first + offset as u32;
                let class = if marked.contains(&number) {
                    "ln diff"
                } else {
                    "ln"
                };
                let _ = write!(
                    out,
                    "<div class=\"{class}\"><span class=\"n\">{number}</span><code>{}</code></div>",
                    escape(line)
                );
            }
            out.push_str("</div>");
            if excerpt.truncated {
                let _ = write!(
                    out,
                    "<p class=\"muted\">{}</p>",
                    req.t("Vista recortada; abre el archivo para ver el resto.")
                );
            }
        }
        None => {
            let _ = write!(out, "<p class=\"warning\">{}</p><pre>", req.t("No se pudo leer el archivo (cambió o ya no existe). Se muestran las primeras líneas guardadas por el detector."));
            out.push_str(&escape(&member.excerpt.join("\n")));
            out.push_str("</pre>");
        }
    }
    out.push_str("</section>");
    out
}

pub(super) fn detail(req: &Req<'_>) -> Response {
    let site = req.site;
    let Some(id) = site.duplicate_ids.get(req.param("k")).copied() else {
        return Response::error(404);
    };
    let group = &site.duplicates.groups[id];
    let key = &site.duplicate_keys[id];
    let state = site.duplicate_state(id);
    let count = group.members.len();
    let a = req.number("a").min(count - 1);
    let b = req
        .params
        .get("b")
        .and_then(|b| b.parse().ok())
        .unwrap_or(1)
        .min(count - 1);
    let (a, b) = if a == b { (a, (a + 1) % count) } else { (a, b) };

    let mut body = format!(
        "<p>{}</p><p>{} <span class=\"badge\">{}</span> <span class=\"badge\">{}</span></p><p class=\"meta\">{} <span class=\"muted\">{}</span></p>",
        link("duplicados", &[], req.t("← Todos los grupos")),
        state_chip(req, &state),
        escape(&group.language),
        verdict_label(req.lang, group.verdict.level),
        req.tf(
            "{0} miembros · {1} líneas · {2} tokens",
            &[&count, &group.line_count, &group.token_count]
        ),
        req.t("(medidos por el detector sobre el código normalizado)"),
    );
    body.push_str(&review_form(req, Kind::Duplicate, &key.identity, &state));

    let _ = write!(
        body,
        "<h2>{}</h2><p>{} {}</p>",
        req.t("Por qué se detectó"),
        req.tf(
            "Los {0} fragmentos producen la <strong>misma secuencia de tokens</strong> tras la normalización «{1}».",
            &[&count, &escape(site.duplicates.options.normalization.as_str())]
        ),
        verdict_help(req.lang, group.verdict.level)
    );
    if !group.verdict.reasons.is_empty() {
        body.push_str("<ul>");
        for reason in &group.verdict.reasons {
            let _ = write!(body, "<li>{}</li>", reason_label(req.lang, *reason));
        }
        body.push_str("</ul>");
    }
    let _ = write!(
        body,
        "<p class=\"muted\">{}</p>",
        req.tf(
            "Prioridad: {0} — {1}",
            &[
                &escape(group.priority.level.as_str()),
                &escape(&group.priority.reason)
            ]
        )
    );

    if count > 2 {
        let options: Vec<(String, String)> = group
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    i.to_string(),
                    format!("{} · {}:{}", m.symbol_name, m.path, m.start_line),
                )
            })
            .collect();
        let borrowed: Vec<(&str, String)> = options
            .iter()
            .map(|(v, l)| (v.as_str(), l.clone()))
            .collect();
        let _ = write!(
            body,
            "<form class=\"filters\" action=\"duplicado\" method=\"get\"><input type=\"hidden\" name=\"k\" value=\"{}\">{}{}<button>{}</button></form>",
            escape(&key.identity),
            select("a", req.t("Fragmento izquierdo"), &borrowed, &a.to_string()),
            select("b", req.t("Fragmento derecho"), &borrowed, &b.to_string()),
            req.t("Comparar"),
        );
    }
    let _ = write!(
        body,
        "<h2>{}</h2><p class=\"muted\">{}</p><div class=\"compare\">",
        req.t("Comparación"),
        req.t("Las líneas marcadas contienen una diferencia de tokens señalada por el detector (hasta 12 diferencias distintas por grupo). Una línea sin marca puede diferir igualmente si la diferencia no se listó.")
    );
    body.push_str(&pane(req, group, a, req.t("Fragmento izquierdo")));
    body.push_str(&pane(req, group, b, req.t("Fragmento derecho")));
    body.push_str("</div>");

    let evidence = &group.evidence;
    let pattern = match &evidence.identifier_pattern {
        IdentifierPattern::NotEvaluated => req.t("no evaluado").to_string(),
        IdentifierPattern::Identical => req.t("idénticos").to_string(),
        IdentifierPattern::OneToOne => req.t("renombrado uno a uno").to_string(),
        IdentifierPattern::NotOneToOne(detail) => {
            req.tf("no corresponden uno a uno ({0})", &[&escape(detail)])
        }
    };
    let _ = write!(
        body,
        "<h2>{}</h2><p>{}</p>",
        req.t("Diferencias entre fragmentos"),
        req.tf(
            "Identificadores: {0}. Diferencias distintas: {1}.",
            &[&pattern, &evidence.distinct_differences]
        )
    );
    if evidence.differences.is_empty() {
        let _ = write!(
            body,
            "<p class=\"muted\">{}</p>",
            req.t("El detector no listó diferencias de tokens.")
        );
    } else {
        let _ = write!(
            body,
            "<div class=\"table-wrap\"><table><thead><tr><th>{}</th><th>{}</th>",
            req.t("Tipo"),
            req.t("Veces")
        );
        for member in &group.members {
            let _ = write!(body, "<th>{}</th>", escape(&member.symbol_name));
        }
        body.push_str("</tr></thead><tbody>");
        for difference in &evidence.differences {
            let _ = write!(
                body,
                "<tr><td>{}</td><td>{}</td>",
                difference_label(req.lang, difference.kind),
                difference.occurrences
            );
            for value in &difference.values {
                let _ = write!(body, "<td><code>{}</code></td>", escape(value));
            }
            body.push_str("</tr>");
        }
        body.push_str("</tbody></table></div>");
    }
    for warning in &evidence.warnings {
        let _ = write!(
            body,
            "<p class=\"notice\">{} — {}</p>",
            escape(warning.code.as_str()),
            escape(&warning.detail)
        );
    }
    let _ = write!(
        body,
        "<h2>{}</h2><ul class=\"members\">",
        req.t("Todos los fragmentos")
    );
    for (i, member) in group.members.iter().enumerate() {
        let _ = write!(
            body,
            "<li>{} <code>{}</code> <span class=\"muted\">{}{}</span></li>",
            link(
                "archivo",
                &[
                    ("path", &member.path),
                    ("from", &member.start_line.to_string()),
                    ("to", &member.end_line.to_string())
                ],
                &format!("{}:{}–{}", member.path, member.start_line, member.end_line)
            ),
            escape(&member.symbol_name),
            if i == a || i == b {
                format!("{} · ", req.t("en comparación"))
            } else {
                String::new()
            },
            req.tf("{0} líneas", &[&member.line_count])
        );
    }
    body.push_str("</ul>");
    body.push_str(&history_html(req, Kind::Duplicate, &key.identity));
    let _ = write!(
        body,
        "<details><summary>{}</summary><pre>",
        req.t("Alineación completa, subgrupos y análisis (TOON)")
    );
    let mut evidence_text = String::new();
    crate::duplicates::render_group(
        &mut evidence_text,
        id + 1,
        group,
        site.duplicates.options.normalization,
    );
    body.push_str(&escape(&evidence_text));
    body.push_str("</pre></details>");
    page(
        req,
        &req.tf("Grupo {0}", &[&(id + 1)]),
        Section::Duplicates,
        &body,
    )
}

/// Decision history of one finding, newest first.
pub(super) fn history_html(req: &Req<'_>, kind: Kind, identity: &str) -> String {
    let history = req
        .site
        .reviews
        .history(kind, identity)
        .unwrap_or_else(|error| {
            eprintln!("warning: cannot read review history: {error:#}");
            Vec::new()
        });
    if history.is_empty() {
        return format!(
            "<h2>{}</h2><p class=\"muted\">{}</p>",
            req.t("Historial de decisiones"),
            req.t("Sin decisiones registradas.")
        );
    }
    let mut out = format!(
        "<h2>{}</h2><ol class=\"history\">",
        req.t("Historial de decisiones")
    );
    for record in history {
        let _ = write!(
            out,
            "<li><span class=\"chip chip-{}\">{}</span> <span class=\"muted\">{}</span>{}</li>",
            record.status.value(),
            record.status.label(req.lang),
            utc(record.at),
            if record.reason.is_empty() {
                String::new()
            } else {
                format!(" — {}", escape(&record.reason))
            }
        );
    }
    out.push_str("</ol>");
    out
}
