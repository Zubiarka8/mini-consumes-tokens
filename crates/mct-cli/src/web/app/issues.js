// Dialog listing the files the indexer could not fully parse or read, and where it stopped in each.
import { h, openDialog, copy } from "./core.js";
import { icon, badge, dataTable } from "./components.js";
import { openSource } from "./code.js";
import { issuesText } from "./ai.js";
import { t } from "./i18n.js";

const extension = (name) => (name.includes(".") ? name.split(".").pop() : "");

/** `kind` is "syntax" (partially indexed) or "read" (could not be read); `rows` come from meta.indexing. */
export function openIssues(kind, rows) {
  const where = (r) => `${r.path}${r.line ? `:${r.line}` : ""}`;
  const columns = [
    { key: "file", label: t("issues.file"), render: (r) => h("div", {}, h("strong", {}, r.name), h("div", { class: "path mono muted small", title: r.path, style: { whiteSpace: "normal", wordBreak: "break-all" } }, r.path)) },
    { key: "line", label: t("issues.line"), num: true, render: (r) => r.line ?? "—" },
    { key: "language", label: t("code.col.language"), cls: "hide-sm", render: (r) => (r.language ? badge(r.language) : extension(r.name) ? badge(extension(r.name)) : "—") },
    { key: "inside", label: t("issues.inside"), cls: "hide-sm", render: (r) => (r.symbol ? h("span", {}, h("strong", {}, r.symbol.name), h("span", { class: "muted small" }, ` ${r.symbol.kind} · ${r.symbol.line}–${r.symbol.endLine ?? r.symbol.line}`)) : "—") },
    { key: "message", label: t("issues.message"), cls: "hide-sm", render: (r) => (r.message === "syntax error" ? t("issues.syntaxMessage") : r.message) },
    { key: "actions", label: t("col.actions"), render: (r) => h("span", { class: "row", style: { gap: "4px" } },
      h("button", { class: "btn sm", onclick: () => openSource({ path: r.path, line: r.line || 1, language: r.language || "" }) }, icon("code", 14), t("issues.open")),
      h("button", { class: "btn sm", "aria-label": t("common.copyPath"), onclick: () => copy(where(r), t("common.pathCopied")) }, icon("copy", 14))) },
  ];
  const title = t(kind === "syntax" ? "issues.syntaxTitle" : "issues.readTitle", { n: rows.length });
  return openDialog((close) => h("div", { class: "dlg" },
    h("div", { class: "row spread" }, h("h2", { style: { fontSize: "16px" } }, title),
      h("div", { class: "row" },
        h("button", { class: "btn sm", onclick: () => copy(issuesText(title, rows), t("issues.copied")) }, icon("copy", 14), t("issues.copyList")),
        h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => close(null) }, icon("x")))),
    h("p", { class: "muted small", style: { margin: "6px 0 10px" } }, t(kind === "syntax" ? "issues.syntaxNote" : "issues.readNote")),
    h("div", { style: { maxHeight: "60vh", overflow: "auto" } }, dataTable({ columns, rows, rowKey: where, caption: title }))), { wide: true });
}
