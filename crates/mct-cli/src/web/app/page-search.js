// 06 · Búsqueda inteligente (keyword search over every indexed surface)
import { h, clear, api, isAbort, patchQuery, onQuery, parseHash, listParam, fmt, store, highlight, toast, forget } from "./core.js";
import { icon, pageHead, tabsBar, badge, stateBadge, pager, selectBox, filterGroup, activeChips, emptyState, loading, errorBox, note, drawerButton, kindIcon, noResults } from "./components.js";
import { openSource } from "./code.js";
import { targetOf } from "./palette.js";
import { t } from "./i18n.js";

const SCOPES = ["all", "code", "symbols", "files", "duplicates", "deadCode", "obsidian", "dependencies"];
const TYPE_ICON = { symbol: "func", file: "file", dependency: "graph", code: "code", duplicate: "copy", "dead-code": "skull", note: "note" };
const PAGE_SIZE = 20;

function resultCard(item, query) {
  const where = item.path ? `${item.path}${item.line ? `:${item.line}` : ""}` : "";
  return h("article", { class: "card", style: { display: "flex", gap: "14px", alignItems: "center", marginBottom: "10px" } },
    item.kind ? kindIcon(item.kind) : h("span", { class: "kind-ico tone-primary" }, icon(TYPE_ICON[item.type] || "file", 16)),
    h("div", { style: { minWidth: 0, flex: 1 } },
      h("h3", { class: "ellipsis" }, highlight(item.title, query)),
      where && h("div", { class: "mono muted small ellipsis" }, highlight(where, query)),
      item.snippet && item.snippet !== item.path && h("div", { class: "muted ellipsis" }, highlight(item.snippet, query))),
    h("div", { class: "row hide-sm", style: { justifyContent: "flex-end" } },
      h("span", { class: "badge" }, t(`search.type.${item.type}`)), item.language && badge(item.language),
      item.state && stateBadge(item.state), item.refs !== undefined && badge(t("search.refs", { n: item.refs })),
      (item.tags || []).slice(0, 2).map((tag) => badge(`#${tag}`, "primary"))),
    h("div", { class: "row" },
      item.path && item.line && h("button", { class: "btn sm", "aria-label": `${t("code.viewSource")}: ${item.title}`, onclick: () => openSource({ path: item.path, line: item.line, language: item.language }) }, icon("code", 14)),
      h("a", { class: "btn sm primary", href: targetOf(item) }, t("search.open"))));
}

export async function render({ root, params: initial, summary }) {
  const input = h("input", { class: "input wide", type: "search", value: initial.get("q") || "", placeholder: t("search.pagePlaceholder"), "aria-label": t("search.pagePlaceholder"), style: { border: 0, background: "transparent", fontSize: "15px" } });
  const results = h("div", {}), tabsHost = h("div", {}), chipsHost = h("div", {}), filtersHost = h("div", {}), meta = h("div", {});
  const drawer = { current: null };
  let sortMode = "relevance", data = null;
  const q = () => parseHash().params;

  const submit = () => {
    const query = input.value.trim();
    if (query) { const r = store.get("recent-searches", []).filter((x) => x !== query); r.unshift(query); store.set("recent-searches", r.slice(0, 8)); }
    patchQuery({ q: query || null, page: null });
  };
  const bar = h("div", { class: "row", style: { background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "10px", padding: "4px 6px 4px 12px", marginBottom: "12px", flexWrap: "nowrap" } },
    icon("search", 20), input, h("button", { class: "icon-btn", "aria-label": t("search.clear"), onclick: () => { input.value = ""; submit(); } }, icon("x", 16)),
    h("button", { class: "btn primary", onclick: submit }, t("search.button")));
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") submit(); });

  const clearFilters = () => { forget("busqueda"); patchQuery({ language: null, kind: null }); };
  function drawFilters(params) {
    const langs = (summary.languages || []).map((l) => ({ value: l.language, label: l.language, count: l.symbols }));
    const kinds = (summary.kinds || []).map((k) => ({ value: k.kind, label: k.kind, count: k.count }));
    clear(filtersHost).append(h("div", { class: "card filters" },
      h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("div", { class: "row" }, h("button", { class: "btn ghost sm", onclick: clearFilters }, t("filters.clear")),
        h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => drawer.current?.classList.remove("open") }, icon("x")))),
      filterGroup({ id: "language", label: t("filters.language"), options: langs, selected: listParam(params, "language"), onChange: (v) => patchQuery({ language: v }) }),
      filterGroup({ id: "kind", label: t("filters.symbolKind"), options: kinds, selected: listParam(params, "kind"), onChange: (v) => patchQuery({ kind: v }), search: true, open: false }),
      h("p", { class: "logic" }, t("filters.logic")), note(t("search.unsupported"), "", "info")));
  }

  async function run(params = q()) {
    const query = (params.get("q") || "").trim();
    const scope = params.get("scope") || "all";
    drawFilters(params);
    const values = [...listParam(params, "language"), ...listParam(params, "kind")];
    clear(chipsHost).append(activeChips(values.map((value) => ({ label: value, remove: () => patchQuery({ language: listParam(params, "language").filter((v) => v !== value), kind: listParam(params, "kind").filter((v) => v !== value) }) })), clearFilters) || "");
    clear(tabsHost); clear(meta);
    if (!query) { drawIdle(); return; }
    clear(results).append(loading(t("search.searching")));
    const filters = { q: query, language: listParam(params, "language"), kind: listParam(params, "kind") };
    try {
      const [all, scoped] = await Promise.all([api("search", { ...filters, scope: "all" }, "search-all"), scope === "all" ? null : api("search", { ...filters, scope }, "search-scope")]);
      data = all;
      drawTabs(scope);
      const groups = scope === "all" ? all.groups : scoped.groups;
      const total = Object.values(all.groups).reduce((n, g) => n + g.total, 0);
      clear(meta).append(h("div", { class: "row spread", style: { marginBottom: "10px" } },
        h("span", { class: "muted", role: "status" }, t("search.summary", { n: fmt(scope === "all" ? total : all.groups[scope]?.total || 0), ms: all.tookMs })),
        h("div", { class: "row" }, selectBox({ label: t("search.sort"), value: sortMode, options: [["relevance", t("search.sort.relevance")], ["path", t("search.sort.path")], ["title", t("search.sort.title")]], onChange: (v) => { sortMode = v; drawResults(params, scope, groups); } }),
          h("button", { class: "btn sm", onclick: () => { const saved = store.get("saved-searches", []); if (!saved.includes(query)) { saved.unshift(query); store.set("saved-searches", saved.slice(0, 12)); } toast(t("search.savedOk"), "ok"); } }, icon("star", 14), t("search.save")))));
      drawResults(params, scope, groups);
    } catch (error) { if (!isAbort(error)) clear(results).append(errorBox(error, () => run(params))); }
  }

  function drawTabs(scope) {
    clear(tabsHost).append(tabsBar({ label: t("search.scopes"), active: scope, onSelect: (id) => patchQuery({ scope: id === "all" ? null : id }),
      tabs: SCOPES.map((id) => ({ id, label: t(`search.scope.${id}`), count: id === "all" ? Object.values(data.groups).reduce((n, g) => n + g.total, 0) : data.groups[id]?.total ?? 0 })) }));
  }

  function drawResults(params, scope, groups) {
    const query = params.get("q");
    clear(results);
    if (scope === "all") {
      let any = false;
      for (const id of SCOPES.slice(1)) {
        const g = groups[id];
        if (!g?.items.length) continue;
        any = true;
        results.append(h("div", { class: "row spread", style: { margin: "14px 0 8px" } }, h("h2", {}, `${t(`search.scope.${id}`)} (${fmt(g.total)})`),
          g.total > g.items.length && h("button", { class: "btn ghost sm", onclick: () => patchQuery({ scope: id }) }, t("search.seeAll"))),
          g.items.map((item) => resultCard(item, query)));
      }
      if (!any) results.append(emptyState({ title: t("search.noneTitle"), text: t("search.noneText", { q: query }), actions: [h("button", { class: "btn", onclick: () => { input.value = ""; patchQuery({ q: null, language: null, kind: null }); } }, t("search.modify"))] }));
      return;
    }
    let items = groups[scope]?.items || [];
    if (sortMode === "path") items = [...items].sort((a, b) => (a.path || "").localeCompare(b.path || ""));
    if (sortMode === "title") items = [...items].sort((a, b) => a.title.localeCompare(b.title));
    if (!items.length) { results.append(noResults({ onClear: () => patchQuery({ scope: null, language: null, kind: null }) })); return; }
    const page = Math.min(Number(params.get("page") || 0), Math.ceil(items.length / PAGE_SIZE) - 1);
    results.append(items.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE).map((item) => resultCard(item, query)),
      pager({ page, size: PAGE_SIZE, total: items.length, onPage: (p) => patchQuery({ page: p }) }));
    if ((groups[scope]?.total || 0) > items.length) results.append(note(t("search.capped", { n: items.length }), "", "info"));
  }

  function drawIdle() {
    clear(results);
    const recent = store.get("recent-searches", []), saved = store.get("saved-searches", []);
    const list = (title, values, removable) => (values.length ? h("section", { class: "card", style: { marginBottom: "12px" } }, h("h2", {}, title),
      h("div", { class: "chips" }, values.map((v) => h("span", { class: "chip" }, h("button", { style: { width: "auto", padding: "0 4px" }, onclick: () => { input.value = v; submit(); } }, v),
        removable && h("button", { "aria-label": t("search.removeSaved", { q: v }), onclick: () => { store.set("saved-searches", saved.filter((x) => x !== v)); run(); } }, icon("x", 11)))))) : null);
    results.append(note(t("search.mode"), "", "info"), h("div", { style: { height: "12px" } }), list(t("search.recent"), recent, false), list(t("search.savedTitle"), saved, true),
      !recent.length && !saved.length && emptyState({ title: t("search.startTitle"), text: t("search.startText"), iconName: "search" }));
  }

  const layout = h("div", { class: "workbench" }, h("aside", { class: "filters-col" }, filtersHost), h("div", {}, bar, h("div", { class: "toolbar" }, drawerButton(() => drawer.current)), chipsHost, tabsHost, meta, results));
  drawer.current = layout.querySelector("aside");
  root.append(pageHead(t("search.title"), t("search.subtitle")), layout);
  await run(initial);
  let last = initial.toString();
  const off = onQuery((params) => { const sig = params.toString(); if (sig !== last) { last = sig; if ((params.get("q") || "") !== input.value.trim()) input.value = params.get("q") || ""; run(params); } });
  return () => off();
}
