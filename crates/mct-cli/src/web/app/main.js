// Application shell: sidebar, header, global search, theme, notifications and page routing.
import { h, clear, api, setCsrf, store, parseHash, hrefTo, go, PAGES, applyTheme, ago, dateTime, toast, fmt } from "./core.js";
import { icon, loading, errorBox } from "./components.js";
import { createGlobalSearch, pageCommands } from "./palette.js";
import { t } from "./i18n.js";

const PAGE_MODULES = {
  panel: () => import("./page-dashboard.js"), codigo: () => import("./page-symbols.js"), duplicados: () => import("./page-findings.js"),
  "codigo-muerto": () => import("./page-findings.js"), revisiones: () => import("./page-findings.js"), busqueda: () => import("./page-search.js"),
  dependencias: () => import("./page-deps.js"), estadisticas: () => import("./page-stats.js"), obsidian: () => import("./page-notes.js"),
  configuracion: () => import("./page-settings.js"), espacio: () => import("./page-workspace.js"),
};
const THEMES = [["dark", "theme.dark", "moon"], ["light", "theme.light", "sun"], ["system", "theme.system", "gear"]];

const app = document.getElementById("app");
const ctx = { meta: null, summary: null, refresh: () => boot(true) };
let cleanup = null;
let shell, main, nav, status, bell, notices = [];

function indexingState(meta) {
  const map = { ready: ["ok", "status.ready"], stale: ["warn", "status.stale"], failed: ["bad", "status.failed"], unknown: ["info", "status.unknown"] };
  return map[meta.indexing.state] || map.unknown;
}

function computeNotices(meta, summary) {
  const list = [];
  if (meta.warning) list.push({ tone: "warn", text: meta.warning });
  if (meta.reviewsReadOnly) list.push({ tone: "bad", text: t("notice.reviewsReadOnly", { reason: meta.reviewsReadOnly }) });
  if (meta.indexing.staleFiles > 0) list.push({ tone: "warn", text: t("notice.stale", { n: meta.indexing.staleFiles }) });
  if (meta.indexing.readFailures > 0) list.push({ tone: "bad", text: t("notice.readFailures", { n: meta.indexing.readFailures }) });
  if (meta.indexing.syntaxErrors > 0) list.push({ tone: "info", text: t("notice.syntaxErrors", { n: meta.indexing.syntaxErrors }) });
  if (summary && !summary.duplicates.complete) list.push({ tone: "warn", text: t("notice.incomplete", { n: summary.duplicates.skipped }) });
  if (meta.vault.truncated) list.push({ tone: "info", text: t("notice.vaultTruncated") });
  return list;
}

function applyStatus() {
  const [tone, label] = indexingState(ctx.meta);
  clear(status).append(h("span", { class: `dot ${tone}` }), h("span", {}, t(label)),
    ctx.meta.indexing.lastIndexedAt && h("span", { class: "muted hide-lg", title: dateTime(ctx.meta.indexing.lastIndexedAt) }, `· ${ago(ctx.meta.indexing.lastIndexedAt)}`));
  status.title = t("status.tooltip");
  bell.querySelector(".count")?.remove();
  if (notices.length) bell.append(h("span", { class: "count", style: { position: "absolute", top: "4px", right: "4px", background: "var(--danger)", color: "#fff", borderRadius: "999px", fontSize: "10px", padding: "0 5px" } }, notices.length));
}

function setTheme(choice) { store.set("theme", choice); applyTheme(choice); }
const cycleTheme = () => { const order = ["dark", "light", "system"]; const next = order[(order.indexOf(store.get("theme", "dark")) + 1) % order.length]; setTheme(next); toast(t("theme.changed", { name: t(`theme.${next}`) })); };

function renderNav() {
  const { page } = parseHash();
  const counts = ctx.summary ? { duplicados: ctx.summary.duplicates.groups, "codigo-muerto": ctx.summary.deadCode.candidates, obsidian: ctx.summary.notes } : {};
  clear(nav).append(...PAGES.map((entry) => h("a", { href: hrefTo(entry.id), "aria-current": entry.id === page ? "page" : null, title: t(entry.label) },
    icon(entry.icon, 19), h("span", {}, t(entry.label)), counts[entry.id] !== undefined && h("span", { class: "count" }, fmt(counts[entry.id])))));
}

function notificationsMenu() {
  const existing = document.querySelector(".menu.notices");
  if (existing) { existing.remove(); return; }
  const menu = h("div", { class: "menu notices", role: "menu", style: { right: "60px", top: "56px", minWidth: "300px", padding: "10px" } },
    h("strong", {}, t("notice.title")),
    notices.length ? notices.map((n) => h("p", { class: `banner ${n.tone === "bad" ? "bad" : n.tone === "warn" ? "warn" : ""}`, style: { marginTop: "8px" } }, n.text)) : h("p", { class: "muted", style: { marginTop: "8px" } }, t("notice.none")));
  document.querySelector(".header").append(menu);
  const away = (event) => { if (!menu.contains(event.target) && !bell.contains(event.target)) { menu.remove(); document.removeEventListener("click", away); } };
  setTimeout(() => document.addEventListener("click", away), 0);
}

function buildShell() {
  const collapsed = store.get("sidebar-collapsed", false);
  nav = h("nav", { class: "nav", "aria-label": t("nav.label") });
  status = h("span", { class: "status-pill", role: "status" });
  bell = h("button", { class: "icon-btn", style: { position: "relative" }, "aria-label": t("notice.title"), onclick: notificationsMenu }, icon("bell"));
  const search = createGlobalSearch({ commands: () => [...pageCommands(), { title: t("palette.toggleTheme"), icon: "sun", run: cycleTheme }, { title: t("palette.toggleSidebar"), icon: "panel", run: toggleSidebar }, { title: t("palette.workspace"), icon: "layers", run: () => go("espacio") }] });
  ctx.search = search;
  const toggleSidebar = () => { shell.classList.toggle("collapsed"); store.set("sidebar-collapsed", shell.classList.contains("collapsed")); };
  const themeButton = h("button", { class: "icon-btn", "aria-label": t("theme.label"), title: t("theme.label"), onclick: cycleTheme }, icon("moon"));
  const refresh = h("button", { class: "icon-btn hide-sm", "aria-label": t("header.refresh"), title: t("header.refresh"), onclick: async () => { await boot(true); toast(t("header.refreshed"), "ok", 7000); } }, icon("refresh"));
  shell = h("div", { class: `shell ${collapsed ? "collapsed" : ""}` },
    h("aside", { class: "sidebar" },
      h("a", { class: "brand", href: hrefTo("panel") }, h("span", { class: "logo" }, icon("bolt", 17)), h("span", {}, "MCT")),
      nav,
      h("div", { class: "nav-foot" },
        h("a", { class: "btn ghost", href: hrefTo("espacio"), style: { justifyContent: "flex-start" } }, icon("layers", 18), h("span", { class: "label" }, t("header.workspace"))),
        h("button", { class: "btn ghost hide-md", style: { justifyContent: "flex-start" }, onclick: toggleSidebar, "aria-label": t("palette.toggleSidebar") }, icon("panel", 18), h("span", { class: "label" }, t("sidebar.collapse"))))),
    h("header", { class: "header" },
      h("div", { class: "header-left" },
        h("button", { class: "icon-btn menu-btn", "aria-label": t("nav.open"), onclick: () => shell.classList.add("open") }, icon("menu")),
        h("div", { class: "project hide-sm" }, h("span", { id: "project-name" }), h("small", { class: "hide-lg" }, t("header.repo"))),
        h("a", { class: "status-pill hide-lg", href: hrefTo("espacio"), title: t("header.workspaceTip") }, icon("layers", 14), t("header.workspaceCode"))),
      h("div", { class: "header-center" }, search.el),
      h("div", { class: "header-right" }, status, refresh, bell, themeButton,
        h("a", { class: "icon-btn hide-sm", href: hrefTo("configuracion"), "aria-label": t("nav.configuracion"), title: t("nav.configuracion") }, icon("gear")),
        h("span", { class: "status-pill hide-lg", title: t("header.localTip") }, icon("lock", 13), t("header.local")))),
    h("div", { class: "scrim", onclick: () => shell.classList.remove("open") }),
    (main = h("main", { class: "main", id: "contenido" })));
  clear(app).append(shell, h("div", { class: "toasts", "aria-live": "polite" }));
}

async function route() {
  const { page } = parseHash();
  cleanup?.(); cleanup = null;
  shell.classList.remove("open");
  renderNav();
  document.title = `MCT · ${t(PAGES.find((p) => p.id === page)?.label || "header.workspace")} · ${ctx.meta.project}`;
  clear(main).append(loading());
  try {
    const module = await PAGE_MODULES[page]();
    const root = h("div", { class: `page ${page === "obsidian" ? "obsidian" : ""}` });
    clear(main).append(root);
    cleanup = (await module.render({ ...ctx, root, page, params: parseHash().params })) || null;
    main.scrollTop = 0;
    root.querySelector("#page-title")?.focus({ preventScroll: true });
  } catch (error) {
    clear(main).append(h("div", { class: "page" }, errorBox(error, route)));
  }
}

let booted = false;
async function boot(refresh = false) {
  try {
    const [meta, summary] = await Promise.all([api("meta"), api("summary")]);
    ctx.meta = meta; ctx.summary = summary;
    setCsrf(meta.csrf);
    notices = computeNotices(meta, summary);
    if (!booted) { buildShell(); booted = true; }
    document.getElementById("project-name").textContent = meta.project;
    applyStatus(); renderNav();
    if (refresh) await route();
  } catch (error) {
    clear(app).append(h("div", { class: "page" }, errorBox(error, () => boot())));
    throw error;
  }
}

applyTheme(store.get("theme", "dark"));
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => store.get("theme", "dark") === "system" && applyTheme("system"));
await boot().then(route).catch(() => {});
addEventListener("hashchange", () => { if (booted) route(); });
