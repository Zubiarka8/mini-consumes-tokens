// 08 · Workspace de código unificado: explorer | source | everything known about the file
import { h, clear, api, isAbort, patchQuery, onQuery, fmt, hrefTo, copy, basename } from "./core.js";
import {
  icon,
  pageHead,
  tabsBar,
  badge,
  stateBadge,
  buildTree,
  treeView,
  loading,
  errorBox,
  emptyState,
  kindIcon,
  note,
  docTabs,
  breadcrumb,
} from "./components.js";
import { codeBlock } from "./code.js";
import { findingHref, findingNumber } from "./page-findings.js";
import { ignoreCard } from "./ignore-card.js";
import { t } from "./i18n.js";

const CHUNK = 300;
// The rule files the explorer can open in place of the code viewer, by their `view` in the URL.
const RULE_VIEWS = { ignore: "mctignore", gitignore: "gitignore" };
const ruleView = (params) => (Object.hasOwn(RULE_VIEWS, params.get("view")) ? params.get("view") : null);

export async function render({ root, params: initial }) {
  const files = (await api("files")).files;
  const tree = buildTree(files);
  const expanded = new Set();
  const explorer = h("div", {
    class: "card filters explorer",
  });
  // On narrow screens the explorer is an off-canvas drawer, opened from the page header.
  const drawer = h("aside", { class: "filters-col" }, explorer);
  const center = h("div", {});
  const side = h("div", { class: "inspector-col sticky" });
  let current = null;
  let tab = "code";
  // "ignore" or "gitignore" swaps the code viewer for that rule file's editor; the file stays selected underneath.
  let view = null;

  // `reveal` opens the folders holding the file; toggling a folder redraws without it, so it can close.
  function drawExplorer(path, reveal = true) {
    if (reveal) {
      const parts = (path || "").split("/");
      parts.pop();
      parts.reduce((acc, part) => {
        const next = acc ? `${acc}/${part}` : part;
        expanded.add(next);
        return next;
      }, "");
    }
    clear(explorer).append(
      h(
        "div",
        { class: "row spread mb-2" },
        h("h2", {}, t("ws.explorer")),
        h(
          "button",
          {
            class: "icon-btn only-mobile",
            "aria-label": t("common.close"),
            onclick: () => drawer.classList.remove("open"),
          },
          icon("x"),
        ),
      ),
      ...["ignore", "gitignore"].map((id) =>
        h(
          "button",
          {
            class: "btn sm w-full justify-start mb-2",
            type: "button",
            "aria-pressed": String(view === id),
            title: t(`ws.${id}Hint`),
            onclick: () => patchQuery({ view: view === id ? null : id }),
          },
          icon("filter", 14),
          t(`ws.${id}`),
        ),
      ),
      treeView({
        root: tree,
        current: path,
        expanded,
        onSelect: (p, dir) => {
          if (dir) {
            drawExplorer(path, false);
            return;
          }
          drawer.classList.remove("open");
          patchQuery({ file: p, line: null, tab: null, view: null });
        },
      }),
    );
  }

  async function show(params) {
    const file = params.get("file");
    tab = params.get("tab") || "code";
    view = ruleView(params);
    drawExplorer(file);
    if (view) {
      current = file;
      clear(center).append(ignoreCard(RULE_VIEWS[view]));
      clear(side);
      return;
    }
    if (!file) {
      clear(center).append(
        emptyState({ iconName: "layers", title: t("ws.pickTitle"), text: t("ws.pickText") }),
      );
      clear(side);
      return;
    }
    current = file;
    clear(center).append(loading());
    clear(side).append(h("div", { class: "card inspector" }, loading()));
    try {
      const line = Number(params.get("line") || 1);
      const start = Math.max(1, line - 10) || 1;
      const [syms, dupes, dead, notes, deps, src] = await Promise.all([
        api("symbols", { dir: file, size: 200, sort: "line" }, "ws-sym"),
        api("findings", { type: "duplicate", q: file, state: [], size: 50 }, "ws-dup"),
        api("findings", { type: "dead-code", dir: file, state: [], size: 50 }, "ws-dead"),
        api("notes", { q: file, size: 20 }, "ws-notes"),
        api("dependencies", { level: "file", focus: file, depth: 1 }, "ws-deps"),
        api("source", { path: file, start, end: start + CHUNK - 1 }, "ws-src"),
      ]);
      const language = src.language || "";
      const outgoing = deps.edges.filter((e) => e.from === file).map((e) => e.to);
      const incoming = deps.edges.filter((e) => e.to === file).map((e) => e.from);
      const marks = line ? { [line]: "hl" } : {};
      const tabs = tabsBar({
        label: t("ws.tabs"),
        active: tab,
        onSelect: (id) => patchQuery({ tab: id === "code" ? null : id }),
        tabs: [
          { id: "code", label: t("ws.code") },
          { id: "findings", label: t("ws.findings"), count: dupes.total + dead.total },
          { id: "refs", label: t("ws.refs"), count: incoming.length + outgoing.length },
          { id: "notes", label: t("ws.notes"), count: notes.total },
        ],
      });
      const body = {
        code: () => [
          h(
            "div",
            { class: "row spread mb-2" },
            h(
              "span",
              { class: "muted small" },
              t("source.range", { from: src.start, to: src.start + src.lines.length - 1 }),
            ),
            h(
              "div",
              { class: "row" },
              h(
                "button",
                {
                  class: "btn sm",
                  disabled: start <= 1,
                  onclick: () => patchQuery({ line: Math.max(1, start - CHUNK + 10) }),
                },
                t("source.prev"),
              ),
              h(
                "button",
                {
                  class: "btn sm",
                  disabled: src.lines.length < CHUNK,
                  onclick: () => patchQuery({ line: start + CHUNK + 10 }),
                },
                t("source.next"),
              ),
            ),
          ),
          codeBlock({ lines: src.lines, start: src.start, language, marks, maxHeight: "68vh", label: file }),
        ],
        findings: () =>
          [...dupes.items, ...dead.items].length
            ? [...dupes.items, ...dead.items].map((f) =>
                h(
                  "div",
                  { class: "card mb-2" },
                  h(
                    "div",
                    { class: "row spread" },
                    h(
                      "a",
                      { href: findingHref(f) },
                      h("span", { class: "finding-no" }, findingNumber(f)),
                      " · ",
                      h("strong", {}, f.title),
                    ),
                    h(
                      "span",
                      { class: "row" },
                      badge(
                        t(f.type === "duplicate" ? "common.duplicate" : "common.deadCode"),
                        f.type === "duplicate" ? "warning" : "danger",
                      ),
                      stateBadge(f.state),
                    ),
                  ),
                ),
              )
            : emptyState({ title: t("ws.noFindings"), iconName: "check" }),
        refs: () => [
          h(
            "div",
            { class: "card mb-3" },
            h("h3", {}, t("deps.incoming", { n: incoming.length })),
            incoming.map((p) =>
              h("div", {}, h("a", { class: "mono", href: hrefTo("espacio", { file: p }) }, p)),
            ),
          ),
          h(
            "div",
            { class: "card" },
            h("h3", {}, t("deps.outgoing", { n: outgoing.length })),
            outgoing.map((p) =>
              h("div", {}, h("a", { class: "mono", href: hrefTo("espacio", { file: p }) }, p)),
            ),
          ),
          note(t("ws.refsNote"), "", "info"),
        ],
        notes: () =>
          notes.items.length
            ? notes.items.map((n) =>
                h(
                  "div",
                  { class: "card mb-2" },
                  h("a", { href: hrefTo("obsidian", { note: n.path }) }, h("strong", {}, n.title)),
                  h("div", { class: "muted small" }, n.path),
                ),
              )
            : emptyState({ title: t("code.noNotes"), iconName: "note" }),
      };
      const close = () => patchQuery({ file: null, line: null, tab: null });
      clear(center).append(
        docTabs({
          cls: "pane",
          label: t("tabs.files"),
          tabs: [{ icon: "file", label: basename(file), active: true, onClose: close }],
        }),
        h(
          "div",
          { class: "pane-body" },
          h(
            "div",
            { class: "row spread mb-2" },
            breadcrumb(file),
            h(
              "div",
              { class: "row" },
              h(
                "button",
                { class: "btn sm", onclick: () => copy(file, t("common.pathCopied")) },
                icon("copy", 14),
                t("common.copyPath"),
              ),
              h(
                "a",
                { class: "btn sm", href: hrefTo("dependencias", { level: "file", focus: file }) },
                icon("graph", 14),
                t("code.viewDeps"),
              ),
            ),
          ),
          tabs,
          ...[].concat(body[tab]?.() || body.code()),
        ),
      );
      clear(side).append(
        h(
          "div",
          { class: "card inspector" },
          h("h3", {}, t("ws.summary")),
          h(
            "dl",
            { class: "kv my-3" },
            h("dt", {}, t("dash.symbols")),
            h("dd", {}, fmt(syms.total)),
            h("dt", {}, t("code.col.language")),
            h("dd", {}, language || "—"),
            h("dt", {}, t("nav.duplicados")),
            h("dd", {}, fmt(dupes.total)),
            h("dt", {}, t("nav.muerto")),
            h("dd", {}, fmt(dead.total)),
            h("dt", {}, t("nav.obsidian")),
            h("dd", {}, fmt(notes.total)),
          ),
          h(
            "div",
            { class: "section" },
            h("h4", {}, t("ws.outline")),
            h(
              "div",
              { class: "stack outline-list" },
              syms.items.map((s) =>
                h(
                  "button",
                  {
                    class: "btn ghost sm justify-start",

                    onclick: () => patchQuery({ line: s.line, tab: null }),
                  },
                  kindIcon(s.kind),
                  h("span", { class: "ellipsis" }, s.name),
                  h("span", { class: "muted small" }, `:${s.line}`),
                ),
              ),
            ),
          ),
        ),
      );
    } catch (error) {
      if (!isAbort(error)) {
        clear(center).append(errorBox(error, () => show(params)));
        clear(side);
      }
    }
  }

  root.append(
    pageHead(t("ws.title"), t("ws.subtitle"), [
      h(
        "button",
        { class: "btn drawer-btn", onclick: () => drawer.classList.add("open") },
        icon("folder", 16),
        t("ws.explorer"),
      ),
    ]),
    h(
      "div",
      {
        class: "workbench inspect workspace-layout",
      },
      drawer,
      center,
      side,
    ),
  );
  await show(initial);
  const off = onQuery((params) => {
    if (
      ruleView(params) !== view ||
      params.get("file") !== current ||
      params.get("tab") !== (tab === "code" ? null : tab) ||
      params.get("line")
    )
      show(params);
  });
  return () => off();
}
