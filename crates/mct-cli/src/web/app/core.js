// Shared runtime: DOM builder, API client, routing state, formatting, dialogs, toasts.
import { t, getLang } from "./i18n.js";

// Element.append behaves like h(): arrays are flattened and null/undefined/false are skipped,
// so conditional children can be passed everywhere without printing "null" or "[object …]".
const nativeAppend = Element.prototype.append;
Element.prototype.append = function append(...nodes) {
  return nativeAppend.apply(this, nodes.flat(Infinity).filter((node) => node !== null && node !== undefined && node !== false));
};

const SVG_NS = "http://www.w3.org/2000/svg";
const BOOLEAN_PROPS = new Set(["disabled", "checked", "hidden", "selected", "open", "readOnly", "required"]);

/** Builds DOM nodes. Strings are always text: there is no HTML injection path. */
export function h(tag, attrs, ...children) {
  const svg = tag.startsWith("svg:");
  const el = svg ? document.createElementNS(SVG_NS, tag.slice(4)) : document.createElement(tag);
  for (const [key, value] of Object.entries(attrs || {})) {
    if (value === undefined || value === null || value === false) continue;
    if (key.startsWith("on") && typeof value === "function") el.addEventListener(key.slice(2), value);
    else if (key === "class") el.setAttribute("class", value);
    else if (key === "style" && typeof value === "object") Object.assign(el.style, value);
    else if (BOOLEAN_PROPS.has(key) && !svg) el[key] = Boolean(value);
    else if (key === "value" && !svg) el.value = value;
    else el.setAttribute(key, value === true ? "" : String(value));
  }
  append(el, children);
  return el;
}

export function append(el, children) {
  for (const child of children.flat(Infinity)) {
    if (child === null || child === undefined || child === false) continue;
    el.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return el;
}

export const clear = (el) => { el.replaceChildren(); return el; };

// ---------- storage (best effort: private windows may block it) ----------
export const store = {
  get(key, fallback = null) {
    try { const raw = localStorage.getItem(`mct:${key}`); return raw === null ? fallback : JSON.parse(raw); } catch { return fallback; }
  },
  set(key, value) { try { localStorage.setItem(`mct:${key}`, JSON.stringify(value)); } catch { /* ignore */ } },
};

// ---------- API ----------
export class ApiError extends Error {
  constructor(status, message, body = null) { super(message); this.status = status; this.body = body; }
}

const pending = new Map();

export function cancelApi(key) {
  pending.get(key)?.abort();
  pending.delete(key);
}

/** Fetches `api/<path>`; a newer call with the same `key` aborts the stale one. */
export async function api(path, params = {}, key = null) {
  const query = new URLSearchParams();
  for (const [name, value] of Object.entries(params)) {
    const text = Array.isArray(value) ? value.join(",") : value;
    if (text === "" || text === null || text === undefined || text === false) continue;
    query.set(name, String(text));
  }
  let controller;
  if (key) { cancelApi(key); controller = new AbortController(); pending.set(key, controller); }
  const suffix = query.toString();
  try {
    const response = await fetch(`api/${path}${suffix ? `?${suffix}` : ""}`, { signal: controller?.signal, headers: { Accept: "application/json" } });
    if (!response.ok) {
      let message = response.statusText;
      try { message = (await response.json()).error || message; } catch { /* plain text error */ }
      throw new ApiError(response.status, message);
    }
    return await response.json();
  } finally {
    if (controller && pending.get(key) === controller) pending.delete(key);
  }
}

export const isAbort = (error) => error?.name === "AbortError";

let csrf = "";
export const setCsrf = (value) => { csrf = value; };

/** Records (or clears, with state = null) a review decision based on the
 *  `revision` the page showed. A 409 error carries the current review in `body`. */
export async function review(key, state, note = "", revision = "none") {
  const response = await fetch("api/review", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ csrf, key, state, note, revision }),
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new ApiError(response.status, body.error || response.statusText, body);
  return body;
}

/** Replaces .mctignore. `base` is the text the page loaded; a 409 error carries the current state in `body`. */
export async function saveIgnore(content, base) {
  const response = await fetch("api/ignore", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ csrf, content, base }),
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new ApiError(response.status, body.error || response.statusText, body);
  return body;
}

// ---------- routing: #/page?query ----------
export const PAGES = [
  { id: "panel", icon: "dashboard", label: "nav.panel" },
  { id: "codigo", icon: "code", label: "nav.codigo" },
  { id: "duplicados", icon: "copy", label: "nav.duplicados" },
  { id: "codigo-muerto", icon: "skull", label: "nav.muerto" },
  { id: "busqueda", icon: "search", label: "nav.busqueda" },
  { id: "dependencias", icon: "graph", label: "nav.dependencias" },
  { id: "revisiones", icon: "check", label: "nav.revisiones" },
  { id: "estadisticas", icon: "chart", label: "nav.estadisticas" },
  { id: "obsidian", icon: "note", label: "nav.obsidian" },
  { id: "configuracion", icon: "gear", label: "nav.configuracion" },
];
const EXTRA_PAGES = ["espacio"];
export const isPage = (id) => PAGES.some((page) => page.id === id) || EXTRA_PAGES.includes(id);

/** Last query of each page, so filters and searches survive tab changes. */
const memory = new Map(Object.entries(store.get("filters", {})));
export const parseHash = () => {
  const raw = location.hash.replace(/^#\/?/, "");
  const [page, query = ""] = raw.split("?");
  return { page: isPage(page) ? page : "panel", params: new URLSearchParams(query) };
};
export const hrefTo = (page, params) => {
  const query = params === undefined ? memory.get(page) || "" : new URLSearchParams(params).toString();
  return `#/${page}${query ? `?${query}` : ""}`;
};
export const go = (page, params) => { location.hash = hrefTo(page, params); };
export function remember(page, params) {
  const query = params.toString();
  if (query) memory.set(page, query); else memory.delete(page);
  store.set("filters", Object.fromEntries(memory));
}
export function forget(page) { memory.delete(page); store.set("filters", Object.fromEntries(memory)); }

const queryListeners = new Set();
/** Pages subscribe to in-page query changes (filters, tabs, selection) without a full re-render. */
export function onQuery(fn) { queryListeners.add(fn); return () => queryListeners.delete(fn); }

/** Applies `patch` ({key: value|null|[]}) to the current query, keeps it for the page and notifies listeners. */
export function patchQuery(patch) {
  const { page, params } = parseHash();
  const keepsPage = "page" in patch || ["id", "key", "sel", "note", "file", "line", "focus"].some((key) => key in patch);
  for (const [key, value] of Object.entries(patch)) {
    const text = Array.isArray(value) ? value.join(",") : value;
    if (text === null || text === undefined || text === "" || text === false) params.delete(key); else params.set(key, String(text));
  }
  if (!keepsPage) params.delete("page");
  remember(page, params);
  history.replaceState(null, "", `#/${page}${params.toString() ? `?${params}` : ""}`);
  queryListeners.forEach((fn) => fn(params));
}

export const listParam = (params, key) => (params.get(key) || "").split(",").filter(Boolean);

// ---------- formatting ----------
const LOCALE = getLang() === "en" ? "en-GB" : "es-ES";
const number = new Intl.NumberFormat(LOCALE);
export const fmt = (value, options) => (value === null || value === undefined ? "—" : (options ? new Intl.NumberFormat(LOCALE, options) : number).format(value));
export function bytes(value) {
  if (value === null || value === undefined) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let size = value, unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
  return `${size.toLocaleString(LOCALE, { maximumFractionDigits: unit ? 1 : 0 })} ${units[unit]}`;
}
export function duration(ms) {
  if (ms === null || ms === undefined) return "—";
  if (ms < 1000) return `${fmt(ms)} ms`;
  return `${(ms / 1000).toLocaleString(LOCALE, { maximumFractionDigits: 1 })} s`;
}
const relative = new Intl.RelativeTimeFormat(getLang(), { numeric: "auto" });
export function ago(unixSeconds) {
  if (!unixSeconds) return "—";
  const diff = unixSeconds - Date.now() / 1000;
  const steps = [[60, "second", 1], [3600, "minute", 60], [86400, "hour", 3600], [2592000, "day", 86400], [31536000, "month", 2592000], [Infinity, "year", 31536000]];
  for (const [limit, unit, size] of steps) if (Math.abs(diff) < limit) return relative.format(Math.round(diff / size), unit);
  return "";
}
export const dateTime = (unix) => (unix ? new Date(unix * 1000).toLocaleString(LOCALE, { dateStyle: "medium", timeStyle: "short" }) : "—");
export const shortDate = (unix) => new Date(unix * 1000).toLocaleDateString(LOCALE, { day: "numeric", month: "short" });
export const basename = (path) => path.split("/").pop();

/** Splits `text` around case-insensitive `query` matches and wraps them in <mark>. */
export function highlight(text, query) {
  const source = String(text ?? "");
  const words = [...new Set(String(query || "").trim().split(/\s+/).filter((w) => w.length > 0))];
  if (!words.length) return source;
  const pattern = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "ig");
  return source.split(pattern).map((part, index) => (index % 2 ? h("mark", {}, part) : part));
}

// ---------- toasts, dialogs ----------
export function toast(message, kind = "info", ms = 4500) {
  const host = document.querySelector(".toasts");
  if (!host) return;
  const el = h("div", { class: `toast ${kind}`, role: kind === "err" ? "alert" : "status" }, message);
  host.append(el);
  setTimeout(() => el.remove(), ms);
}

export async function copy(text, label = t("common.copied")) {
  try { await navigator.clipboard.writeText(text); toast(label, "ok"); }
  catch { toast(t("common.copyFailed"), "err"); }
}

function openDialog(build, { wide = false } = {}) {
  return new Promise((resolve) => {
    const dialog = h("dialog", { class: wide ? "wide" : "", "aria-modal": "true" });
    const close = (value) => { dialog.close(); dialog.remove(); resolve(value); };
    dialog.addEventListener("cancel", (event) => { event.preventDefault(); close(null); });
    dialog.append(build(close));
    document.body.append(dialog);
    dialog.showModal();
  });
}

/** Resolves true/false. Use for every action that is hard to undo. */
export function confirmDialog({ title, body, confirm = t("common.confirm"), danger = false }) {
  return openDialog((close) => h("div", { class: "dlg" },
    h("h2", {}, title), h("p", { class: "muted" }, body),
    h("footer", {},
      h("button", { class: "btn", onclick: () => close(false), autofocus: true }, t("common.cancel")),
      h("button", { class: `btn ${danger ? "danger" : "primary"}`, onclick: () => close(true) }, confirm))));
}

/** Asks for an optional/required note. Resolves the note string, or null when cancelled. */
export function noteDialog({ title, body, required = false, confirm = t("common.save"), initial = "" }) {
  return openDialog((close) => {
    const field = h("textarea", { class: "input wide", rows: 9, maxlength: 500, "aria-label": t("review.noteLabel"), value: initial });
    const submit = h("button", { class: "btn primary", disabled: required && !initial.trim(), onclick: () => close(field.value.trim()) }, confirm);
    field.addEventListener("input", () => { submit.disabled = required && !field.value.trim(); });
    return h("div", { class: "dlg" }, h("h2", {}, title), body && h("p", { class: "muted" }, body),
      h("label", { class: "field" }, required ? t("review.reasonRequired") : t("review.noteOptional"), field),
      h("footer", {}, h("button", { class: "btn", onclick: () => close(null) }, t("common.cancel")), submit));
  });
}

export { openDialog };

// ---------- misc ----------
export function debounce(fn, ms) {
  let timer;
  const wrapped = (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
  wrapped.cancel = () => clearTimeout(timer);
  return wrapped;
}

export function applyTheme(choice) {
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.dataset.theme = choice === "system" ? (dark ? "dark" : "light") : choice;
}
