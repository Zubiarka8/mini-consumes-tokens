//! "Código muerto": symbols nothing in the index refers to, reviewed in the browser.
//!
//! The detector is a heuristic (ADR-018 D6): it flags a top-level symbol when no
//! indexed relation points at its *name*. It cannot prove the code is unreachable.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::i18n::{t, Lang};
use super::render::{
    escape, href, link, page, pager, review_form, select, state_chip, status_counts,
    status_matches, status_tabs, text_input, Req, Response, Section,
};
use super::reviews::{Kind, State};
use super::source;
use super::view_duplicates::history_html;

const PAGE: usize = 50;

fn kind_label(lang: Lang, kind: &str) -> String {
    match kind {
        "function" => t(lang, "Función").to_string(),
        "class" => t(lang, "Clase").to_string(),
        "struct" => t(lang, "Estructura").to_string(),
        "interface" => t(lang, "Interfaz").to_string(),
        "enum" => t(lang, "Enumeración").to_string(),
        "trait" => "Trait".to_string(),
        "type_alias" => t(lang, "Alias de tipo").to_string(),
        other => other.to_string(),
    }
}

/// Reasons to doubt a candidate, from rules on its language and path. These are
/// reminders of what static analysis cannot see, not detector output.
fn cautions(lang: Lang, language: &str, path: &str, kind: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if matches!(
        language,
        "python" | "javascript" | "typescript" | "php" | "ruby" | "lua" | "bash" | "powershell"
    ) {
        out.push(t(lang, "Lenguaje dinámico: puede usarse por nombre en tiempo de ejecución (reflexión, getattr, importaciones dinámicas)."));
    }
    let file = path.rsplit('/').next().unwrap_or(path);
    if matches!(
        file,
        "lib.rs" | "mod.rs" | "main.rs" | "__init__.py" | "index.js" | "index.ts" | "index.tsx"
    ) || path.contains("/bin/")
        || path.contains("/examples/")
        || path.starts_with("examples/")
    {
        out.push(t(lang, "Archivo de entrada o de exportación: puede formar parte de una API pública o de un punto de entrada."));
    }
    if matches!(kind, "class" | "interface" | "trait" | "struct" | "enum") {
        out.push(t(lang, "Un tipo puede usarse solo por herencia, serialización o inyección de dependencias, que el índice no resuelve."));
    }
    out.push(t(lang, "Código generado, referencias desde otros lenguajes o desde otros repositorios tampoco se ven en el índice."));
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
    ) {
        return Response::error(400);
    }
    let (language, kind, directory) = (req.param("lang"), req.param("kind"), req.param("dir"));
    let query = req.param("q").to_lowercase();
    let filtered: Vec<usize> = (0..site.dead.len())
        .filter(|id| {
            let entry = &site.dead[*id].entry;
            (language.is_empty() || entry.language == language)
                && (kind.is_empty() || entry.kind == kind)
                && (directory.is_empty()
                    || entry.relative_path == directory
                    || entry
                        .relative_path
                        .starts_with(&format!("{}/", directory.trim_end_matches('/'))))
                && (query.is_empty()
                    || entry.name.to_lowercase().contains(&query)
                    || entry.relative_path.to_lowercase().contains(&query))
        })
        .collect();
    let states: Vec<State> = filtered.iter().map(|id| site.dead_state(*id)).collect();
    let counts = status_counts(states.iter());
    let shown: Vec<(usize, &State)> = filtered
        .iter()
        .copied()
        .zip(&states)
        .filter(|(_, state)| status_matches(status, state))
        .collect();

    let languages: BTreeSet<_> = site
        .dead
        .iter()
        .map(|d| d.entry.language.as_str())
        .collect();
    let kinds: BTreeSet<_> = site.dead.iter().map(|d| d.entry.kind.as_str()).collect();
    let mut language_options = vec![("", req.t("Todos").to_string())];
    language_options.extend(languages.into_iter().map(|l| (l, l.to_string())));
    let mut kind_options = vec![("", req.t("Todos").to_string())];
    kind_options.extend(kinds.into_iter().map(|k| (k, kind_label(req.lang, k))));

    let mut body = format!("<p class=\"intro\">{}</p>", req.t("Símbolos a los que <strong>ningún elemento del índice hace referencia por nombre</strong>. Son candidatos a revisar, no código probadamente inalcanzable."));
    let _ = write!(body, "<details class=\"notice\"><summary>{}</summary><ul><li>{}</li><li>{}</li><li>{}</li><li>{}</li></ul></details>",
        req.t("Qué significa y qué no ve este análisis"),
        req.t("<strong>Posiblemente sin uso</strong>: el nombre no aparece como destino de ninguna llamada, importación, herencia o referencia indexada."),
        req.t("No hay ninguna categoría «inalcanzable demostrado»: este detector no lo comprueba."),
        req.t("No se ven: reflexión, importaciones dinámicas, código generado, puntos de entrada de frameworks (rutas, manejadores, decoradores), API pública usada desde fuera del repositorio, ni referencias entre lenguajes."),
        req.t("Se excluyen: métodos, <code>main</code> y símbolos con aspecto de prueba."),
    );
    let _ = write!(
        body,
        "<form action=\"muerto\" method=\"get\" class=\"filters\" role=\"search\" data-filter-form>{}{}{}{}<input type=\"hidden\" name=\"status\" value=\"{}\"><button>{}</button></form>",
        text_input("q", req.t("Buscar"), req.param("q"), req.t("nombre o ruta")),
        select("lang", req.t("Lenguaje"), &language_options, language),
        select("kind", req.t("Tipo de símbolo"), &kind_options, kind),
        text_input("dir", req.t("Carpeta"), directory, req.t("p. ej. crates/mct-cli")),
        escape(status),
        req.t("Filtrar"),
    );
    let base = [
        ("q", req.param("q")),
        ("lang", language),
        ("kind", kind),
        ("dir", directory),
    ];
    body.push_str(&status_tabs(req, "muerto", &base, status, &counts));
    let mut paging: Vec<(&str, &str)> = base.to_vec();
    paging.push(("status", status));
    let (start, pagination) = pager(req, "muerto", &paging, "page", shown.len(), PAGE);
    if shown.is_empty() {
        let _ = write!(
            body,
            "<p class=\"empty\">{}</p>",
            req.t("No hay candidatos con estos filtros.")
        );
    } else {
        let _ = write!(body, "<div class=\"table-wrap\"><table class=\"findings\"><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            req.t("Símbolo"), req.t("Tipo"), req.t("Lenguaje"), req.t("Ubicación"), req.t("Estado"), req.t("Decisión"));
        for (id, state) in shown.iter().skip(start).take(PAGE) {
            let dead = &site.dead[*id];
            let entry = &dead.entry;
            let end = entry.end_line.unwrap_or(entry.line);
            let _ = write!(
                body,
                "<tr data-search=\"{}\"><td><a href=\"{}\"><code>{}</code></a>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&format!("{} {}", entry.name, entry.relative_path).to_lowercase()),
                href("simbolo", &[("k", &dead.identity)]),
                escape(&entry.name),
                entry.parent.as_deref().map_or(String::new(), |p| format!(
                    " <span class=\"muted\">{}</span>",
                    req.tf("en {0}", &[&escape(p)])
                )),
                escape(&kind_label(req.lang, &entry.kind)),
                escape(&entry.language),
                link(
                    "archivo",
                    &[
                        ("path", &entry.relative_path),
                        ("from", &entry.line.to_string()),
                        ("to", &end.to_string())
                    ],
                    &format!("{}:{}", entry.relative_path, entry.line)
                ),
                state_chip(req, state),
                review_form(req, Kind::Dead, &dead.identity, state),
            );
        }
        body.push_str("</tbody></table></div>");
    }
    body.push_str(&pagination);
    page(req, req.t("Código muerto"), Section::Dead, &body)
}

pub(super) fn detail(req: &Req<'_>) -> Response {
    let site = req.site;
    let Some(id) = site.dead_ids.get(req.param("k")).copied() else {
        return Response::error(404);
    };
    let dead = &site.dead[id];
    let entry = &dead.entry;
    let state = site.dead_state(id);
    let end = entry.end_line.unwrap_or(entry.line);
    let mut body = format!(
        "<p>{}</p><p>{} <span class=\"badge\">{}</span> <span class=\"badge\">{}</span></p><dl class=\"facts\"><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd><code>{}</code>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd></dl>",
        link("muerto", &[], req.t("← Todos los candidatos")),
        state_chip(req, &state),
        escape(&entry.language),
        escape(&kind_label(req.lang, &entry.kind)),
        req.t("Ubicación"),
        link(
            "archivo",
            &[
                ("path", &entry.relative_path),
                ("from", &entry.line.to_string()),
                ("to", &end.to_string())
            ],
            &format!("{}:{}–{}", entry.relative_path, entry.line, end)
        ),
        req.t("Símbolo"),
        escape(&entry.name),
        entry.parent.as_deref().map_or(String::new(), |p| format!(
            " {}",
            req.tf("en <code>{0}</code>", &[&escape(p)])
        )),
        req.t("Por qué se marcó"),
        req.tf(
            "Ninguna relación del índice (llamada, referencia, importación, herencia) tiene como destino el nombre <code>{0}</code>, y no es <code>main</code> ni parece una prueba.",
            &[&escape(&entry.name)]
        ),
        req.t("Referencias conocidas"),
        req.t("0 por nombre en todo el proyecto. Es la condición del detector, no una comprobación de uso real."),
        req.t("Confianza"),
        req.t("El detector no calcula una puntuación. Trátalo como una pista."),
    );
    body.push_str(&review_form(req, Kind::Dead, &dead.identity, &state));
    let _ = write!(body, "<h2>{}</h2><ul>", req.t("Antes de borrar, comprueba"));
    for caution in cautions(req.lang, &entry.language, &entry.relative_path, &entry.kind) {
        let _ = write!(body, "<li>{caution}</li>");
    }
    body.push_str("</ul>");
    let homonyms: Vec<_> = site
        .navigation
        .symbols
        .iter()
        .filter(|s| {
            s.name == entry.name
                && !(s.relative_path == entry.relative_path && s.line == entry.line)
        })
        .take(10)
        .collect();
    if !homonyms.is_empty() {
        let _ = write!(
            body,
            "<h2>{}</h2><ul>",
            req.t("Otros símbolos con el mismo nombre")
        );
        for other in homonyms {
            let _ = write!(
                body,
                "<li>{} <span class=\"muted\">{}</span></li>",
                link(
                    "archivo",
                    &[
                        ("path", &other.relative_path),
                        ("from", &other.line.to_string())
                    ],
                    &format!("{}:{}", other.relative_path, other.line)
                ),
                escape(&other.kind)
            );
        }
        body.push_str("</ul>");
    }
    let _ = write!(body, "<h2>{}</h2>", req.t("Código"));
    match source::excerpt(&site.root, &entry.relative_path, entry.line, end) {
        Some(excerpt) => {
            let _ = write!(
                body,
                "<div class=\"code\" role=\"region\" tabindex=\"0\" aria-label=\"{}\">",
                req.t("Código")
            );
            for (offset, line) in excerpt.lines.iter().enumerate() {
                let _ = write!(
                    body,
                    "<div class=\"ln\"><span class=\"n\">{}</span><code>{}</code></div>",
                    excerpt.first + offset as u32,
                    escape(line)
                );
            }
            body.push_str("</div>");
            if excerpt.truncated {
                let _ = write!(
                    body,
                    "<p class=\"muted\">{}</p>",
                    req.t("Vista recortada; abre el archivo para ver el resto.")
                );
            }
        }
        None => {
            let _ = write!(
                body,
                "<p class=\"warning\">{}</p>",
                req.t("No se pudo leer el archivo: cambió o ya no existe. Recarga el análisis en Estado.")
            );
        }
    }
    body.push_str(&history_html(req, Kind::Dead, &dead.identity));
    page(req, &entry.name, Section::Dead, &body)
}
