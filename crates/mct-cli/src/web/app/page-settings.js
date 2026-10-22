// 10 · Configuración: theme, local server facts, backend capability matrix, local data
import { h, clear, store, applyTheme, toast, confirmDialog, dateTime, forget } from "./core.js";
import { icon, pageHead, badge, note } from "./components.js";
import { t } from "./i18n.js";

const CAPABILITIES = [
  ["symbols", () => true], ["duplicates", (c) => c.duplicates], ["deadCode", (c) => c.deadCode], ["reviews", (c) => c.reviews], ["history", (c) => c.history],
  ["importGraph", (c) => c.dependencies.imports], ["callGraph", (c) => c.dependencies.calls], ["cycles", (c) => c.dependencies.cycles], ["keyword", (c) => c.search.keyword],
  ["literals", (c) => c.search.literals], ["semantic", (c) => c.search.semantic], ["obsidianRead", (c) => c.obsidian.read], ["obsidianEdit", (c) => c.obsidian.edit],
  ["obsidianOpen", (c) => c.obsidian.openInObsidian], ["reindex", (c) => c.reindex], ["tokens", (c) => c.tokens], ["similarity", (c) => c.similarityScore],
  ["confidence", (c) => c.confidence], ["severity", (c) => c.severity], ["reviewers", (c) => c.reviewers],
];

export function render({ root, meta }) {
  const theme = store.get("theme", "dark");
  const caps = h("tbody", {}, CAPABILITIES.map(([id, test]) => h("tr", {}, h("td", {}, t(`cap.${id}`)), h("td", {}, test(meta.capabilities) ? badge(t("cap.yes"), "success") : badge(t("cap.no"), "")), h("td", { class: "muted" }, t(`cap.${id}.note`)))));
  const presets = store.get("presets", []);
  root.append(pageHead(t("settings.title"), t("settings.subtitle")),
    h("div", { class: "grid two" },
      h("section", { class: "card" }, h("h2", {}, t("settings.appearance")),
        h("div", { role: "radiogroup", "aria-label": t("theme.label"), class: "stack" }, [["dark", "theme.dark"], ["light", "theme.light"], ["system", "theme.system"]].map(([id, label]) =>
          h("label", { class: "opt" }, h("input", { type: "radio", name: "theme", checked: theme === id, onchange: () => { store.set("theme", id); applyTheme(id); } }), t(label)))),
        h("label", { class: "field", style: { marginTop: "12px" } }, t("settings.language"), h("select", { class: "select", disabled: true }, h("option", {}, "Español (España)"))),
        h("p", { class: "muted small" }, t("settings.languageNote"))),
      h("section", { class: "card" }, h("h2", {}, t("settings.server")),
        h("dl", { class: "kv" }, h("dt", {}, t("header.repo")), h("dd", {}, meta.project), h("dt", {}, t("settings.snapshot")), h("dd", {}, dateTime(meta.generated)),
          h("dt", {}, t("settings.lastIndexed")), h("dd", {}, dateTime(meta.indexing.lastIndexedAt)), h("dt", {}, t("settings.binding")), h("dd", {}, location.host),
          h("dt", {}, t("settings.reviewsFile")), h("dd", { class: "mono" }, ".mct-index/duplicate-reviews.toon"), h("dt", {}, t("settings.vault")), h("dd", {}, t("settings.vaultNotes", { n: meta.vault.notes }))),
        note(t("settings.readOnly"), "", "lock"))),
    h("section", { class: "card", style: { marginTop: "14px" } }, h("h2", {}, t("settings.capabilities")), h("p", { class: "muted", style: { marginBottom: "10px" } }, t("settings.capabilitiesNote")),
      h("div", { class: "table-wrap" }, h("table", { class: "t" }, h("thead", {}, h("tr", {}, h("th", {}, t("settings.capability")), h("th", {}, t("settings.status")), h("th", {}, t("settings.detail")))), caps))),
    h("div", { class: "grid two", style: { marginTop: "14px" } },
      h("section", { class: "card" }, h("h2", {}, t("settings.localData")), h("p", { class: "muted" }, t("settings.localDataText", { n: presets.length })),
        h("div", { class: "row", style: { marginTop: "10px" } },
          h("button", { class: "btn", onclick: async () => { if (await confirmDialog({ title: t("settings.resetTitle"), body: t("settings.resetBody"), confirm: t("settings.reset"), danger: true })) { for (const k of ["filters", "presets", "recent-searches", "saved-searches", "recent-items"]) store.set(k, k === "filters" ? {} : []); toast(t("settings.resetDone"), "ok"); } } }, icon("trash", 14), t("settings.reset")))),
      h("section", { class: "card" }, h("h2", {}, t("settings.shortcuts")),
        h("dl", { class: "kv" }, [["Ctrl/⌘ K", "settings.sc.search"], ["/", "settings.sc.slash"], ["↑ ↓ Enter", "settings.sc.list"], ["Esc", "settings.sc.esc"]].map(([k, d]) => [h("dt", {}, h("span", { class: "kbd" }, k)), h("dd", {}, t(d))])))));
}
