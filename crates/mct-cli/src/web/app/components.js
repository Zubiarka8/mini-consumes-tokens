// Reusable UI pieces: icons, badges, metric cards, tables, filters, pagination, states.
import { h, clear, fmt, debounce } from "./core.js";
import { t, hasKey } from "./i18n.js";

const dot = (x, y, r = 2) => `M${x - r} ${y}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
const ICONS = {
  dashboard: ["M3 3h7v9H3z", "M14 3h7v5h-7z", "M14 12h7v9h-7z", "M3 16h7v5H3z"],
  code: ["M16 18l6-6-6-6", "M8 6l-6 6 6 6"],
  copy: ["M9 9h11v11H9z", "M5 15H4V4h11v1"],
  skull: ["M3 6h18", "M8 6V4h8v2", "M6 6l1 14h10l1-14", "M10 10v6", "M14 10v6"],
  search: [dot(11, 11, 7), "M21 21l-4.3-4.3"],
  graph: [dot(6, 6), dot(18, 6), dot(12, 18), "M7.5 7.5l3.5 8", "M16.5 7.5l-3.5 8", "M8 6h8"],
  check: ["M20 6L9 17l-5-5"],
  checkCircle: ["M22 11.1V12a10 10 0 1 1-5.9-9.1", "M22 4L12 14l-3-3"],
  chart: ["M3 3v18h18", "M7 15l4-4 3 3 5-6"],
  note: ["M4 4h12a3 3 0 0 1 3 3v13H7a3 3 0 0 1-3-3z", "M4 17a3 3 0 0 1 3-3h12"],
  gear: ["M4 6h8", "M18 6h2", "M4 12h2", "M12 12h8", "M4 18h10", "M20 18h0", dot(15, 6, 2), dot(9, 12, 2), dot(17, 18, 2)],
  refresh: ["M21 12a9 9 0 1 1-3-6.7L21 8", "M21 3v5h-5"],
  bell: ["M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9", "M13.7 21a2 2 0 0 1-3.4 0"],
  sun: [dot(12, 12, 4), "M12 2v2", "M12 20v2", "M4.9 4.9l1.4 1.4", "M17.7 17.7l1.4 1.4", "M2 12h2", "M20 12h2", "M4.9 19.1l1.4-1.4", "M17.7 6.3l1.4-1.4"],
  moon: ["M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"],
  x: ["M18 6L6 18", "M6 6l12 12"],
  down: ["M6 9l6 6 6-6"], right: ["M9 6l6 6-6 6"], left: ["M15 6l-6 6 6 6"],
  folder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
  file: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5"],
  menu: ["M3 6h18", "M3 12h18", "M3 18h18"],
  filter: ["M3 5h18l-7 8v6l-4 2v-8z"],
  eye: ["M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z", dot(12, 12, 3)],
  external: ["M14 4h6v6", "M20 4l-9 9", "M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"],
  alert: ["M12 3l10 18H2z", "M12 10v5", "M12 18v.01"],
  info: [dot(12, 12, 10), "M12 11v5", "M12 8v.01"],
  func: ["M10 4c-3 0-3 3-3 5v6c0 2 0 5-3 5", "M5 12h6", "M14 10l6 8", "M20 10l-6 8"],
  box: ["M21 8l-9-5-9 5v8l9 5 9-5z", "M3 8l9 5 9-5", "M12 13v8"],
  tag: ["M20 12l-8 8-9-9V3h8z", "M7.5 7.5v.01"],
  link: ["M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1", "M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1"],
  plus: ["M12 5v14", "M5 12h14"],
  trash: ["M3 6h18", "M8 6V4h8v2", "M6 6l1 14h10l1-14"],
  edit: ["M12 20h9", "M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"],
  panel: ["M3 4h18v16H3z", "M9 4v16"],
  clock: [dot(12, 12, 10), "M12 7v5l3 2"],
  star: ["M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z"],
  layers: ["M12 3l9 5-9 5-9-5z", "M3 13l9 5 9-5"],
  download: ["M12 3v12", "M7 10l5 5 5-5", "M4 21h16"],
  hash: ["M5 9h14", "M5 15h14", "M10 3L8 21", "M16 3l-2 18"],
  lock: ["M5 11h14v10H5z", "M8 11V7a4 4 0 0 1 8 0v4"],
  home: ["M3 11l9-8 9 8", "M5 10v10h14V10"],
  bolt: ["M13 2L4 14h7l-1 8 9-12h-7z"],
  list: ["M8 6h13", "M8 12h13", "M8 18h13", "M3 6h.01", "M3 12h.01", "M3 18h.01"],
  split: ["M3 4h18v16H3z", "M12 4v16"],
};

export function icon(name, size = 18) {
  return h("svg:svg", { viewBox: "0 0 24 24", width: size, height: size, fill: "none", stroke: "currentColor", "stroke-width": 2, "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true", focusable: "false" },
    (ICONS[name] || ICONS.file).map((d) => h("svg:path", { d })));
}

// ---------- review states and symbol kinds ----------
export const STATES = {
  unreviewed: { label: "state.unreviewed", tone: "warning", color: "var(--warning)" },
  "in-review": { label: "state.inReview", tone: "info", color: "var(--info)" },
  confirmed: { label: "state.confirmed", tone: "danger", color: "var(--danger)" },
  "false-positive": { label: "state.falsePositive", tone: "primary", color: "var(--primary)" },
  resolved: { label: "state.resolved", tone: "success", color: "var(--success)" },
  dismissed: { label: "state.dismissed", tone: "", color: "var(--text-3)" },
  // Derived by the server, never stored: a decision about earlier code, and a
  // finding marked resolved that the detector still finds unchanged.
  "needs-reevaluation": { label: "state.needsReevaluation", tone: "warning", color: "var(--warning)" },
  "resolved-but-detected": { label: "state.resolvedButDetected", tone: "info", color: "var(--info)" },
};
export const STATE_ORDER = ["unreviewed", "needs-reevaluation", "in-review", "confirmed", "resolved-but-detected", "false-positive", "resolved", "dismissed"];
/**
 * Without `type` the generic label ("Confirmado"); with "duplicate" or "dead-code" the label says
 * what was decided ("Duplicado confirmado", "Falso positivo: sí se usa").
 */
export const stateLabel = (state, type) => {
  const generic = STATES[state]?.label || state;
  if (type === "duplicate" || type === "dead-code") {
    const key = `ctx.${type}.${state}`;
    if (hasKey(key)) return t(key);
  }
  return t(generic);
};

export const badge = (text, tone = "", iconName = null) => h("span", { class: `badge ${tone}` }, iconName && icon(iconName, 12), text);
export const stateBadge = (state, type) => badge(stateLabel(state, type), STATES[state]?.tone || "");

const KIND_GROUPS = [
  ["function", ["function", "method", "constructor"], "func", "info"],
  ["class", ["class", "struct", "interface", "trait", "impl", "object"], "box", "primary"],
  ["type", ["enum", "type_alias", "type", "union"], "layers", "accent"],
  ["variable", ["variable", "constant", "const", "static", "field", "property"], "hash", "success"],
  ["module", ["module", "namespace", "package", "heading", "section"], "folder", "warning"],
  ["import", ["import", "use", "include"], "link", "info"],
  ["macro", ["macro"], "bolt", "warning"],
];
export const kindGroup = (kind) => KIND_GROUPS.find(([, kinds]) => kinds.includes(kind))?.[0] || "other";
export const kindGroups = () => KIND_GROUPS.map(([id, kinds]) => ({ id, kinds }));
export function kindIcon(kind) {
  const entry = KIND_GROUPS.find(([, kinds]) => kinds.includes(kind));
  return h("span", { class: `kind-ico tone-${entry?.[3] || "info"}`, title: kind }, icon(entry?.[2] || "file", 15));
}

// ---------- structure ----------
export function pageHead(title, subtitle, tools) {
  return h("div", { class: "page-head" }, h("div", {}, h("h1", { id: "page-title", tabindex: -1 }, title), subtitle && h("p", {}, subtitle)), tools && h("div", { class: "page-tools" }, tools));
}

export function metricCard({ icon: name, tone = "primary", label, value, hint, href, onclick }) {
  const body = [h("span", { class: `ico tone-${tone}` }, icon(name, 20)), h("span", {}, h("div", { class: "v" }, value), h("div", { class: "l" }, label), hint && h("div", { class: "hint" }, hint))];
  return href ? h("a", { class: "card metric", href }, body) : onclick ? h("button", { class: "card metric", onclick }, body) : h("div", { class: "card metric", style: { cursor: "default" } }, body);
}

export const loading = (text = t("common.loading")) => h("div", { class: "loading", role: "status" }, h("div", { class: "spinner" }), text);

export function errorBox(error, retry) {
  return h("div", { class: "banner bad", role: "alert" }, icon("alert"), h("div", { class: "stack" },
    h("strong", {}, t("common.errorTitle")), h("span", {}, error?.message || String(error)),
    retry && h("div", {}, h("button", { class: "btn sm", onclick: retry }, t("common.retry")))));
}

export function emptyState({ title, text, actions = [], iconName = "search" }) {
  return h("div", { class: "empty" }, icon(iconName, 32), h("h3", {}, title), text && h("p", {}, text), actions.length > 0 && h("div", { class: "actions" }, actions));
}

export const note = (text, tone = "", iconName = "info") => h("div", { class: `banner ${tone}` }, icon(iconName), h("div", {}, text));

export function tabsBar({ tabs, active, onSelect, label }) {
  const bar = h("div", { class: "tabs", role: "tablist", "aria-label": label },
    tabs.map((tab) => h("button", { class: "tab", role: "tab", "aria-selected": String(tab.id === active), tabindex: tab.id === active ? 0 : -1, onclick: () => onSelect(tab.id) },
      tab.label, tab.count !== undefined && tab.count !== null && h("span", { class: "n" }, fmt(tab.count)))));
  bar.addEventListener("keydown", (event) => {
    if (!["ArrowLeft", "ArrowRight"].includes(event.key)) return;
    const buttons = [...bar.querySelectorAll("[role=tab]")];
    const next = buttons[(buttons.indexOf(document.activeElement) + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length];
    next?.focus(); next?.click();
  });
  return bar;
}

export function segmented({ options, value, onSelect, label }) {
  return h("div", { class: "seg", role: "group", "aria-label": label },
    options.map(([id, text]) => h("button", { "aria-pressed": String(id === value), onclick: () => onSelect(id) }, text)));
}

export function selectBox({ options, value, onChange, label, cls = "" }) {
  return h("select", { class: `select ${cls}`, "aria-label": label, onchange: (event) => onChange(event.target.value) },
    options.map(([id, text]) => h("option", { value: id, selected: id === value }, text)));
}

export function searchBox({ value = "", placeholder, label, onInput }) {
  const input = h("input", { class: "input wide", type: "search", value, placeholder, "aria-label": label, autocomplete: "off" });
  const notify = debounce(() => onInput(input.value), 250);
  input.addEventListener("input", notify);
  const box = h("div", { class: "grow" }, icon("search", 16), input);
  box.sync = (value) => { notify.cancel(); if (input.value !== value) input.value = value; };
  box.destroy = () => { notify.cancel(); input.removeEventListener("input", notify); };
  return box;
}

// ---------- filters ----------
export function filterGroup({ id, label, options, selected, onChange, search = false, limit = 8, open = true }) {
  const query = h("input", { class: "input wide", type: "search", placeholder: t("filters.searchOptions"), "aria-label": `${t("filters.searchOptions")}: ${label}` });
  const list = h("div", { role: "group", "aria-label": label });
  let expanded = false;
  const draw = () => {
    clear(list);
    const needle = query.value.trim().toLowerCase();
    const shown = options.filter((option) => !needle || String(option.label).toLowerCase().includes(needle));
    for (const option of expanded || needle ? shown : shown.slice(0, limit)) {
      const checkbox = h("input", { type: "checkbox", checked: selected.includes(option.value), onchange: (event) => {
        onChange(event.target.checked ? [...selected, option.value] : selected.filter((value) => value !== option.value));
      } });
      list.append(h("label", { class: "opt" }, checkbox, h("span", { class: "ellipsis" }, option.label), option.count !== undefined && h("span", { class: "n" }, fmt(option.count))));
    }
    if (!needle && shown.length > limit) list.append(h("button", { class: "btn ghost sm", onclick: () => { expanded = !expanded; draw(); } }, expanded ? t("filters.fewer") : t("filters.more", { n: shown.length - limit })));
    if (!shown.length) list.append(h("p", { class: "muted small" }, t("filters.noOptions")));
  };
  query.addEventListener("input", draw);
  draw();
  return h("details", { class: "fgroup", open, "data-group": id }, h("summary", {}, label), search && options.length > limit ? h("div", { style: { margin: "6px 0" } }, query) : null, list);
}

export function filterPanel({ groups, extra = [], onClear, active = 0, drawerRef }) {
  const aside = h("aside", { class: "filters-col", "aria-label": t("filters.title") },
    h("div", { class: "card filters" },
      h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("div", { class: "row" },
        h("button", { class: "btn ghost sm", disabled: active === 0, onclick: onClear }, t("filters.clear")),
        h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => aside.classList.remove("open") }, icon("x")))),
      groups.map((group) => filterGroup(group)), extra,
      h("p", { class: "logic" }, t("filters.logic"))));
  if (drawerRef) drawerRef.current = aside;
  return aside;
}

export const drawerButton = (getAside, label = t("filters.title")) => h("button", { class: "btn drawer-btn", onclick: () => getAside()?.classList.add("open") }, icon("filter", 16), label);

export function activeChips(chips, onClearAll) {
  if (!chips.length) return null;
  return h("div", { class: "chips", style: { marginBottom: "10px" }, "aria-label": t("filters.active") },
    chips.map((chip) => h("span", { class: "chip primary" }, chip.label,
      h("button", { "aria-label": t("filters.remove", { name: chip.label }), onclick: chip.remove }, icon("x", 11)))),
    h("button", { class: "btn ghost sm", onclick: onClearAll }, t("filters.clearAll")));
}

export function noResults({ onClear, extra = [] }) {
  return emptyState({ title: t("filters.noResults"), actions: [h("button", { class: "btn primary", onclick: onClear }, t("filters.clearAll")), ...extra] });
}

// ---------- table + pager ----------
export function dataTable({ columns, rows, rowKey, selected, onSelect, sort, order, onSort, caption }) {
  const head = h("tr", {}, columns.map((column) => {
    const sorted = sort === column.key;
    const label = column.sortable ? h("button", { onclick: () => onSort(column.key, sorted && order === "asc" ? "desc" : "asc") }, column.label, sorted ? (order === "asc" ? " ↑" : " ↓") : "") : column.label;
    return h("th", { scope: "col", class: `${column.num ? "num" : ""} ${column.cls || ""}`, "aria-sort": sorted ? (order === "asc" ? "ascending" : "descending") : null }, label);
  }));
  const body = h("tbody", {}, rows.map((row, index) => {
    const key = rowKey(row);
    const tr = h("tr", { tabindex: index === 0 || key === selected ? 0 : -1, "aria-selected": String(key === selected), "data-key": key,
      onclick: (event) => { if (!event.target.closest("a, button, input")) onSelect?.(row); } },
      columns.map((column) => h("td", { class: `${column.num ? "num" : ""} ${column.cls || ""}` }, column.render(row))));
    tr.addEventListener("keydown", (event) => {
      const sibling = event.key === "ArrowDown" ? tr.nextElementSibling : event.key === "ArrowUp" ? tr.previousElementSibling : null;
      if (sibling) { event.preventDefault(); tr.tabIndex = -1; sibling.tabIndex = 0; sibling.focus(); }
      else if ((event.key === "Enter" || event.key === " ") && event.target === tr) { event.preventDefault(); onSelect?.(row); }
    });
    return tr;
  }));
  return h("div", { class: "table-wrap" }, h("table", { class: "t" }, caption && h("caption", { class: "sr-only" }, caption), h("thead", {}, head), body));
}

export function pager({ page, size, total, onPage }) {
  const pages = Math.max(1, Math.ceil(total / size));
  if (total === 0) return null;
  const first = page * size + 1, last = Math.min(total, (page + 1) * size);
  const numbers = new Set([0, pages - 1, page - 1, page, page + 1].filter((n) => n >= 0 && n < pages));
  const buttons = [];
  let previous = -1;
  for (const n of [...numbers].sort((a, b) => a - b)) {
    if (n - previous > 1) buttons.push(h("span", { "aria-hidden": "true" }, "…"));
    buttons.push(h("button", { "aria-current": n === page ? "page" : null, "aria-label": t("pager.page", { n: n + 1 }), onclick: () => onPage(n) }, n + 1));
    previous = n;
  }
  return h("nav", { class: "pager", "aria-label": t("pager.label") },
    h("span", {}, t("pager.showing", { first: fmt(first), last: fmt(last), total: fmt(total) })),
    h("div", { class: "pages" }, h("button", { disabled: page === 0, "aria-label": t("pager.prev"), onclick: () => onPage(page - 1) }, "‹"), buttons,
      h("button", { disabled: page >= pages - 1, "aria-label": t("pager.next"), onclick: () => onPage(page + 1) }, "›")));
}

// ---------- file tree ----------
/** Builds a nested folder structure from [path, language, symbols] rows. */
export function buildTree(files) {
  const root = { name: "", path: "", dirs: new Map(), files: [], count: 0 };
  for (const [path, language, symbols] of files) {
    const parts = path.split("/");
    let node = root;
    node.count += 1;
    for (let index = 0; index < parts.length - 1; index += 1) {
      const name = parts[index];
      if (!node.dirs.has(name)) node.dirs.set(name, { name, path: parts.slice(0, index + 1).join("/"), dirs: new Map(), files: [], count: 0 });
      node = node.dirs.get(name);
      node.count += 1;
    }
    node.files.push({ name: parts[parts.length - 1], path, language, symbols });
  }
  return root;
}

export function treeView({ root, current, onSelect, expanded = new Set() }) {
  const render = (node, depth) => {
    const list = h("ul", { role: depth === 0 ? "tree" : "group", class: depth === 0 ? "tree" : "" });
    for (const dir of [...node.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))) {
      const open = expanded.has(dir.path) || (depth === 0 && node.dirs.size <= 3);
      const item = h("li", { role: "treeitem", "aria-expanded": String(open) });
      const button = h("button", { class: "node", "aria-current": String(current === dir.path), title: dir.path, onclick: () => {
        if (expanded.has(dir.path)) expanded.delete(dir.path); else expanded.add(dir.path);
        onSelect(dir.path, true);
      } }, icon(open ? "down" : "right", 13), icon("folder", 14), h("span", { class: "ellipsis" }, dir.name), h("span", { class: "n" }, fmt(dir.count)));
      item.append(button);
      if (open) item.append(render(dir, depth + 1));
      list.append(item);
    }
    for (const file of node.files.sort((a, b) => a.name.localeCompare(b.name)).slice(0, 300)) {
      list.append(h("li", { role: "treeitem" }, h("button", { class: "node", "aria-current": String(current === file.path), title: file.path, onclick: () => onSelect(file.path, false) },
        h("span", { style: { width: "13px" } }), icon("file", 14), h("span", { class: "ellipsis" }, file.name), h("span", { class: "n" }, fmt(file.symbols)))));
    }
    return list;
  };
  return render(root, 0);
}

/** Wraps an async loader with loading and error states inside `host`. */
export async function load(host, loader, draw) {
  clear(host).append(loading());
  try {
    const data = await loader();
    clear(host);
    draw(data);
  } catch (error) {
    if (error?.name === "AbortError") return;
    clear(host).append(errorBox(error, () => load(host, loader, draw)));
  }
}

/** Reveal a bounded batch without removing the focused Show more control. */
export function pagedLinks(items, renderItem, size = 30) {
  const list = h("div", {}), status = h("span", { class: "muted small", role: "status" });
  let shown = 0;
  const more = h("button", { class: "btn sm", onclick: () => reveal() }, t("common.showMore"));
  function reveal() {
    const end = Math.min(items.length, shown + size);
    list.append(items.slice(shown, end).map(renderItem));
    shown = end;
    status.textContent = `${fmt(shown)} / ${fmt(items.length)}`;
    more.disabled = shown === items.length;
  }
  reveal();
  return h("div", {}, list, items.length > size && h("div", { class: "row", style: { marginTop: "8px" } }, more, status));
}
