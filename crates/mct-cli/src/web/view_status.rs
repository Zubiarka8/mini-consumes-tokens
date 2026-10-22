//! "Estado" (index freshness, diagnostics, reload) and "Ajustes" (read-only).

use std::fmt::Write as _;

use super::render::{escape, page, utc, Req, Response, Section};

pub(super) fn status(req: &Req<'_>) -> Response {
    let site = req.site;
    let index = &site.status;
    let counts = site.counts();
    let mut body = String::new();
    if super::index_stamp(&site.root) != site.index_stamp {
        let _ = write!(body, "<p class=\"warning\" role=\"alert\">{}</p>", req.t("El índice se modificó después de esta instantánea. Pulsa «Recargar análisis» para ver los datos actuales."));
    }
    let _ = write!(
        body,
        "<section class=\"cards\"><div class=\"card\"><span class=\"eyebrow\">{}</span><strong>{}</strong><span>{}</span></div><div class=\"card\"><span class=\"eyebrow\">{}</span><strong>{}</strong><span>{}</span></div></section>",
        req.t("Índice"),
        req.tf("{0} archivos", &[&index.total_files]),
        req.tf(
            "{0} símbolos · última indexación: {1}",
            &[
                &index.total_symbols,
                &index
                    .last_indexed_at
                    .map_or(req.t("desconocida").to_string(), |t| utc(t.max(0) as u64))
            ]
        ),
        req.t("Instantánea del panel"),
        utc(site.generated),
        req.t("Los datos de esta página son de ese momento."),
    );
    let _ = write!(
        body,
        "<form action=\"reload\" method=\"post\" class=\"review\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button>{}</button><span class=\"muted\">{}</span></form>",
        escape(&site.csrf),
        req.t("Recargar análisis"),
        req.t("Vuelve a leer el índice y repite el análisis de duplicados y código muerto. No reindexa el código (usa <code>mct-cli reindex</code>) y puede tardar en proyectos grandes. Tus decisiones no se tocan."),
    );

    let _ = write!(body, "<h2>{}</h2>", req.t("Pendiente de revisión"));
    let _ = write!(
        body,
        "<ul><li>{}</li><li>{}</li><li>{}</li></ul>",
        req.tf(
            "<a href=\"duplicados\">Código duplicado</a>: {0} de {1} grupos pendientes",
            &[&counts.duplicates_pending, &site.duplicates.groups.len()]
        ),
        req.tf(
            "<a href=\"muerto\">Código muerto</a>: {0} de {1} candidatos pendientes",
            &[&counts.dead_pending, &site.dead.len()]
        ),
        req.tf(
            "<a href=\"revisiones?status=reevaluate\">Para reevaluar</a>: {0} (el código cambió desde la decisión)",
            &[&counts.reevaluate]
        ),
    );

    let _ = write!(body, "<h2>{}</h2><div class=\"table-wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
        req.t("Cobertura por lenguaje"), req.t("Lenguaje"), req.t("Archivos"), req.t("Símbolos"));
    for language in &index.languages {
        let _ = write!(
            body,
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            escape(&language.language),
            language.file_count,
            language.symbol_count
        );
    }
    body.push_str("</tbody></table></div>");
    if !index.unsupported_languages.is_empty() {
        let _ = write!(
            body,
            "<p>{}</p>",
            req.tf(
                "Lenguajes detectados sin plugin: {0}.",
                &[&escape(&index.unsupported_languages.join(", "))]
            )
        );
    }
    for (title, list) in [
        (req.t("Errores de sintaxis"), &index.syntax_errors),
        (
            req.t("Archivos que no se pudieron leer"),
            &index.read_failures,
        ),
    ] {
        if list.is_empty() {
            continue;
        }
        let _ = write!(body, "<h2>{title} ({})</h2><ul>", list.len());
        for file in list.iter().take(20) {
            let _ = write!(
                body,
                "<li><code>{}</code> <span class=\"muted\">{}</span></li>",
                escape(&file.relative_path),
                escape(&file.detail)
            );
        }
        body.push_str("</ul>");
    }

    let report = &site.duplicates;
    let _ = write!(
        body,
        "<h2>{}</h2><p>{}</p>",
        req.t("Análisis de duplicados"),
        req.tf(
            "{0} candidatos analizados, {1} omitidos. Análisis {2}.",
            &[
                &report.candidates_analyzed,
                &report.skipped.len(),
                &if report.complete && report.has_indexing_record {
                    req.t("completo")
                } else {
                    req.t("parcial o sin verificar: pueden faltar candidatos")
                }
            ]
        )
    );
    let _ = write!(
        body,
        "<h2>{}</h2><p>{}{}</p>",
        req.t("Bóveda de Obsidian"),
        req.tf(
            "{0} notas, {1} enlaces entre notas. Carpeta: <code>.mct-index/obsidian/</code>.",
            &[&site.notes.list.len(), &site.notes.link_count]
        ),
        if site.no_obsidian {
            format!(
                " {}",
                req.t("Regeneración desactivada con <code>--no-obsidian</code>.")
            )
        } else {
            String::new()
        }
    );
    page(req, req.t("Estado del proyecto"), Section::Status, &body)
}

pub(super) fn settings(req: &Req<'_>) -> Response {
    let site = req.site;
    let rules = mct_index::read_ignore_file(&site.root);
    let options = &site.duplicates.options;
    let mut body = format!("<p class=\"intro\">{}</p>", req.t("Esta pantalla solo muestra la configuración; el panel no modifica código, índice ni reglas."));
    let _ = write!(
        body,
        "<h2>{}</h2><dl class=\"facts\"><dt>{}</dt><dd><code>{}</code></dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd><code>.mct-index/obsidian/</code></dd></dl>",
        req.t("Rutas"),
        req.t("Proyecto"),
        escape(&site.root.display().to_string()),
        req.t("Índice"),
        req.t("<code>.mct-index/index.sqlite3</code> (solo lectura)"),
        req.t("Decisiones"),
        req.t("<code>.mct-index/reviews.toon</code> (con historial). El archivo anterior <code>.mct-index/duplicate-reviews.toon</code> se sigue leyendo y no se modifica."),
        req.t("Bóveda"),
    );
    let _ = write!(
        body,
        "<h2>{}</h2><p>{}</p>",
        req.t("Detector de duplicados"),
        req.tf(
            "Normalización «{0}», mínimo {1} líneas y {2} tokens. Se cambia con <code>mct-cli duplicates</code>, no desde el panel.",
            &[
                &escape(options.normalization.as_str()),
                &options.min_lines,
                &options.min_tokens
            ]
        )
    );
    let _ = write!(
        body,
        "<h2>{}</h2>{}",
        req.t("Exclusiones (<code>.mctignore</code>)"),
        if rules.is_empty() {
            format!(
                "<p class=\"muted\">{}</p>",
                req.t("No hay reglas propias; se aplican las exclusiones integradas.")
            )
        } else {
            format!(
                "<ul>{}</ul>",
                rules
                    .iter()
                    .take(100)
                    .map(|rule| format!("<li><code>{}</code></li>", escape(rule)))
                    .collect::<String>()
            )
        }
    );
    let _ = write!(
        body,
        "<h2>{}</h2><ul><li>{}</li><li>{}</li><li>{}</li></ul>",
        req.t("Seguridad"),
        req.t("Solo escucha en <code>127.0.0.1</code> y exige el token secreto de la URL."),
        req.t("Comprueba Host, Origin y un token CSRF en cada cambio."),
        req.t("Solo lee archivos que el índice lista, sin seguir enlaces simbólicos, y recorta cada vista previa."),
    );
    page(req, req.t("Ajustes"), Section::Settings, &body)
}
