// 09 · Obsidian: independent knowledge workspace (notes | list | viewer), read-only
import { h, clear, api, cancelApi, isAbort, patchQuery, onQuery, parseHash, listParam, fmt, ago, copy, hrefTo, forget, basename, toast } from "./core.js";
import { icon, pageHead, pagedLinks, tabsBar, badge, pager, searchBox, filterGroup, activeChips, noResults, loading, errorBox, note, emptyState, drawerButton, selectBox, dataTable } from "./components.js";
import { renderMarkdown } from "./markdown.js";
import { graphView } from "./graph.js";
import { t } from "./i18n.js";

const VIEWS = ["notas", "grafo", "etiquetas", "backlinks", "codigo"];
const REL = ["with-code", "no-code", "with-backlinks", "no-backlinks", "duplicates", "dead-code"];
const DAYS = ["0", "7", "30"];

function resolveRelative(from, target) {
  const parts = from.split("/"); parts.pop();
  for (const piece of decodeURIComponent(target.split("#")[0]).split("/")) { if (piece === "..") parts.pop(); else if (piece && piece !== ".") parts.push(piece); }
  return parts.join("/");
}

export async function render({ root, params: initial }) {
  const listHost = h("div", {}), viewer = h("div", { class: "inspector-col sticky" }), filtersHost = h("div", {}), chipsHost = h("div", {}), center = h("div", {});
  const drawer = { current: null };
  let facets = { folder: [], tag: [] }, selected = initial.get("note") || null, lastQuery = "", listGeneration = 0, graphGeneration = 0;
  let graph = null;
  const q = () => parseHash().params;
  const view = (params) => (VIEWS.includes(params.get("view")) ? params.get("view") : "notas");
  const listQuery = (params) => {
    const v = view(params);
    return { q: params.get("q") || "", folder: listParam(params, "folder"), tag: listParam(params, "tag"), rel: [...listParam(params, "rel"), ...(v === "codigo" ? ["with-code"] : v === "backlinks" ? ["with-backlinks"] : [])],
      days: params.get("days") || "", sort: params.get("sort") || (v === "backlinks" ? "backlinks" : "title"), order: params.get("order") || (v === "backlinks" ? "desc" : "asc"), page: Number(params.get("page") || 0), size: 40 };
  };
  const clearAll = () => { forget("obsidian"); patchQuery({ q: null, folder: null, tag: null, rel: null, days: null, sort: null, order: null }); };

  function drawFilters(params) {
    const v = view(params);
    const navItem = (id) => h("a", { class: "btn ghost", style: { justifyContent: "flex-start", width: "100%", background: v === id ? "var(--primary-soft)" : "" }, href: "#", "aria-current": v === id ? "page" : null,
      onclick: (e) => { e.preventDefault(); patchQuery({ view: id === "notas" ? null : id, page: null }); } }, icon({ notas: "note", grafo: "graph", etiquetas: "tag", backlinks: "link", codigo: "code" }[id], 16), t(`obs.view.${id}`));
    clear(filtersHost).append(
      h("div", { class: "card filters", style: { marginBottom: "12px" } }, h("div", { class: "head" }, h("h2", {}, t("obs.nav")), h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => drawer.current?.classList.remove("open") }, icon("x"))), h("nav", { "aria-label": t("obs.nav"), class: "stack", style: { gap: "2px" } }, VIEWS.map(navItem))),
      h("div", { class: "card filters" }, h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("button", { class: "btn ghost sm", onclick: clearAll }, t("filters.clear"))),
        filterGroup({ id: "folder", label: t("obs.folder"), options: facets.folder.map(([value, count]) => ({ value, label: value, count })), selected: listParam(params, "folder"), onChange: (x) => patchQuery({ folder: x }), search: true }),
        filterGroup({ id: "tag", label: t("obs.tags"), options: facets.tag.map(([value, count]) => ({ value, label: `#${value}`, count })), selected: listParam(params, "tag"), onChange: (x) => patchQuery({ tag: x }), search: true }),
        filterGroup({ id: "rel", label: t("obs.relation"), options: REL.map((value) => ({ value, label: t(`obs.rel.${value}`) })), selected: listParam(params, "rel"), onChange: (x) => patchQuery({ rel: x }), open: false }),
        h("details", { class: "fgroup", open: false }, h("summary", {}, t("obs.modified")), selectBox({ label: t("obs.modified"), value: params.get("days") || "0", options: DAYS.map((d) => [d, t(`obs.days.${d}`)]), onChange: (x) => patchQuery({ days: x === "0" ? null : x }), cls: "wide" })),
        h("p", { class: "logic" }, t("filters.logic")), note(t("obs.unsupported"), "", "info")));
  }

  function drawChips(params) {
    const chips = [];
    for (const key of ["folder", "tag", "rel"]) for (const v of listParam(params, key)) chips.push({ label: key === "tag" ? `#${v}` : key === "rel" ? t(`obs.rel.${v}`) : v, remove: () => patchQuery({ [key]: listParam(params, key).filter((x) => x !== v) }) });
    if (params.get("q")) chips.push({ label: `“${params.get("q")}”`, remove: () => patchQuery({ q: null }) });
    if (params.get("days")) chips.push({ label: t(`obs.days.${params.get("days")}`), remove: () => patchQuery({ days: null }) });
    clear(chipsHost).append(activeChips(chips, clearAll) || "");
  }

  const noteCard = (n) => h("button", { class: "note-card", "data-path": n.path, "aria-selected": String(n.path === selected), onclick: () => patchQuery({ note: n.path }) },
    h("span", { class: "kind-ico tone-primary" }, icon("note", 15)),
    h("span", { style: { minWidth: 0, flex: 1 } }, h("strong", { class: "ellipsis", style: { display: "block" } }, n.title), h("span", { class: "muted small ellipsis", style: { display: "block" } }, n.path),
      h("span", { class: "row", style: { gap: "4px", marginTop: "4px" } }, n.tags.slice(0, 3).map((tag) => badge(`#${tag}`, "primary")), n.symbolCount != null && badge(t("obs.symbols", { n: n.symbolCount }), "info"), n.backlinks > 0 && badge(t("obs.backlinksN", { n: n.backlinks }), "accent"))),
    h("span", { class: "muted small" }, ago(n.modified)));

  async function ensureFacets(params) {
    if (facets.folder.length || facets.tag.length) return;
    try { facets = (await api("notes", { size: 1 }, "facets")).facets; drawFilters(q()); } catch { /* filters stay empty */ }
  }

  async function drawList(params) {
    const generation = ++listGeneration;
    graphGeneration += 1;
    const v = view(params);
    graph?.destroy(); graph = null;
    cancelApi("notes"); cancelApi("ngraph");
    drawFilters(params);
    clear(listHost); clear(center);
    if (v === "grafo") { await ensureFacets(params); if (generation !== listGeneration || view(q()) !== v) return; return drawGraph(params); }
    if (v === "etiquetas") { await ensureFacets(params); if (generation !== listGeneration || view(q()) !== v) return; return drawTags(params); }
    clear(listHost).append(loading());
    try {
      const data = await api("notes", listQuery(params), "notes");
      if (generation !== listGeneration) return;
      facets = data.facets; drawFilters(q());
      clear(listHost);
      if (!data.all) { listHost.append(emptyState({ iconName: "note", title: t("obs.emptyTitle"), text: t("obs.emptyText") })); return; }
      if (data.truncated) listHost.append(note(t("obs.truncated"), "warn", "alert"));
      if (!data.total) { listHost.append(noResults({ onClear: clearAll })); return; }
      listHost.append(h("h2", { style: { margin: "0 0 8px" } }, t("obs.notesCount", { n: fmt(data.total) })), h("div", { class: "card", style: { padding: "6px" } }, data.items.map(noteCard)),
        pager({ page: data.page, size: data.size, total: data.total, onPage: (page) => patchQuery({ page }) }));
    } catch (error) { if (!isAbort(error) && generation === listGeneration) clear(listHost).append(errorBox(error, () => drawList(q()))); }
  }

  function drawTags(params) {
    clear(listHost).append(h("div", { class: "card" }, h("h2", {}, t("obs.tags")), facets.tag.length ? h("div", { class: "chips", style: { marginTop: "10px" } }, facets.tag.map(([tag, n]) => h("button", { class: "tag", onclick: () => patchQuery({ view: null, tag }) }, `#${tag} · ${n}`))) : emptyState({ title: t("obs.noTags"), iconName: "tag" })));
  }

  async function drawGraph(params) {
    const generation = ++graphGeneration;
    clear(listHost).append(loading());
    graph?.destroy();
    try {
      const data = await api("notes-graph", { folder: listParam(params, "folder"), tag: listParam(params, "tag"), focus: params.get("focus") || "", depth: params.get("depth") || 2, orphans: params.get("orphans") || "", all: params.get("all") || "" }, "ngraph");
      if (generation !== graphGeneration || view(q()) !== "grafo") return;
      clear(listHost);
      const search = h("input", { class: "input", type: "search", placeholder: t("obs.graphSearch"), "aria-label": t("obs.graphSearch"), list: "ngraph-nodes" });
      const dl = h("datalist", { id: "ngraph-nodes" }, data.nodes.map((n) => h("option", { value: n.label })));
      search.addEventListener("change", () => { const hit = data.nodes.find((n) => n.label === search.value); if (hit) { graph.select(hit.id); patchQuery({ note: hit.id }); } });
      listHost.append(h("div", { class: "toolbar" }, search, dl,
        h("label", { class: "field", style: { gridAutoFlow: "column", alignItems: "center" } }, t("obs.orphans"), selectBox({ label: t("obs.orphans"), value: params.get("orphans") || "", options: [["", t("obs.orphans.all")], ["hide", t("obs.orphans.hide")], ["only", t("obs.orphans.only")]], onChange: (x) => patchQuery({ orphans: x }) })),
        params.get("focus") && h("button", { class: "btn sm", onclick: () => patchQuery({ focus: null }) }, t("obs.clearFocus")),
        params.get("focus") && h("label", { class: "field", style: { gridAutoFlow: "column", alignItems: "center" } }, t("deps.depth"), h("input", { type: "range", min: 1, max: 6, value: params.get("depth") || 2, onchange: (e) => patchQuery({ depth: e.target.value }) }))));
      if (data.truncated) listHost.append(note(t("obs.graphTruncated", { n: data.nodes.length, total: data.total }), "warn", "alert"), h("button", { class: "btn sm", style: { margin: "8px 0" }, onclick: () => patchQuery({ all: 1 }) }, t("graph.showAll", { n: fmt(data.total) })));
      if (!data.nodes.length) { listHost.append(emptyState({ iconName: "graph", title: t("obs.noGraph"), text: t("obs.noGraphText") })); return; }
      graph = graphView({ label: t("obs.graphLabel"), directed: false, nodes: data.nodes.map((n) => ({ ...n, color: n.id === selected ? "var(--primary)" : n.orphan ? "var(--text-3)" : n.hasCode ? "var(--accent)" : "var(--primary)", radius: 6 + Math.min(10, n.degree), title: `${n.label} · ${n.folder}` })), edges: data.edges,
        onSelect: (node) => patchQuery({ note: node.id }) });
      listHost.append(graph.el, h("p", { class: "muted small", style: { marginTop: "8px" } }, t("obs.graphNote")), h("div", { class: "legend", style: { marginTop: "6px" } }, [["var(--primary)", t("obs.legend.note")], ["var(--accent)", t("obs.legend.code")], ["var(--text-3)", t("obs.legend.orphan")]].map(([c, l]) => h("span", {}, h("i", { style: { background: c } }), l))));
    } catch (error) { if (!isAbort(error) && generation === graphGeneration) clear(listHost).append(errorBox(error, () => drawGraph(q()))); }
  }

  async function showNote(path) {
    selected = path;
    cancelApi("note");
    root.querySelectorAll(".note-card").forEach((el) => el.setAttribute("aria-selected", "false"));
    if (!path) { clear(viewer).append(h("div", { class: "card inspector" }, emptyState({ iconName: "note", title: t("obs.pickTitle"), text: t("obs.pickText") }))); return; }
    clear(viewer).append(h("div", { class: "card inspector" }, loading()));
    try {
      const n = await api("note", { path }, "note");
      if (selected !== path) return;
      const open = (target, wiki) => {
        const hit = wiki ? n.linksTo.find((l) => basename(l.path).replace(/\.md$/, "") === target) : n.linksTo.find((l) => l.path === resolveRelative(n.path, target));
        if (hit) patchQuery({ note: hit.path }); else toast(t("obs.unresolved", { name: target }), "info");
      };
      const mdLink = `[${n.title}](${n.path.split("/").map(encodeURIComponent).join("/")})`;
      clear(viewer).append(h("div", { class: "card inspector" },
        h("header", {}, h("div", {}, h("h3", {}, n.title), h("div", { class: "mono muted small" }, n.path)), h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => patchQuery({ note: null }) }, icon("x"))),
        h("div", { class: "row", style: { marginBottom: "10px" } },
          h("div", { class: "seg", role: "group", "aria-label": t("obs.mode") }, h("button", { "aria-pressed": "true" }, t("obs.preview")), h("button", { disabled: true, title: t("obs.editDisabled"), "aria-pressed": "false" }, t("obs.edit"))),
          h("button", { class: "btn sm", onclick: () => copy(n.path, t("common.pathCopied")) }, icon("copy", 14), t("common.copyPath")), h("button", { class: "btn sm", onclick: () => copy(mdLink, t("obs.linkCopied")) }, icon("link", 14), t("obs.copyLink")),
          h("button", { class: "btn sm", disabled: true, title: t("obs.openDisabled") }, icon("external", 14), t("obs.openObsidian"))),
        n.tags.length > 0 && h("div", { class: "chips", style: { marginBottom: "10px" } }, n.tags.map((tag) => h("button", { class: "tag", onclick: () => patchQuery({ view: null, tag }) }, `#${tag}`))),
        n.frontmatter.length > 0 && h("details", { class: "section", open: true }, h("summary", {}, t("obs.frontmatter")), h("dl", { class: "kv", style: { marginTop: "8px" } }, n.frontmatter.map(([k, v]) => [h("dt", {}, k), h("dd", { class: "mono small" }, v)]))),
        h("div", { class: "section" }, renderMarkdown(n.body, { onNote: open, onTag: (tag) => patchQuery({ view: null, tag }) })),
        h("div", { class: "section" }, h("h4", {}, t("obs.links", { n: n.linksTo.length })), n.linksTo.length ? pagedLinks(n.linksTo, (l) => h("div", {}, h("button", { class: "btn ghost sm", onclick: () => patchQuery({ note: l.path }) }, l.title || l.path))) : h("p", { class: "muted" }, t("obs.none"))),
        h("div", { class: "section" }, h("h4", {}, t("obs.backlinks", { n: n.backlinkList.length })), n.backlinkList.length ? pagedLinks(n.backlinkList, (l) => h("div", {}, h("button", { class: "btn ghost sm", onclick: () => patchQuery({ note: l.path }) }, l.title || l.path))) : h("p", { class: "muted" }, t("obs.none")),
          h("button", { class: "btn sm", style: { marginTop: "6px" }, onclick: () => patchQuery({ view: "grafo", focus: n.path }) }, icon("graph", 14), t("obs.showInGraph"))),
        h("div", { class: "section" }, h("h4", {}, t("obs.code")), n.codePath ? [
          h("p", {}, h("a", { href: hrefTo("espacio", { file: n.codePath }), class: "mono" }, n.codePath)),
          n.symbols.length > 0 && h("div", { class: "chips" }, n.symbols.map((s) => h("a", { class: "chip", href: hrefTo("codigo", { sel: s.id, dir: n.codePath }) }, s.name))),
          (n.duplicateKeys.length > 0 || n.deadCodeKeys.length > 0) && h("p", { class: "muted" }, t("obs.findings", { d: n.duplicateKeys.length, m: n.deadCodeKeys.length }), " ", n.duplicateKeys.length > 0 && h("a", { href: hrefTo("duplicados", { dir: n.codePath, state: "all" }) }, t("nav.duplicados")), " ", n.deadCodeKeys.length > 0 && h("a", { href: hrefTo("codigo-muerto", { dir: n.codePath, state: "all" }) }, t("nav.muerto"))),
          h("p", { class: "muted small" }, t("obs.relationSource"))] : h("p", { class: "muted" }, t("obs.noCode")))));
    } catch (error) {
      if (!isAbort(error) && selected === path) clear(viewer).append(h("div", { class: "card inspector" }, errorBox(error, () => showNote(path))));
    }
  }

  const stable = (params) => [...params].filter(([k]) => k !== "note").join("&");
  const search = searchBox({ value: initial.get("q") || "", placeholder: t("obs.searchPlaceholder"), label: t("obs.searchPlaceholder"), onInput: (v) => patchQuery({ q: v }) });
  const layout = h("div", { class: "workbench inspect" }, h("aside", { class: "filters-col" }, filtersHost),
    h("div", {}, h("div", { class: "toolbar" }, search, drawerButton(() => drawer.current)), chipsHost, listHost), viewer);
  drawer.current = layout.querySelector("aside");
  root.append(pageHead(t("obs.title"), t("obs.subtitle"), [badge(t("obs.readOnly"), "primary", "lock")]), layout);
  lastQuery = stable(initial);
  drawFilters(initial); drawChips(initial);
  await drawList(initial);
  await showNote(selected);
  const off = onQuery((params) => {
    search.sync(params.get("q") || "");
    const sel = params.get("note");
    if (sel !== selected) { showNote(sel); if (graph && sel) graph.select(sel); }
    root.querySelectorAll(".note-card").forEach((el) => el.setAttribute("aria-selected", String(el.dataset.path === sel)));
    if (stable(params) !== lastQuery) { lastQuery = stable(params); drawChips(params); drawList(params); }
  });
  return () => { listGeneration += 1; graphGeneration += 1; off(); graph?.destroy(); cancelApi("notes"); cancelApi("ngraph"); cancelApi("facets"); cancelApi("note"); search.destroy(); };
}
