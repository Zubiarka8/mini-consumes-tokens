// 02 · Código indexado: file tree and filters | symbol table | symbol inspector
import { h, clear, api, isAbort, patchQuery, onQuery, parseHash, listParam, fmt, copy, hrefTo, go, forget } from "./core.js";
import { icon, pageHead, tabsBar, segmented, dataTable, pager, kindIcon, kindGroup, kindGroups, badge, searchBox, selectBox, filterGroup, activeChips, noResults,
  emptyState, loading, errorBox, buildTree, treeView, drawerButton, note, stateBadge } from "./components.js";
import { codeBlock, openSource } from "./code.js";
import { t } from "./i18n.js";

const REFS = { any: {}, zero: { maxRefs: 0 }, some: { minRefs: 1 }, many: { minRefs: 5 } };
const FINDINGS = ["any", "duplicate", "dead-code", "anyFinding", "none"];

export async function render({ root, params: initial }) {
  const files = (await api("files")).files;
  const tree = buildTree(files);
  const expanded = new Set();
  const results = h("div", {});
  const inspector = h("div", { class: "inspector-col sticky", hidden: true });
  const filtersHost = h("div", {});
  const tabsHost = h("div", {});
  const chipsHost = h("div", {});
  const quickHost = h("div", { class: "quick-filters" });
  const drawer = { current: null };
  let facets = { kind: [], language: [], ext: [] };
  let selectedId = null;

  const q = () => parseHash().params;
  const queryFor = (params) => {
    const tab = params.get("tab") || "all";
    const kinds = tab === "all" ? [] : kindGroups().find((group) => group.id === tab)?.kinds || [];
    const finding = params.get("finding") || "any";
    return {
      q: params.get("q") || "", dir: params.get("dir") || "", kind: kinds, language: listParam(params, "language"), ext: listParam(params, "ext"),
      ...REFS[params.get("refs") || "any"], finding: finding === "anyFinding" ? "any" : finding === "any" ? "" : finding,
      sort: params.get("sort") || "name", order: params.get("order") || "asc", page: Number(params.get("page") || 0), size: 50,
    };
  };

  const clearAll = () => { forget("codigo"); patchQuery({ q: null, tab: null, language: null, ext: null, dir: null, refs: null, finding: null, sort: null, order: null }); };

  function drawFilters(params) {
    const dir = params.get("dir") || "";
    clear(filtersHost).append(
      h("div", { class: "card filters" },
        h("div", { class: "head" }, h("h2", {}, t("code.files")), h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => drawer.current?.classList.remove("open") }, icon("x"))),
        h("div", {},
          h("button", { class: "btn ghost sm", style: { marginBottom: "4px" }, onclick: () => patchQuery({ dir: null }) }, dir ? t("code.allFiles") : h("strong", {}, t("code.allFiles"))),
          treeView({ root: tree, current: dir, expanded, onSelect: (path) => { patchQuery({ dir: path }); } }))),
      h("div", { class: "card filters", style: { marginTop: "12px" } },
        h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("button", { class: "btn ghost sm", onclick: clearAll }, t("filters.clear"))),
        filterGroup({ id: "language", label: t("filters.language"), options: facets.language.map(([value, count]) => ({ value, label: value, count })), selected: listParam(params, "language"), onChange: (v) => patchQuery({ language: v }) }),
        filterGroup({ id: "ext", label: t("filters.extension"), options: facets.ext.map(([value, count]) => ({ value, label: `.${value || "—"}`, count })), selected: listParam(params, "ext"), onChange: (v) => patchQuery({ ext: v }), search: true, open: false }),
        h("details", { class: "fgroup", open: true }, h("summary", {}, t("filters.references")),
          selectBox({ label: t("filters.references"), value: params.get("refs") || "any", options: Object.keys(REFS).map((id) => [id, t(`filters.refs.${id}`)]), onChange: (v) => patchQuery({ refs: v === "any" ? null : v }), cls: "wide" })),
        h("details", { class: "fgroup", open: true }, h("summary", {}, t("filters.findings")),
          selectBox({ label: t("filters.findings"), value: params.get("finding") || "any", options: FINDINGS.map((id) => [id, t(`filters.finding.${id}`)]), onChange: (v) => patchQuery({ finding: v === "any" ? null : v }), cls: "wide" })),
        h("p", { class: "logic" }, t("filters.logic")),
        note(t("code.unsupported"), "", "info")));
  }

  function drawTabs(params) {
    const present = new Set(facets.kind.map(([kind]) => kindGroup(kind)));
    if (params.get("tab")) present.add(params.get("tab")); // the open tab stays, even when the filters leave it empty
    const groups = kindGroups().filter((group) => present.has(group.id));
    const counts = Object.fromEntries(groups.map((g) => [g.id, facets.kind.filter(([k]) => kindGroup(k) === g.id).reduce((n, [, c]) => n + c, 0)]));
    const total = facets.kind.reduce((n, [, c]) => n + c, 0);
    clear(tabsHost).append(tabsBar({ label: t("code.kinds"), active: params.get("tab") || "all", onSelect: (id) => patchQuery({ tab: id === "all" ? null : id }),
      tabs: [{ id: "all", label: t("code.tab.all"), count: total }, ...groups.map((g) => ({ id: g.id, label: t(`code.tab.${g.id}`), count: counts[g.id] }))] }));
  }

  // Always-visible filters: the same query parameters as the side panel, one click away.
  function drawQuick(params) {
    const pick = (key, label, all, options) => {
      const selected = listParam(params, key);
      const choices = [["", selected.length > 1 ? t("quick.several", { n: selected.length }) : all], ...options.map(([value]) => [value, value])];
      return h("label", { class: "quick-select" }, h("span", { class: "muted small" }, label),
        selectBox({ label, value: selected.length === 1 ? selected[0] : "", options: choices, onChange: (v) => patchQuery({ [key]: v ? [v] : null }) }));
    };
    clear(quickHost).append(
      segmented({ label: t("filters.findings"), value: params.get("finding") || "any", options: FINDINGS.map((id) => [id, t(`quick.finding.${id}`)]), onSelect: (v) => patchQuery({ finding: v === "any" ? null : v }) }),
      segmented({ label: t("filters.references"), value: params.get("refs") || "any", options: Object.keys(REFS).map((id) => [id, t(`quick.refs.${id}`)]), onSelect: (v) => patchQuery({ refs: v === "any" ? null : v }) }),
      pick("language", t("filters.language"), t("quick.allLanguages"), facets.language),
      pick("ext", t("filters.extension"), t("quick.allExtensions"), facets.ext.filter(([value]) => value).map(([value]) => [value])));
  }

  function drawChips(params) {
    const chips = [];
    const add = (key, label, value) => chips.push({ label, remove: () => patchQuery({ [key]: value }) });
    if (params.get("q")) add("q", `“${params.get("q")}”`, null);
    if (params.get("dir")) add("dir", `${t("code.dir")}: ${params.get("dir")}`, null);
    for (const key of ["language", "ext"]) for (const value of listParam(params, key)) chips.push({ label: value, remove: () => patchQuery({ [key]: listParam(params, key).filter((v) => v !== value) }) });
    if (params.get("refs")) add("refs", t(`filters.refs.${params.get("refs")}`), null);
    if (params.get("finding")) add("finding", t(`filters.finding.${params.get("finding")}`), null);
    clear(chipsHost).append(activeChips(chips, clearAll) || "");
  }

  async function refresh(params = q(), full = true) {
    const query = queryFor(params);
    if (full) { drawChips(params); clear(results).append(loading()); }
    try {
      const data = await api("symbols", query, "symbols");
      facets = data.facets;
      drawTabs(params); drawQuick(params); drawFilters(params);
      clear(results);
      if (!data.total) { results.append(noResults({ onClear: clearAll })); return; }
      results.append(
        dataTable({
          caption: t("code.tableCaption"), rows: data.items, rowKey: (r) => String(r.id), selected: selectedId === null ? null : String(selectedId), sort: query.sort, order: query.order,
          onSort: (sort, order) => patchQuery({ sort, order }), onSelect: (row) => patchQuery({ sel: row.id }),
          columns: [
            { key: "name", label: t("code.col.name"), sortable: true, render: (r) => h("span", { class: "name" }, kindIcon(r.kind), h("span", { class: "ellipsis" }, r.name)) },
            { key: "kind", label: t("code.col.kind"), sortable: true, render: (r) => r.kind, cls: "hide-sm hide-insp" },
            { key: "language", label: t("code.col.language"), sortable: true, render: (r) => badge(r.language), cls: "hide-sm hide-insp" },
            { key: "path", label: t("code.col.path"), sortable: true, render: (r) => h("span", { class: "path", title: r.path }, r.path) },
            { key: "line", label: t("code.col.line"), sortable: true, num: true, render: (r) => r.line, cls: "hide-sm hide-insp" },
            { key: "refs", label: t("code.col.refs"), sortable: true, num: true, render: (r) => r.refs, cls: "hide-sm hide-insp" },
            { key: "findings", label: t("code.col.findings"), render: (r) => h("span", { class: "row", style: { gap: "4px" } }, r.duplicateGroups > 0 && badge(t("common.duplicate"), "warning"), r.deadCandidate && badge(t("common.deadCode"), "danger")) },
            { key: "actions", label: t("code.col.actions"), render: (r) => h("button", { class: "btn sm", "aria-label": `${t("code.viewSource")}: ${r.name}`, onclick: () => openSource({ path: r.path, line: r.line, language: r.language }) }, icon("code", 14), t("code.view")) },
          ],
        }),
        pager({ page: data.page, size: data.size, total: data.total, onPage: (page) => patchQuery({ page }) }));
    } catch (error) { if (!isAbort(error)) clear(results).append(errorBox(error, () => refresh(params))); }
  }

  async function showSymbol(id) {
    selectedId = id;
    if (id === null) { inspector.hidden = true; root.querySelector(".workbench")?.classList.remove("inspect"); return; }
    root.querySelector(".workbench")?.classList.add("inspect");
    inspector.hidden = false;
    clear(inspector).append(h("div", { class: "card inspector" }, loading()));
    try {
      const data = await api("symbol", { id }, "symbol");
      const s = data.symbol;
      const select = (row) => patchQuery({ sel: row.id });
      clear(inspector).append(h("div", { class: "card inspector" },
        h("header", {}, h("div", {}, h("h3", {}, s.name), h("div", { class: "row", style: { marginTop: "6px" } }, kindIcon(s.kind), badge(s.kind), badge(s.language))),
          h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => patchQuery({ sel: null }) }, icon("x"))),
        h("dl", { class: "kv" }, h("dt", {}, t("code.col.path")), h("dd", { class: "mono" }, s.path),
          h("dt", {}, t("code.lines")), h("dd", {}, s.endLine ? `${s.line}–${s.endLine}` : s.line),
          s.parent && [h("dt", {}, t("code.parent")), h("dd", {}, s.parent)],
          h("dt", {}, t("code.col.refs")), h("dd", {}, `${fmt(s.refs)} · ${t("code.refsByName")}`)),
        h("div", { class: "row", style: { margin: "12px 0" } },
          h("button", { class: "btn sm", onclick: () => copy(`${s.path}:${s.line}`, t("common.pathCopied")) }, icon("copy", 14), t("code.copyPath")),
          h("button", { class: "btn sm", onclick: () => copy(s.parent ? `${s.parent}::${s.name}` : s.name, t("code.idCopied")) }, t("code.copyId")),
          h("button", { class: "btn sm", onclick: () => openSource({ path: s.path, line: s.line, language: s.language }) }, icon("external", 14), t("code.openSource")),
          h("a", { class: "btn sm", href: hrefTo("dependencias", { level: "file", focus: s.path }) }, icon("graph", 14), t("code.viewDeps"))),
        data.code ? codeBlock({ lines: data.code.lines, start: data.code.start, language: s.language, maxHeight: "260px", label: t("code.preview") }) : note(t("code.noPreview"), "warn", "alert"),
        data.code?.truncated && h("p", { class: "muted small" }, t("code.truncated")),
        h("div", { class: "section" }, h("h4", {}, t("code.references", { n: data.references.length })),
          data.references.length ? h("ul", { class: "stack", style: { listStyle: "none", margin: 0, padding: 0 } }, data.references.slice(0, 12).map((r) => h("li", {}, h("button", { class: "btn ghost sm", onclick: () => openSource({ path: r.path, line: r.line }) }, h("span", { class: "mono" }, `${r.path}:${r.line}`), " ", badge(r.kind))))) : h("p", { class: "muted" }, t("code.noRefs")),
          h("p", { class: "muted small" }, t("code.refsNote"))),
        data.related.length > 0 && h("div", { class: "section" }, h("h4", {}, t("code.related")), h("div", { class: "chips" }, data.related.map((r) => h("button", { class: "chip", onclick: () => select(r) }, r.name)))),
        h("div", { class: "section" }, h("h4", {}, t("code.dupes")),
          data.duplicates.length ? data.duplicates.map((d) => h("p", {}, h("a", { href: hrefTo("duplicados", { key: d.key }) }, d.title), " ", stateBadge(d.state))) : h("p", { class: "muted" }, t("code.noDupes"))),
        h("div", { class: "section" }, h("h4", {}, t("code.deadHead")),
          data.dead ? h("p", {}, badge(t("common.deadCode"), "danger"), " ", h("a", { href: hrefTo("codigo-muerto", { key: data.dead.key }) }, t("code.seeDead")), " ", stateBadge(data.dead.state)) : h("p", { class: "muted" }, t("code.noDead"))),
        h("div", { class: "section" }, h("h4", {}, t("code.notes")),
          data.notes.length ? data.notes.map((n) => h("p", {}, h("a", { href: hrefTo("obsidian", { note: n.path }) }, n.title))) : h("p", { class: "muted" }, t("code.noNotes")))));
    } catch (error) { clear(inspector).append(h("div", { class: "card inspector" }, errorBox(error))); }
  }

  const layout = h("div", { class: "workbench" }, h("aside", { class: "filters-col" }, filtersHost), h("div", {}, h("div", { class: "toolbar" },
    searchBox({ value: initial.get("q") || "", placeholder: t("code.searchPlaceholder"), label: t("code.searchPlaceholder"), onInput: (v) => patchQuery({ q: v }) }),
    drawerButton(() => drawer.current)), chipsHost, quickHost, tabsHost, results), inspector);
  drawer.current = layout.querySelector("aside");
  root.append(pageHead(t("code.title"), t("code.subtitle")), layout);
  const last = initial.get("sel");
  if (last) selectedId = Number(last);
  await refresh(initial);
  if (last) showSymbol(Number(last));
  const stable = (params) => [...params].filter(([key]) => key !== "sel").join("&");
  let lastQuery = stable(initial);
  const off = onQuery((params) => {
    const sel = params.get("sel");
    if ((sel === null ? null : Number(sel)) !== selectedId) showSymbol(sel === null ? null : Number(sel));
    root.querySelectorAll("tbody tr").forEach((row) => row.setAttribute("aria-selected", String(row.dataset.key === sel)));
    if (stable(params) !== lastQuery) { lastQuery = stable(params); refresh(params); }
  });
  return () => off();
}
