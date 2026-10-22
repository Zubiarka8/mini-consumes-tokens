//! "Obsidian": project statistics, explorer, generated notes and their graph.
//! Nothing here needs the Obsidian application; the notes are the vault files.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

use super::i18n::{t, Lang};
use super::markdown;
use super::notes::Note;
use super::render::{
    escape, href, link, page, pager, select, text_input, Req, Response, Section, PAGE_SIZE,
};
use super::reviews::State;
use super::source;
use super::stats::{bucket, Bucket};

const WINDOW: u32 = 120;
const MAX_TREE_DIRECTORIES: usize = 1500;
const MAX_GRAPH_NODES: usize = 150;

fn subnav(req: &Req<'_>, active: &str) -> String {
    let mut out = String::from("<nav class=\"tabs\" aria-label=\"Obsidian\">");
    for (route, label) in [
        ("obsidian", req.t("Resumen")),
        ("explorar", req.t("Explorador de código")),
        ("notas", req.t("Notas")),
        ("grafo", req.t("Grafo")),
    ] {
        let current = if route == active {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(out, "<a href=\"{route}\"{current}>{label}</a>");
    }
    out.push_str("</nav>");
    out
}

fn card(route: &str, params: &[(&str, &str)], eyebrow: &str, value: &str, text: &str) -> String {
    format!(
        "<a class=\"card\" href=\"{}\"><span class=\"eyebrow\">{eyebrow}</span><strong>{value}</strong><span>{text}</span></a>",
        href(route, params)
    )
}

fn count_cell(count: usize, route: &str, params: &[(&str, &str)]) -> String {
    if count == 0 {
        "<td class=\"num muted\">0</td>".to_string()
    } else {
        format!(
            "<td class=\"num\"><a href=\"{}\">{count}</a></td>",
            href(route, params)
        )
    }
}

pub(super) fn dashboard(req: &Req<'_>) -> Response {
    let site = req.site;
    let stats = &site.stats;
    let mut body = subnav(req, "obsidian");
    let _ = write!(
        body,
        "<p class=\"intro\">{}</p><section class=\"cards\">",
        req.t("Resumen del proyecto a partir del índice. Cada cifra abre la lista que la compone.")
    );
    body.push_str(&card(
        "explorar",
        &[],
        req.t("Archivos indexados"),
        &stats.files.to_string(),
        req.t("Ver el árbol de carpetas →"),
    ));
    body.push_str(&card(
        "explorar",
        &[],
        req.t("Directorios"),
        &stats.directories.to_string(),
        req.t("Con al menos un archivo indexado →"),
    ));
    body.push_str(&card(
        "explorar",
        &[],
        req.t("Lenguajes"),
        &stats.languages.len().to_string(),
        req.t("Detectados en el índice →"),
    ));
    for bucket in Bucket::ALL {
        body.push_str(&card(
            "explorar",
            &[("kind", bucket.value())],
            bucket.short_label(req.lang),
            &stats.count(bucket).to_string(),
            req.t("Ver la lista →"),
        ));
    }
    body.push_str(&card(
        "notas",
        &[],
        req.t("Notas generadas"),
        &site.notes.list.len().to_string(),
        req.t("Bóveda de Obsidian →"),
    ));
    body.push_str(&card(
        "grafo",
        &[],
        req.t("Relaciones"),
        &format!("{} + {}", stats.imports, site.notes.link_count),
        req.t("importaciones entre archivos + enlaces entre notas →"),
    ));
    body.push_str("</section>");

    let unsupported = &site.status.unsupported_languages;
    let _ = write!(
        body,
        "<p class=\"notice\">{}</p>",
        req.tf(
            "{0} símbolos indexados en total. Archivos en disco: <strong>no medido</strong> (el índice solo guarda lo que se indexa; lo ignorado por <code>.mctignore</code> y los recursos 3D no cuentan aquí). Sin plugin de lenguaje: {1}. Archivos con errores de sintaxis: {2}.",
            &[
                &stats.total_symbols(),
                &if unsupported.is_empty() {
                    req.t("ninguno").to_string()
                } else {
                    escape(&unsupported.join(", "))
                },
                &site.status.syntax_errors.len()
            ]
        )
    );

    let _ = write!(
        body,
        "<h2>{}</h2><div class=\"table-wrap\"><table><thead><tr><th>{}</th><th>{}</th>",
        req.t("Por lenguaje"),
        req.t("Lenguaje"),
        req.t("Archivos")
    );
    for bucket in Bucket::ALL {
        let _ = write!(body, "<th>{}</th>", bucket.short_label(req.lang));
    }
    body.push_str("</tr></thead><tbody>");
    for (language, data) in &stats.languages {
        let _ = write!(body, "<tr><td>{}</td>", escape(language));
        body.push_str(&count_cell(data.files, "explorar", &[("lang", language)]));
        for bucket in Bucket::ALL {
            body.push_str(&count_cell(
                data.count(bucket),
                "explorar",
                &[("lang", language), ("kind", bucket.value())],
            ));
        }
        body.push_str("</tr>");
    }
    let _ = write!(
        body,
        "</tbody><tfoot><tr><th>Total</th><td class=\"num\">{}</td>",
        stats.files
    );
    for bucket in Bucket::ALL {
        let _ = write!(body, "<td class=\"num\">{}</td>", stats.count(bucket));
    }
    body.push_str("</tr></tfoot></table></div>");
    let _ = write!(body, "<details class=\"notice\"><summary>{}</summary><ul><li>{}</li><li>{}</li><li>{}</li><li>{}</li><li>{}</li></ul></details>",
        req.t("Cómo se cuenta"),
        req.t("<strong>Archivo</strong>: archivo indexado que no es un recurso (<code>asset</code>), contado bajo el lenguaje que le asignó el indexador. Un archivo con varios lenguajes cuenta una vez."),
        req.t("<strong>Función</strong>: símbolo <code>function</code> sin padre. <strong>Método</strong>: símbolo <code>method</code>, o <code>function</code> con padre."),
        req.t("<strong>Clase</strong>: símbolo <code>class</code>. <strong>Tipos</strong>: <code>struct</code>, <code>enum</code>, <code>trait</code>, <code>interface</code> y <code>type_alias</code>. <strong>Otros</strong>: cualquier otro tipo de símbolo (módulos, variables, constantes, reglas CSS…)."),
        req.t("<strong>Directorio</strong>: cuenta si contiene algún archivo indexado a cualquier profundidad."),
        req.t("Los símbolos de archivos excluidos por <code>.mctignore</code> no se cuentan."),
    );
    page(
        req,
        req.t("Obsidian: inteligencia del proyecto"),
        Section::Obsidian,
        &body,
    )
}

struct Child {
    directory: bool,
    files: usize,
    symbols: usize,
}

fn tree(req: &Req<'_>, current: &str, lang: &str) -> String {
    let site = req.site;
    let mut directories: BTreeSet<&str> = BTreeSet::new();
    for file in &site.navigation.files {
        if !lang.is_empty() && file.language != lang {
            continue;
        }
        let mut path = file.relative_path.as_str();
        while let Some((parent, _)) = path.rsplit_once('/') {
            if !directories.insert(parent) {
                break;
            }
            path = parent;
        }
    }
    let mut out = format!(
        "<nav class=\"tree\" aria-label=\"{}\"><ul>",
        req.t("Árbol de carpetas")
    );
    let _ = write!(
        out,
        "<li>{}</li>",
        link("explorar", &[("lang", lang)], req.t("(raíz)"))
    );
    // Component-wise order keeps every folder's children right after it.
    let mut ordered: Vec<&str> = directories.iter().copied().collect();
    ordered.sort_by(|a, b| a.split('/').cmp(b.split('/')));
    let parents: BTreeSet<&str> = ordered
        .iter()
        .filter_map(|d| d.rsplit_once('/').map(|(parent, _)| parent))
        .collect();
    let mut open: Vec<&str> = Vec::new();
    for (emitted, directory) in ordered.iter().enumerate() {
        if emitted >= MAX_TREE_DIRECTORIES {
            let _ = write!(
                out,
                "<li class=\"muted\">{}</li>",
                req.t("… árbol recortado; usa la búsqueda.")
            );
            break;
        }
        // Close the folders we have left, then open the new one.
        while let Some(last) = open.last() {
            if directory.starts_with(&format!("{last}/")) {
                break;
            }
            out.push_str("</ul></details></li>");
            open.pop();
        }
        let shown = directory.rsplit('/').next().unwrap_or(directory);
        let is_open = current == *directory || current.starts_with(&format!("{directory}/"));
        let has_children = parents.contains(directory);
        let label = if current == *directory {
            format!("<strong>{}</strong>", escape(shown))
        } else {
            link("explorar", &[("dir", directory), ("lang", lang)], shown)
        };
        if has_children {
            let _ = write!(
                out,
                "<li><details{}><summary>{label}</summary><ul>",
                if is_open { " open" } else { "" }
            );
            open.push(directory);
        } else {
            let _ = write!(out, "<li>{label}</li>");
        }
    }
    while open.pop().is_some() {
        out.push_str("</ul></details></li>");
    }
    out.push_str("</ul></nav>");
    out
}

pub(super) fn explorer(req: &Req<'_>) -> Response {
    let site = req.site;
    let (directory, lang, q) = (req.param("dir"), req.param("lang"), req.param("q"));
    let kind_param = req.param("kind");
    let kind = if kind_param.is_empty() {
        None
    } else {
        match Bucket::parse(kind_param) {
            Some(bucket) => Some(bucket),
            None => return Response::error(400),
        }
    };
    let query = q.to_lowercase();
    let languages: Vec<_> = site.stats.languages.keys().collect();
    let mut language_options = vec![("", req.t("Todos").to_string())];
    language_options.extend(languages.iter().map(|l| (l.as_str(), (*l).clone())));
    let kind_options: Vec<(&str, String)> = std::iter::once(("", req.t("Archivos").to_string()))
        .chain(
            Bucket::ALL
                .iter()
                .map(|b| (b.value(), b.label(req.lang).to_string())),
        )
        .collect();
    let mut body = subnav(req, "explorar");
    let _ = write!(
        body,
        "<form action=\"explorar\" method=\"get\" class=\"filters\" role=\"search\" data-filter-form><input type=\"hidden\" name=\"dir\" value=\"{}\">{}{}{}<button>{}</button></form>",
        escape(directory),
        text_input("q", req.t("Buscar"), q, req.t("ruta o nombre de símbolo")),
        select("lang", req.t("Lenguaje"), &language_options, lang),
        select("kind", req.t("Mostrar"), &kind_options, kind_param),
        req.t("Filtrar"),
    );
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    body.push_str("<div class=\"split\">");
    body.push_str(&tree(req, directory, lang));
    body.push_str("<div class=\"split-main\">");

    body.push_str("<p class=\"crumbs\">");
    body.push_str(&link(
        "explorar",
        &[("lang", lang), ("kind", kind_param), ("q", q)],
        req.t("raíz"),
    ));
    let mut so_far = String::new();
    for part in directory.split('/').filter(|p| !p.is_empty()) {
        if !so_far.is_empty() {
            so_far.push('/');
        }
        so_far.push_str(part);
        let _ = write!(
            body,
            " / {}",
            link(
                "explorar",
                &[
                    ("dir", &so_far),
                    ("lang", lang),
                    ("kind", kind_param),
                    ("q", q)
                ],
                part
            )
        );
    }
    body.push_str("</p>");
    let base = [
        ("dir", directory),
        ("lang", lang),
        ("kind", kind_param),
        ("q", q),
    ];

    if let Some(bucket_filter) = kind {
        let shown: Vec<_> = site
            .navigation
            .symbols
            .iter()
            .filter(|s| {
                bucket(&s.kind, s.parent.is_some()) == bucket_filter
                    && (lang.is_empty() || s.language == lang)
                    && (directory.is_empty() || s.relative_path.starts_with(&prefix))
                    && (query.is_empty()
                        || s.name.to_lowercase().contains(&query)
                        || s.relative_path.to_lowercase().contains(&query))
            })
            .collect();
        let (start, pagination) = pager(req, "explorar", &base, "page", shown.len(), PAGE_SIZE);
        let _ = write!(
            body,
            "<h2>{} <span class=\"muted\">({})</span></h2>",
            bucket_filter.label(req.lang),
            shown.len()
        );
        let _ = write!(body, "<div class=\"table-wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            req.t("Símbolo"), req.t("Tipo"), req.t("Lenguaje"), req.t("Ubicación"));
        for symbol in shown.iter().skip(start).take(PAGE_SIZE) {
            let end = symbol.end_line.unwrap_or(symbol.line);
            let _ = write!(
                body,
                "<tr data-search=\"{}\"><td><code>{}</code>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&format!("{} {}", symbol.name, symbol.relative_path).to_lowercase()),
                escape(&symbol.name),
                symbol.parent.as_deref().map_or(String::new(), |p| format!(
                    " <span class=\"muted\">{}</span>",
                    req.tf("en {0}", &[&escape(p)])
                )),
                escape(&symbol.kind),
                escape(&symbol.language),
                link(
                    "archivo",
                    &[
                        ("path", &symbol.relative_path),
                        ("from", &symbol.line.to_string()),
                        ("to", &end.to_string())
                    ],
                    &format!("{}:{}", symbol.relative_path, symbol.line)
                ),
            );
        }
        if shown.is_empty() {
            let _ = write!(
                body,
                "<tr><td colspan=\"4\" class=\"empty\">{}</td></tr>",
                req.t("No hay símbolos con estos filtros.")
            );
        }
        body.push_str("</tbody></table></div>");
        body.push_str(&pagination);
    } else {
        let mut children: BTreeMap<String, Child> = BTreeMap::new();
        for file in &site.navigation.files {
            let Some(rest) = file.relative_path.strip_prefix(&prefix) else {
                continue;
            };
            if !lang.is_empty() && file.language != lang {
                continue;
            }
            let ids = site.symbols.get(&file.relative_path);
            let matches = query.is_empty()
                || file.relative_path.to_lowercase().contains(&query)
                || ids.into_iter().flatten().any(|id| {
                    site.navigation.symbols[*id]
                        .name
                        .to_lowercase()
                        .contains(&query)
                });
            if !matches {
                continue;
            }
            let (name, directory_entry) = rest
                .split_once('/')
                .map_or((rest, false), |(dir, _)| (dir, true));
            let child = children.entry(format!("{prefix}{name}")).or_insert(Child {
                directory: directory_entry,
                files: 0,
                symbols: 0,
            });
            child.files += 1;
            child.symbols += ids.map_or(0, Vec::len);
        }
        let (start, pagination) = pager(req, "explorar", &base, "page", children.len(), PAGE_SIZE);
        body.push_str("<ul class=\"entries\">");
        for (path, child) in children.iter().skip(start).take(PAGE_SIZE) {
            let name = path.rsplit('/').next().unwrap_or(path);
            if child.directory {
                let _ = write!(
                    body,
                    "<li data-search=\"{}\"><span>📁 {}</span><span class=\"muted\">{}</span></li>",
                    escape(&path.to_lowercase()),
                    link(
                        "explorar",
                        &[("dir", path), ("lang", lang), ("q", q)],
                        &format!("{name}/")
                    ),
                    req.tf(
                        "{0} archivos · {1} símbolos",
                        &[&child.files, &child.symbols]
                    )
                );
            } else {
                let groups = site.active_groups(path);
                let language = site
                    .navigation
                    .files
                    .binary_search_by(|f| f.relative_path.as_str().cmp(path))
                    .ok()
                    .map_or("", |i| site.navigation.files[i].language.as_str());
                let _ = write!(
                    body,
                    "<li data-search=\"{}\"><span>{} <span class=\"badge\">{}</span></span><span class=\"muted\">{}{}</span></li>",
                    escape(&path.to_lowercase()),
                    link("archivo", &[("path", path)], name),
                    escape(language),
                    req.tf("{0} símbolos", &[&child.symbols]),
                    if groups > 0 {
                        format!(
                            " · {}",
                            req.tf("{0} grupos duplicados pendientes", &[&groups])
                        )
                    } else {
                        String::new()
                    }
                );
            }
        }
        if children.is_empty() {
            let _ = write!(
                body,
                "<li class=\"empty\">{}</li>",
                req.t("No hay entradas con estos filtros.")
            );
        }
        body.push_str("</ul>");
        body.push_str(&pagination);
    }
    body.push_str("</div></div>");
    page(req, req.t("Explorador de código"), Section::Obsidian, &body)
}

pub(super) fn file(req: &Req<'_>) -> Response {
    let site = req.site;
    let path = req.param("path");
    let Ok(id) = site
        .navigation
        .files
        .binary_search_by(|file| file.relative_path.as_str().cmp(path))
    else {
        return Response::error(404);
    };
    let file = &site.navigation.files[id];
    let directory = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let from: u32 = req.param("from").parse().unwrap_or(0);
    let to: u32 = req.param("to").parse().unwrap_or(from);
    let start = if from == 0 {
        1
    } else {
        from.saturating_sub(3).max(1)
    };
    let ids = site
        .symbols
        .get(path)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let note = site.notes.for_source(path);
    let dead: Vec<_> = site
        .dead
        .iter()
        .enumerate()
        .filter(|(_, d)| d.entry.relative_path == path)
        .collect();
    let group_ids = site.groups.get(path).map(Vec::as_slice).unwrap_or_default();

    let mut body = format!(
        "<p>{} · {}</p><dl class=\"facts\"><dt>{}</dt><dd><code>{}</code></dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd><code>{}</code></dd>",
        link("explorar", &[("dir", directory)], req.t("← Carpeta")),
        link("explorar", &[("q", path)], req.t("Buscar en el proyecto")),
        req.t("Ruta"),
        escape(path),
        req.t("Lenguaje"),
        escape(&file.language),
        req.t("Símbolos"),
        ids.len(),
        req.t("Huella del contenido indexado"),
        escape(&file.content_hash)
    );
    if let Some(note) = note {
        let _ = write!(
            body,
            "<dt>{}</dt><dd>{}</dd>",
            req.t("Nota de Obsidian"),
            link(
                "nota",
                &[("n", &site.notes.list[note].name)],
                &site.notes.list[note].title
            )
        );
    }
    if !group_ids.is_empty() {
        let pending = group_ids
            .iter()
            .filter(|i| site.duplicate_state(**i) == State::Pending)
            .count();
        let _ = write!(
            body,
            "<dt>{}</dt><dd>{}: {}</dd>",
            req.t("Duplicados"),
            link(
                "duplicados",
                &[("path", path), ("status", "all")],
                req.t("Ver grupos")
            ),
            req.tf("{0} grupos ({1} pendientes)", &[&group_ids.len(), &pending])
        );
    }
    if !dead.is_empty() {
        let _ = write!(
            body,
            "<dt>{}</dt><dd>{}</dd>",
            req.t("Código muerto"),
            link(
                "muerto",
                &[("q", path), ("status", "all")],
                &req.tf("{0} candidatos", &[&dead.len()])
            )
        );
    }
    body.push_str("</dl>");

    let _ = write!(body, "<h2>{}</h2>", req.t("Código"));
    let end_wanted = start + WINDOW - 1;
    match source::excerpt(&site.root, path, start, end_wanted) {
        Some(excerpt) => {
            let _ = write!(
                body,
                "<div class=\"code\" role=\"region\" tabindex=\"0\" aria-label=\"{}\">",
                req.t("Código")
            );
            for (offset, line) in excerpt.lines.iter().enumerate() {
                let number = excerpt.first + offset as u32;
                let class = if from != 0 && (from..=to.max(from)).contains(&number) {
                    "ln hl"
                } else {
                    "ln"
                };
                let _ = write!(body, "<div class=\"{class}\" id=\"L{number}\"><span class=\"n\">{number}</span><code>{}</code></div>", escape(line));
            }
            body.push_str("</div>");
            let last = excerpt.first + excerpt.lines.len() as u32 - 1;
            body.push_str("<p class=\"pagination\">");
            if start > 1 {
                let _ = write!(
                    body,
                    "{} · ",
                    link(
                        "archivo",
                        &[
                            ("path", path),
                            ("from", &start.saturating_sub(WINDOW).max(1).to_string())
                        ],
                        req.t("Tramo anterior")
                    )
                );
            }
            let _ = write!(
                body,
                "{}",
                req.tf("Líneas {0}–{1}", &[&excerpt.first, &last])
            );
            if excerpt.lines.len() as u32 == WINDOW {
                let _ = write!(
                    body,
                    " · {}",
                    link(
                        "archivo",
                        &[("path", path), ("from", &(last + 1).to_string())],
                        req.t("Tramo siguiente")
                    )
                );
            }
            body.push_str("</p>");
        }
        None => {
            let _ = write!(
                body,
                "<p class=\"warning\">{}</p>",
                req.t(
                    "No se pudo leer el archivo (cambió, es demasiado grande o no es texto UTF-8)."
                )
            );
        }
    }

    let _ = write!(body, "<h2>{}</h2>", req.t("Símbolos"));
    let (first, pagination) = pager(
        req,
        "archivo",
        &[
            ("path", path),
            ("from", req.param("from")),
            ("to", req.param("to")),
        ],
        "sp",
        ids.len(),
        PAGE_SIZE,
    );
    let _ = write!(body, "<div class=\"table-wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
        req.t("Símbolo"), req.t("Tipo"), req.t("Líneas"));
    for id in ids.iter().skip(first).take(PAGE_SIZE) {
        let symbol = &site.navigation.symbols[*id];
        let end = symbol.end_line.unwrap_or(symbol.line);
        let _ = write!(
            body,
            "<tr><td><code>{}</code>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&symbol.name),
            symbol.parent.as_deref().map_or(String::new(), |p| format!(
                " <span class=\"muted\">{}</span>",
                req.tf("en {0}", &[&escape(p)])
            )),
            escape(&symbol.kind),
            link(
                "archivo",
                &[
                    ("path", path),
                    ("from", &symbol.line.to_string()),
                    ("to", &end.to_string())
                ],
                &format!("{}–{end}", symbol.line)
            ),
        );
    }
    if ids.is_empty() {
        let _ = write!(
            body,
            "<tr><td colspan=\"3\" class=\"empty\">{}</td></tr>",
            req.t("Sin símbolos indexados.")
        );
    }
    body.push_str("</tbody></table></div>");
    body.push_str(&pagination);

    for (title, items) in [
        (
            req.t("Importa (resuelto en el índice)"),
            site.imports.get(path),
        ),
        (req.t("Importado por"), site.importers.get(path)),
    ] {
        let items = items.map(Vec::as_slice).unwrap_or_default();
        let _ = write!(
            body,
            "<h2>{title} <span class=\"muted\">({})</span></h2>",
            items.len()
        );
        if items.is_empty() {
            let _ = write!(body, "<p class=\"muted\">{}</p>", req.t("Ninguno."));
            continue;
        }
        body.push_str("<ul>");
        for target in items.iter().take(PAGE_SIZE) {
            let _ = write!(
                body,
                "<li>{}</li>",
                link("archivo", &[("path", target)], target)
            );
        }
        if items.len() > PAGE_SIZE {
            let _ = write!(
                body,
                "<li class=\"muted\">{}</li>",
                req.tf("y {0} más", &[&(items.len() - PAGE_SIZE)])
            );
        }
        body.push_str("</ul>");
    }
    page(req, path, Section::Obsidian, &body)
}

fn note_kind(lang: Lang, name: &str) -> (&'static str, &'static str) {
    if name == "index.md" {
        ("index", t(lang, "Índice"))
    } else if name.starts_with("directories/") {
        ("directories", t(lang, "Carpeta"))
    } else if name.starts_with("files/") {
        ("files", t(lang, "Archivo"))
    } else {
        ("other", t(lang, "Otra"))
    }
}

pub(super) fn notes(req: &Req<'_>) -> Response {
    let site = req.site;
    let (q, kind) = (req.param("q"), req.param("kind"));
    if !matches!(kind, "" | "index" | "directories" | "files" | "other") {
        return Response::error(400);
    }
    let mut body = subnav(req, "notas");
    if site.notes.list.is_empty() {
        let _ = write!(body, "<p class=\"empty\">{}</p>", req.t("La bóveda de Obsidian no está generada. Quita <code>--no-obsidian</code> o ejecuta <code>mct-cli init</code>."));
        return page(req, req.t("Notas"), Section::Obsidian, &body);
    }
    let kind_options = [
        ("", req.t("Todas").to_string()),
        ("files", req.t("Archivo").to_string()),
        ("directories", req.t("Carpeta").to_string()),
        ("index", req.t("Índice").to_string()),
    ];
    let _ = write!(
        body,
        "<form action=\"notas\" method=\"get\" class=\"filters\" role=\"search\" data-filter-form>{}{}<button>{}</button></form>",
        text_input("q", req.t("Buscar en títulos y contenido"), q, req.t("palabra o ruta")),
        select("kind", req.t("Tipo de nota"), &kind_options, kind),
        req.t("Buscar"),
    );
    let ids: Vec<usize> = if q.is_empty() {
        (0..site.notes.list.len()).collect()
    } else {
        site.notes.search(q, 5000)
    };
    let ids: Vec<usize> = ids
        .into_iter()
        .filter(|id| kind.is_empty() || note_kind(req.lang, &site.notes.list[*id].name).0 == kind)
        .collect();
    let (start, pagination) = pager(
        req,
        "notas",
        &[("q", q), ("kind", kind)],
        "page",
        ids.len(),
        PAGE_SIZE,
    );
    body.push_str("<ul class=\"entries\">");
    for id in ids.iter().skip(start).take(PAGE_SIZE) {
        let note = &site.notes.list[*id];
        let _ = write!(
            body,
            "<li data-search=\"{}\"><span>{}</span><span class=\"muted\">{} · {}</span></li>",
            escape(&format!("{} {}", note.title, note.name).to_lowercase()),
            link("nota", &[("n", &note.name)], &note.title),
            note_kind(req.lang, &note.name).1,
            req.tf(
                "{0} enlaces · {1} entrantes",
                &[&note.links.len(), &site.notes.backlinks[*id].len()]
            )
        );
    }
    if ids.is_empty() {
        let _ = write!(
            body,
            "<li class=\"empty\">{}</li>",
            req.t("No hay notas con estos filtros.")
        );
    }
    body.push_str("</ul>");
    body.push_str(&pagination);
    page(req, req.t("Notas"), Section::Obsidian, &body)
}

fn note_list(req: &Req<'_>, ids: &[usize]) -> String {
    if ids.is_empty() {
        return format!("<p class=\"muted\">{}</p>", req.t("Ninguna."));
    }
    let mut out = String::from("<ul>");
    for id in ids.iter().take(PAGE_SIZE) {
        let note: &Note = &req.site.notes.list[*id];
        let _ = write!(
            out,
            "<li>{}</li>",
            link("nota", &[("n", &note.name)], &note.title)
        );
    }
    if ids.len() > PAGE_SIZE {
        let _ = write!(
            out,
            "<li class=\"muted\">{}</li>",
            req.tf("y {0} más", &[&(ids.len() - PAGE_SIZE)])
        );
    }
    out.push_str("</ul>");
    out
}

pub(super) fn note(req: &Req<'_>) -> Response {
    let site = req.site;
    let Some(id) = site.notes.get(req.param("n")) else {
        return Response::error(404);
    };
    let note = &site.notes.list[id];
    let (_, text) = markdown::split_front_matter(&note.text);
    let resolve = |target: &str| {
        site.notes
            .resolve(&note.name, target.split('#').next().unwrap_or(""))
            .map(|found| href("nota", &[("n", &site.notes.list[found].name)]))
            .map(|escaped| escaped.replace("&amp;", "&"))
    };
    let mut body = subnav(req, "notas");
    let _ = write!(
        body,
        "<p>{} · {}</p>",
        link("notas", &[], req.t("← Todas las notas")),
        link("grafo", &[("n", &note.name)], req.t("Ver en el grafo"))
    );
    if let Some(source) = &note.source {
        if site
            .navigation
            .files
            .binary_search_by(|f| f.relative_path.as_str().cmp(source))
            .is_ok()
        {
            let _ = write!(
                body,
                "<p>{}</p>",
                req.tf(
                    "Describe el archivo {0}",
                    &[&link("archivo", &[("path", source)], source)]
                )
            );
        }
    }
    body.push_str("<article class=\"note\">");
    if note.text.is_empty() {
        let _ = write!(
            body,
            "<p class=\"warning\">{}</p>",
            req.t("No se pudo leer esta nota.")
        );
    } else {
        body.push_str(&markdown::render(text, &resolve));
    }
    body.push_str("</article>");
    let _ = write!(
        body,
        "<h2>{} <span class=\"muted\">({})</span></h2>{}",
        req.t("Enlaza a"),
        note.links.len(),
        note_list(req, &note.links)
    );
    let backlinks = &site.notes.backlinks[id];
    let _ = write!(
        body,
        "<h2>{} <span class=\"muted\">({})</span></h2>{}",
        req.t("Enlazada desde"),
        backlinks.len(),
        note_list(req, backlinks)
    );
    page(req, &note.title, Section::Obsidian, &body)
}

pub(super) fn graph(req: &Req<'_>) -> Response {
    let site = req.site;
    let notes = &site.notes;
    let mut body = subnav(req, "grafo");
    if notes.list.is_empty() {
        let _ = write!(
            body,
            "<p class=\"empty\">{}</p>",
            req.t("La bóveda de Obsidian no está generada, así que no hay grafo.")
        );
        return page(req, req.t("Grafo de notas"), Section::Obsidian, &body);
    }
    let focus = if req.param("n").is_empty() {
        None
    } else {
        notes.get(req.param("n"))
    };
    if !req.param("n").is_empty() && focus.is_none() {
        return Response::error(404);
    }
    let degree = |id: usize| notes.list[id].links.len() + notes.backlinks[id].len();
    let mut nodes: Vec<usize> = Vec::new();
    match focus {
        Some(start) => {
            let mut seen = BTreeSet::from([start]);
            let mut queue = VecDeque::from([(start, 0_u8)]);
            while let Some((id, depth)) = queue.pop_front() {
                nodes.push(id);
                if depth == 2 {
                    continue;
                }
                for next in notes.list[id].links.iter().chain(&notes.backlinks[id]) {
                    if nodes.len() + queue.len() < MAX_GRAPH_NODES && seen.insert(*next) {
                        queue.push_back((*next, depth + 1));
                    }
                }
            }
        }
        None => {
            nodes = (0..notes.list.len()).filter(|id| degree(*id) > 0).collect();
            nodes.sort_by_key(|id| (std::cmp::Reverse(degree(*id)), *id));
            nodes.truncate(MAX_GRAPH_NODES);
        }
    }
    let index: BTreeMap<usize, usize> = nodes.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut edges = Vec::new();
    for id in &nodes {
        for target in &notes.list[*id].links {
            if let (Some(a), Some(b)) = (index.get(id), index.get(target)) {
                edges.push([*a, *b]);
            }
        }
    }
    let data = serde_json::json!({
        "nodes": nodes.iter().map(|id| serde_json::json!({
            "t": notes.list[*id].title,
            "h": href("nota", &[("n", &notes.list[*id].name)]).replace("&amp;", "&"),
            "f": Some(*id) == focus,
            "d": degree(*id),
        })).collect::<Vec<_>>(),
        "edges": edges,
    });
    let _ = write!(
        body,
        "<p class=\"intro\">{}</p><p class=\"muted\">{} {}</p>",
        match focus {
            Some(id) => req.tf(
                "Vecindad de «{0}» (hasta 2 saltos).",
                &[&escape(&notes.list[id].title)]
            ),
            None => req.tf(
                "Las {0} notas más conectadas de {1}.",
                &[&nodes.len(), &notes.list.len()]
            ),
        },
        req.tf(
            "{0} notas y {1} enlaces mostrados. Cada punto es una nota; arrastra para mover, haz clic para abrirla.",
            &[&nodes.len(), &edges.len()]
        ),
        if focus.is_some() {
            link("grafo", &[], req.t("Ver el grafo general"))
        } else {
            String::new()
        }
    );
    let _ = write!(
        body,
        "<div class=\"graph\" id=\"graph\" data-graph=\"{}\" role=\"img\" aria-label=\"{}\"><canvas></canvas></div>",
        escape(&data.to_string()),
        req.t("Grafo de notas; la lista de abajo contiene los mismos datos")
    );
    let _ = write!(body, "<h2>{}</h2><ul>", req.t("Lista de notas"));
    for id in &nodes {
        let _ = write!(
            body,
            "<li>{} <span class=\"muted\">{}</span></li>",
            link(
                "nota",
                &[("n", &notes.list[*id].name)],
                &notes.list[*id].title
            ),
            req.tf("{0} enlaces", &[&degree(*id)])
        );
    }
    body.push_str("</ul>");
    page(req, req.t("Grafo de notas"), Section::Obsidian, &body)
}
