// 08 · Workspace de código unificado: explorer | source | everything known about the file
import { h, clear, api, isAbort, patchQuery, onQuery, parseHash, fmt, hrefTo, copy } from "./core.js";
import { icon, pageHead, tabsBar, badge, stateBadge, buildTree, treeView, loading, errorBox, emptyState, kindIcon, note, drawerButton } from "./components.js";
import { codeBlock } from "./code.js";
import { findingHref } from "./page-findings.js";
import { t } from "./i18n.js";

const CHUNK = 300;

export async function render({ root, params: initial }) {
  const files = (await api("files")).files;
  const tree = buildTree(files);
  const expanded = new Set();
  const explorer = h("div", { class: "card filters", style: { maxHeight: "calc(100vh - 150px)", overflow: "auto" } });
  const center = h("div", {}), side = h("div", { class: "inspector-col sticky" });
  let current = null, tab = "code";
  const q = () => parseHash().params;

  function drawExplorer(path) {
    const parts = (path || "").split("/");
    parts.pop();
    parts.reduce((acc, part) => { const next = acc ? `${acc}/${part}` : part; expanded.add(next); return next; }, "");
    clear(explorer).append(h("div", { class: "head" }, h("h2", {}, t("ws.explorer")), h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => explorerColumn.classList.remove("open") }, icon("x"))), treeView({ root: tree, current: path, expanded, onSelect: (p, dir) => { if (dir) drawExplorer(path); else patchQuery({ file: p, line: null, tab: null }); } }));
  }

  async function show(params) {
    const file = params.get("file");
    tab = params.get("tab") || "code";
    drawExplorer(file);
    if (!file) { clear(center).append(emptyState({ iconName: "layers", title: t("ws.pickTitle"), text: t("ws.pickText") })); clear(side); return; }
    current = file;
    clear(center).append(loading());
    clear(side).append(h("div", { class: "card inspector" }, loading()));
    try {
      const line = Number(params.get("line") || 1);
      const start = Math.max(1, line - 10) || 1;
      const [syms, dupes, dead, notes, deps, src] = await Promise.all([
        api("symbols", { dir: file, size: 200, sort: "line" }, "ws-sym"), api("findings", { type: "duplicate", q: file, state: [], size: 50 }, "ws-dup"),
        api("findings", { type: "dead-code", dir: file, state: [], size: 50 }, "ws-dead"), api("notes", { q: file, size: 20 }, "ws-notes"),
        api("dependencies", { level: "file", focus: file, depth: 1 }, "ws-deps"), api("source", { path: file, start, end: start + CHUNK - 1 }, "ws-src")]);
      const language = src.language || "";
      const outgoing = deps.edges.filter((e) => e.from === file).map((e) => e.to), incoming = deps.edges.filter((e) => e.to === file).map((e) => e.from);
      const marks = line ? { [line]: "hl" } : {};
      const tabs = tabsBar({ label: t("ws.tabs"), active: tab, onSelect: (id) => patchQuery({ tab: id === "code" ? null : id }),
        tabs: [{ id: "code", label: t("ws.code") }, { id: "findings", label: t("ws.findings"), count: dupes.total + dead.total }, { id: "refs", label: t("ws.refs"), count: incoming.length + outgoing.length }, { id: "notes", label: t("ws.notes"), count: notes.total }] });
      const body = {
        code: () => [h("div", { class: "row spread", style: { marginBottom: "8px" } }, h("span", { class: "muted small" }, t("source.range", { from: src.start, to: src.start + src.lines.length - 1 })),
          h("div", { class: "row" }, h("button", { class: "btn sm", disabled: start <= 1, onclick: () => patchQuery({ line: Math.max(1, start - CHUNK + 10) }) }, t("source.prev")), h("button", { class: "btn sm", disabled: src.lines.length < CHUNK, onclick: () => patchQuery({ line: start + CHUNK + 10 }) }, t("source.next")))),
          codeBlock({ lines: src.lines, start: src.start, language, marks, maxHeight: "68vh", label: file })],
        findings: () => [...dupes.items, ...dead.items].length ? [...dupes.items, ...dead.items].map((f) => h("div", { class: "card", style: { marginBottom: "8px" } }, h("div", { class: "row spread" }, h("a", { href: findingHref(f) }, h("strong", {}, f.title)), h("span", { class: "row" }, badge(t(f.type === "duplicate" ? "common.duplicate" : "common.deadCode"), f.type === "duplicate" ? "warning" : "danger"), stateBadge(f.state))))) : emptyState({ title: t("ws.noFindings"), iconName: "check" }),
        refs: () => [h("div", { class: "card", style: { marginBottom: "10px" } }, h("h3", {}, t("deps.incoming", { n: incoming.length })), incoming.map((p) => h("div", {}, h("a", { class: "mono", href: hrefTo("espacio", { file: p }) }, p)))),
          h("div", { class: "card" }, h("h3", {}, t("deps.outgoing", { n: outgoing.length })), outgoing.map((p) => h("div", {}, h("a", { class: "mono", href: hrefTo("espacio", { file: p }) }, p)))), note(t("ws.refsNote"), "", "info")],
        notes: () => notes.items.length ? notes.items.map((n) => h("div", { class: "card", style: { marginBottom: "8px" } }, h("a", { href: hrefTo("obsidian", { note: n.path }) }, h("strong", {}, n.title)), h("div", { class: "muted small" }, n.path))) : emptyState({ title: t("code.noNotes"), iconName: "note" }),
      };
      clear(center).append(h("div", { class: "row spread", style: { marginBottom: "10px" } }, h("h2", { class: "mono ellipsis", style: { fontSize: "14px" } }, file), h("div", { class: "row" }, h("button", { class: "btn sm", onclick: () => copy(file, t("common.pathCopied")) }, icon("copy", 14), t("common.copyPath")), h("a", { class: "btn sm", href: hrefTo("dependencias", { level: "file", focus: file }) }, icon("graph", 14), t("code.viewDeps")))), tabs, ...[].concat(body[tab]?.() || body.code()));
      clear(side).append(h("div", { class: "card inspector" }, h("h3", {}, t("ws.summary")),
        h("dl", { class: "kv", style: { margin: "10px 0" } }, h("dt", {}, t("dash.symbols")), h("dd", {}, fmt(syms.total)), h("dt", {}, t("code.col.language")), h("dd", {}, language || "—"), h("dt", {}, t("nav.duplicados")), h("dd", {}, fmt(dupes.total)), h("dt", {}, t("nav.muerto")), h("dd", {}, fmt(dead.total)), h("dt", {}, t("nav.obsidian")), h("dd", {}, fmt(notes.total))),
        h("div", { class: "section" }, h("h4", {}, t("ws.outline")), h("div", { class: "stack", style: { gap: "2px", maxHeight: "40vh", overflow: "auto" } }, syms.items.map((s) => h("button", { class: "btn ghost sm", style: { justifyContent: "flex-start" }, onclick: () => patchQuery({ line: s.line, tab: null }) }, kindIcon(s.kind), h("span", { class: "ellipsis" }, s.name), h("span", { class: "muted small" }, `:${s.line}`)))))));
    } catch (error) { if (!isAbort(error)) { clear(center).append(errorBox(error, () => show(params))); clear(side); } }
  }

  // Below 1025 px the explorer becomes a drawer, like the filters of the other pages.
  const explorerColumn = h("aside", { class: "filters-col" }, explorer);
  root.append(pageHead(t("ws.title"), t("ws.subtitle")), h("div", { class: "workbench inspect workspace" }, explorerColumn, h("div", {}, h("div", { class: "toolbar" }, drawerButton(() => explorerColumn, t("ws.explorer"))), center), side));
  await show(initial);
  const off = onQuery((params) => { if (params.get("file") !== current || params.get("tab") !== (tab === "code" ? null : tab) || params.get("line")) show(params); });
  return () => off();
}
