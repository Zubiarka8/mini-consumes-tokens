// Global search box + command palette (Ctrl/Cmd+K): indexed, bounded, cancellable queries.
import { h, clear, api, isAbort, store, debounce, highlight, go, hrefTo, PAGES } from "./core.js";
import { icon, kindIcon } from "./components.js";
import { t } from "./i18n.js";

const MAX_RECENTS = 6;
const SCOPES = [
  ["all", "search.scope.all"],
  ["code", "search.scope.code"],
  ["symbols", "search.scope.symbols"],
  ["files", "search.scope.files"],
  ["duplicates", "search.scope.duplicates"],
  ["deadCode", "search.scope.deadCode"],
  ["obsidian", "search.scope.obsidian"],
  ["dependencies", "search.scope.dependencies"],
];
export const SCOPE_OPTIONS = SCOPES;

export function targetOf(item) {
  switch (item.type) {
    case "symbol":
      return hrefTo("codigo", { q: item.title, sel: item.symbolId ?? "" });
    case "file":
    case "dependency":
      return hrefTo("espacio", { file: item.path });
    case "note":
      return hrefTo("obsidian", { note: item.path });
    case "duplicate":
      return hrefTo("duplicados", { key: item.key });
    case "dead-code":
      return hrefTo("codigo-muerto", { key: item.key });
    default:
      return hrefTo("espacio", { file: item.path, line: item.line });
  }
}

export function createGlobalSearch({ commands }) {
  let scope = store.get("scope", "all");
  let items = [];
  let active = -1;
  let controller = 0;
  const input = h("input", {
    type: "search",
    id: "gsearch",
    placeholder: t("search.placeholder"),
    "aria-label": t("search.label"),
    role: "combobox",
    "aria-expanded": "false",
    "aria-controls": "gsearch-list",
    "aria-autocomplete": "list",
    autocomplete: "off",
  });
  const list = h("div", { class: "suggest", id: "gsearch-list", role: "listbox", hidden: true });
  const select = h(
    "select",
    {
      "aria-label": t("search.scopeLabel"),
      onchange: () => {
        scope = select.value;
        store.set("scope", scope);
        run();
      },
    },
    SCOPES.map(([id, label]) => h("option", { value: id, selected: id === scope }, t(label))),
  );
  const clearButton = h(
    "button",
    {
      class: "icon-btn",
      hidden: true,
      "aria-label": t("search.clear"),
      onclick: () => {
        input.value = "";
        clearButton.hidden = true;
        input.focus();
        run();
      },
    },
    icon("x", 16),
  );
  const hint = h(
    "span",
    { class: "kbd hide-sm", "aria-hidden": "true" },
    /Mac|iPhone/.test(navigator.platform) ? "⌘ K" : "Ctrl K",
  );
  const root = h(
    "div",
    { class: "gsearch", role: "search" },
    icon("search", 18),
    input,
    clearButton,
    hint,
    select,
    list,
  );

  const close = () => {
    list.hidden = true;
    input.setAttribute("aria-expanded", "false");
    active = -1;
  };
  const open = () => {
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
  };
  const choose = (item) => {
    if (!item) return;
    if (item.run) item.run();
    else if (item.href) {
      const recents = store.get("recent-items", []).filter((r) => r.href !== item.href);
      recents.unshift({ title: item.title, sub: item.sub, type: item.type, href: item.href });
      store.set("recent-items", recents.slice(0, MAX_RECENTS));
      location.hash = item.href;
    }
    close();
    input.blur();
  };
  const submitSearch = () => {
    const q = input.value.trim();
    if (!q) return;
    const recents = store.get("recent-searches", []).filter((r) => r !== q);
    recents.unshift(q);
    store.set("recent-searches", recents.slice(0, MAX_RECENTS));
    go("busqueda", { q, scope });
    close();
    input.blur();
  };

  const section = (title, rows, query) =>
    rows.length
      ? [
          h("h4", { role: "presentation" }, title),
          ...rows.map((row) => {
            const idx = items.length;
            items.push(row);
            const option = h(
              "div",
              {
                role: "option",
                id: `gs-${idx}`,
                "aria-selected": "false",
                onmousedown: (event) => {
                  event.preventDefault();
                  choose(row);
                },
              },
              row.kind ? kindIcon(row.kind) : icon(row.icon || "file", 16),
              h("span", { class: "ellipsis" }, highlight(row.title, query)),
              row.sub && h("span", { class: "sub mono" }, row.sub),
            );
            return option;
          }),
        ]
      : [];

  function paint(groups, query, message) {
    items = [];
    clear(list);
    const body = [];
    if (message) body.push(h("p", { class: "muted palette-message", role: "status" }, message));
    for (const [title, rows] of groups) body.push(...section(title, rows, query));
    if (query)
      body.push(
        h(
          "div",
          {
            role: "option",
            id: `gs-${items.length}`,
            "aria-selected": "false",
            onmousedown: (e) => {
              e.preventDefault();
              submitSearch();
            },
          },
          icon("search", 16),
          h("span", {}, t("search.allResults", { q: query })),
        ),
      );
    if (query) items.push({ run: submitSearch });
    list.append(...body);
    active = -1;
    open();
  }

  const commandRows = (query) =>
    commands()
      .filter((c) => !query || c.title.toLowerCase().includes(query.toLowerCase()))
      .slice(0, 8);

  async function run() {
    const query = input.value.trim();
    clearButton.hidden = !input.value;
    controller += 1;
    const mine = controller;
    if (!query) {
      const recents = store.get("recent-items", []).map((r) => ({ ...r, icon: "clock" }));
      const searches = store.get("recent-searches", []).map((q) => ({
        title: q,
        icon: "search",
        run: () => {
          input.value = q;
          clearButton.hidden = false;
          run();
          input.focus();
        },
      }));
      paint(
        [
          [t("palette.recentFiles"), recents],
          [t("search.recent"), searches],
          [t("palette.commands"), commandRows("")],
        ],
        "",
      );
      return;
    }
    paint([[t("palette.commands"), commandRows(query)]], query, t("palette.searching"));
    try {
      const data = await api("search", { q: query, scope }, "palette");
      if (mine !== controller) return;
      const g = data.groups;
      const rows = (name, map) => (g[name]?.items || []).slice(0, 5).map(map);
      paint(
        [
          [
            t("palette.files"),
            rows("files", (i) => ({ ...i, icon: "file", sub: i.path, href: targetOf(i) })),
          ],
          [
            t("palette.symbols"),
            rows("symbols", (i) => ({ ...i, sub: `${i.path}:${i.line}`, href: targetOf(i) })),
          ],
          [
            t("palette.notes"),
            rows("obsidian", (i) => ({ ...i, icon: "note", sub: i.path, href: targetOf(i) })),
          ],
          [
            t("palette.findings"),
            [
              ...rows("duplicates", (i) => ({ ...i, icon: "copy", sub: i.path, href: targetOf(i) })),
              ...rows("deadCode", (i) => ({ ...i, icon: "skull", sub: i.path, href: targetOf(i) })),
            ],
          ],
          [t("palette.commands"), commandRows(query)],
        ],
        query,
        Object.values(g).every((x) => !x.total) ? t("palette.none") : "",
      );
    } catch (error) {
      if (!isAbort(error) && mine === controller)
        paint([], query, t("palette.error", { message: error.message }));
    }
  }
  const debounced = debounce(run, 180);
  input.addEventListener("input", () => {
    clearButton.hidden = !input.value;
    debounced();
  });
  input.addEventListener("focus", () => {
    run();
  });
  input.addEventListener("blur", () => setTimeout(close, 120));
  input.addEventListener("keydown", (event) => {
    const options = [...list.querySelectorAll("[role=option]")];
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!options.length) return;
      active = (active + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length;
      options.forEach((o, i) => o.setAttribute("aria-selected", String(i === active)));
      options[active].scrollIntoView({ block: "nearest" });
      input.setAttribute("aria-activedescendant", options[active].id);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (active >= 0) choose(items[active]);
      else submitSearch();
    } else if (event.key === "Escape") {
      if (input.value) {
        input.value = "";
        run();
      } else {
        close();
        input.blur();
      }
    }
  });
  document.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      input.focus();
      input.select();
    } else if (
      event.key === "/" &&
      !/INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName || "") &&
      !document.activeElement?.isContentEditable
    ) {
      event.preventDefault();
      input.focus();
    }
  });
  return {
    el: root,
    focus: () => input.focus(),
    setValue: (v) => {
      input.value = v;
      clearButton.hidden = !v;
    },
  };
}

export const pageCommands = () =>
  PAGES.map((page) => ({
    title: `${t("palette.goTo")} ${t(page.label)}`,
    icon: page.icon,
    run: () => go(page.id),
  }));
