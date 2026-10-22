// 07 · Dependencias: graph | tree | table over verified import edges
import { h, append, clear, api, isAbort, patchQuery, onQuery, parseHash, listParam, fmt, hrefTo, forget } from "./core.js";
import { icon, pageHead, segmented, badge, dataTable, filterGroup, note, loading, errorBox, emptyState, selectBox, drawerButton, activeChips } from "./components.js";
import { graphView } from "./graph.js";
import { t } from "./i18n.js";

const nodeColor = (node, focus) => (node.id === focus ? "var(--primary)" : node.cycle ? "var(--danger)" : node.unused ? "var(--text-3)" : node.importers > 0 ? "var(--success)" : "var(--info)");

export async function render({ root, params: initial, summary }) {
  const host = h("div", {}), filtersHost = h("div", {}), chipsHost = h("div", {});
  const drawer = { current: null };
  let graph = null, data = null;
  const q = () => parseHash().params;
  const queryFor = (params) => ({ level: params.get("level") || "dir", group: params.get("group") || 2, focus: params.get("focus") || "", depth: params.get("depth") || 2, language: listParam(params, "language") });
  const clearAll = () => { forget("dependencias"); patchQuery({ level: null, group: null, focus: null, depth: null, only: null, language: null }); };

  function drawFilters(params) {
    const query = queryFor(params);
    const only = (params.get("only") || "").split(",").filter(Boolean);
    const toggleOnly = (key) => patchQuery({ only: only.includes(key) ? only.filter((x) => x !== key) : [...only, key] });
    const depthInput = h("input", { type: "range", min: 1, max: 6, value: query.depth, "aria-label": t("deps.depth"), style: { width: "100%", accentColor: "var(--primary)" }, onchange: (e) => patchQuery({ depth: e.target.value }) });
    clear(filtersHost).append(h("div", { class: "card filters" },
      h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("div", { class: "row" }, h("button", { class: "btn ghost sm", onclick: clearAll }, t("filters.clear")), h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => drawer.current?.classList.remove("open") }, icon("x")))),
      h("details", { class: "fgroup", open: true }, h("summary", {}, t("deps.level")),
        segmented({ label: t("deps.level"), value: query.level, options: [["dir", t("deps.levelDir")], ["file", t("deps.levelFile")]], onSelect: (v) => patchQuery({ level: v === "dir" ? null : v, focus: null }) }),
        query.level === "dir" && h("label", { class: "field", style: { marginTop: "8px" } }, t("deps.group"), selectBox({ label: t("deps.group"), value: String(query.group), options: [1, 2, 3, 4].map((n) => [String(n), t("deps.groupN", { n })]), onChange: (v) => patchQuery({ group: v === "2" ? null : v, focus: null }) }))),
      h("details", { class: "fgroup", open: true }, h("summary", {}, t("deps.focusGroup")),
        query.focus ? h("div", { class: "stack" }, h("span", { class: "mono small", style: { wordBreak: "break-all" } }, query.focus), h("label", { class: "field" }, `${t("deps.depth")}: ${query.depth}`, depthInput), h("button", { class: "btn sm", onclick: () => patchQuery({ focus: null }) }, t("deps.clearFocus"))) : h("p", { class: "muted small" }, t("deps.noFocus"))),
      h("details", { class: "fgroup", open: true }, h("summary", {}, t("deps.show")),
        [["cycles", t("deps.onlyCycles")], ["unused", t("deps.onlyUnused")]].map(([key, label]) => h("label", { class: "opt" }, h("input", { type: "checkbox", checked: only.includes(key), onchange: () => toggleOnly(key) }), h("span", {}, label)))),
      filterGroup({ id: "language", label: t("filters.language"), options: (summary.languages || []).map((l) => ({ value: l.language, label: l.language, count: l.files })), selected: query.language, onChange: (v) => patchQuery({ language: v }), open: false }),
      h("p", { class: "logic" }, t("filters.logic")), note(t("deps.unsupported"), "", "info")));
  }

  function visible(params) {
    const only = (params.get("only") || "").split(",").filter(Boolean);
    let nodes = data.nodes;
    if (only.includes("cycles")) nodes = nodes.filter((n) => n.cycle);
    if (only.includes("unused")) nodes = nodes.filter((n) => n.unused);
    const ids = new Set(nodes.map((n) => n.id));
    return { nodes, edges: data.edges.filter((e) => ids.has(e.from) && ids.has(e.to)) };
  }

  function inspector(node, edges) {
    const incoming = edges.filter((e) => e.to === node.id), outgoing = edges.filter((e) => e.from === node.id);
    const link = (id) => h("button", { class: "btn ghost sm", style: { justifyContent: "flex-start" }, onclick: () => patchQuery({ focus: id }) }, h("span", { class: "mono ellipsis" }, id));
    return h("div", { class: "card inspector" },
      h("header", {}, h("div", {}, h("h3", {}, node.label), h("div", { class: "mono muted small" }, node.id)), h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => graph?.clearSelection() }, icon("x"))),
      h("div", { class: "row" }, node.cycle && badge(t("deps.cycle"), "danger"), node.unused && badge(t("deps.unused"), "")),
      h("dl", { class: "kv", style: { margin: "10px 0" } }, node.files != null && [h("dt", {}, t("deps.files")), h("dd", {}, fmt(node.files))], h("dt", {}, t("deps.out")), h("dd", {}, fmt(node.imports)), h("dt", {}, t("deps.in")), h("dd", {}, fmt(node.importers))),
      h("div", { class: "row" }, h("button", { class: "btn primary sm", onclick: () => patchQuery({ focus: node.id }) }, t("deps.focus")),
        h("button", { class: "btn sm", onclick: () => patchQuery({ focus: node.id, depth: Math.min(6, Number(queryFor(q()).depth) + 1) }) }, t("deps.expand")),
        node.kind === "file" ? h("a", { class: "btn sm", href: hrefTo("espacio", { file: node.id }) }, t("deps.openFile")) : h("a", { class: "btn sm", href: hrefTo("codigo", { dir: node.id }) }, t("deps.openSymbols"))),
      h("div", { class: "section" }, h("h4", {}, t("deps.incoming", { n: incoming.length })), incoming.length ? incoming.slice(0, 20).map((e) => link(e.from)) : h("p", { class: "muted" }, t("deps.none"))),
      h("div", { class: "section" }, h("h4", {}, t("deps.outgoing", { n: outgoing.length })), outgoing.length ? outgoing.slice(0, 20).map((e) => link(e.to)) : h("p", { class: "muted" }, t("deps.none"))));
  }

  function drawGraph(params) {
    const { nodes, edges } = visible(params);
    if (!nodes.length) { host.append(emptyState({ title: t("deps.emptyTitle"), text: t("deps.emptyText"), iconName: "graph" })); return; }
    const focus = data.focus;
    const side = h("div", { class: "inspector-col sticky", hidden: true });
    const frame = h("div", { class: "workbench nofilter", style: { gap: "12px" } });
    graph?.destroy();
    graph = graphView({ label: t("deps.graphLabel"), directed: true,
      nodes: nodes.map((n) => ({ ...n, color: nodeColor(n, focus), radius: 7 + Math.min(12, Math.sqrt(n.importers + n.imports)), title: `${n.id} · ${t("deps.out")} ${n.imports} · ${t("deps.in")} ${n.importers}` })),
      edges, onSelect: (node) => { side.hidden = false; frame.classList.add("inspect"); clear(side).append(inspector(node, edges)); } });
    host.append(append(frame, [h("div", {}, graph.el, h("div", { class: "legend", style: { marginTop: "10px" } },
      [["var(--primary)", t("deps.legend.focus")], ["var(--info)", t("deps.legend.entry")], ["var(--success)", t("deps.legend.used")], ["var(--text-3)", t("deps.legend.unused")], ["var(--danger)", t("deps.legend.cycle")]].map(([c, l]) => h("span", {}, h("i", { style: { background: c } }), l)))), side]));
  }

  function drawTree(params) {
    const { nodes, edges } = visible(params);
    const out = new Map(nodes.map((n) => [n.id, []]));
    for (const e of edges) out.get(e.from)?.push(e.to);
    const roots = data.focus ? [data.focus] : nodes.filter((n) => n.importers === 0).map((n) => n.id).slice(0, 40);
    const branch = (id, path, depth) => {
      const kids = (out.get(id) || []).filter((k) => !path.has(k));
      const label = h("span", { class: "row", style: { gap: "6px" } }, icon(data.level === "file" ? "file" : "folder", 14), h("span", { class: "mono" }, id), path.has(id) && badge(t("deps.cycle"), "danger"));
      if (!kids.length || depth >= 5) return h("li", { style: { padding: "2px 0" } }, label);
      return h("li", {}, h("details", { open: depth < 1 }, h("summary", {}, label), h("ul", { class: "tree" }, kids.map((k) => branch(k, new Set([...path, id]), depth + 1)))));
    };
    host.append(h("div", { class: "card" }, roots.length ? h("ul", { class: "tree", role: "tree", "aria-label": t("deps.treeLabel") }, roots.map((r) => branch(r, new Set(), 0))) : emptyState({ title: t("deps.emptyTitle"), text: t("deps.emptyText") })), note(t("deps.treeNote"), "", "info"));
  }

  function drawTable(params) {
    const { nodes } = visible(params);
    let sort = "importers", order = "desc";
    const holder = h("div", {});
    const paint = () => {
      const rows = [...nodes].sort((a, b) => { const x = a[sort], y = b[sort]; const c = typeof x === "string" ? x.localeCompare(y) : Number(x) - Number(y); return order === "asc" ? c : -c; });
      clear(holder).append(dataTable({ caption: t("deps.tableCaption"), rows, rowKey: (r) => r.id, sort, order, onSort: (s, o) => { sort = s; order = o; paint(); }, onSelect: (r) => patchQuery({ focus: r.id }),
        columns: [{ key: "id", label: t("deps.col.name"), sortable: true, render: (r) => h("span", { class: "path" }, r.id) }, { key: "files", label: t("deps.files"), sortable: true, num: true, render: (r) => r.files ?? 1 },
          { key: "imports", label: t("deps.out"), sortable: true, num: true, render: (r) => r.imports }, { key: "importers", label: t("deps.in"), sortable: true, num: true, render: (r) => r.importers },
          { key: "cycle", label: t("deps.cycle"), sortable: true, render: (r) => (r.cycle ? badge(t("deps.cycle"), "danger") : "—") }, { key: "unused", label: t("deps.unused"), sortable: true, render: (r) => (r.unused ? badge(t("deps.unused")) : "—") }] }));
    };
    paint();
    host.append(holder);
    if (data.external.length) host.append(h("div", { class: "card", style: { marginTop: "14px" } }, h("h2", {}, t("deps.external")), h("p", { class: "muted small" }, t("deps.externalNote")),
      data.external.map((m) => h("details", { style: { margin: "6px 0" } }, h("summary", {}, h("span", { class: "mono" }, m.manifest), " ", badge(t("deps.packages", { n: m.count }))),
        h("div", { class: "chips", style: { marginTop: "6px" } }, m.dependencies.map((d) => h("span", { class: "chip" }, d.name, d.version && h("span", { class: "muted" }, d.version))))))));
  }

  async function refresh(params = q()) {
    drawFilters(params);
    clear(chipsHost);
    const view = params.get("view") || "graph";
    clear(host).append(loading());
    try {
      data = await api("dependencies", queryFor(params), "deps");
      clear(host);
      if (data.truncated) host.append(note(t("deps.truncated", { shown: data.nodes.length, total: data.totalNodes }), "warn", "alert"), h("div", { style: { height: "10px" } }));
      if (data.cycles.length) host.append(note(t("deps.cycles", { n: data.cycles.length }), "warn", "alert"), h("div", { style: { height: "10px" } }));
      if (view === "tree") drawTree(params); else if (view === "table") drawTable(params); else drawGraph(params);
    } catch (error) { if (!isAbort(error)) clear(host).append(errorBox(error, () => refresh(params))); }
  }

  const viewsHost = h("div", {});
  const drawViews = (params) => clear(viewsHost).append(segmented({ label: t("deps.views"), value: params.get("view") || "graph", options: [["graph", t("deps.view.graph")], ["tree", t("deps.view.tree")], ["table", t("deps.view.table")]], onSelect: (v) => patchQuery({ view: v === "graph" ? null : v }) }));
  drawViews(initial);
  const layout = h("div", { class: "workbench" }, h("aside", { class: "filters-col" }, filtersHost), h("div", {}, h("div", { class: "toolbar" }, viewsHost, drawerButton(() => drawer.current)), chipsHost, host));
  drawer.current = layout.querySelector("aside");
  root.append(pageHead(t("deps.title"), t("deps.subtitle")), layout);
  await refresh(initial);
  let last = initial.toString();
  const off = onQuery((params) => { const sig = params.toString(); if (sig === last) return; last = sig; drawViews(params); refresh(params); });
  return () => { off(); graph?.destroy(); };
}
