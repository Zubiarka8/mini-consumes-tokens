// 08 · Estadísticas y rendimiento: only what this server really measures
import { h, clear, api, fmt, bytes, duration, dateTime, toast } from "./core.js";
import { icon, pageHead, metricCard, note, loading, errorBox, emptyState } from "./components.js";
import { donut, hbars } from "./charts.js";
import { kindGroup } from "./components.js";
import { t } from "./i18n.js";

const PALETTE = ["var(--info)", "var(--primary)", "var(--success)", "var(--warning)", "var(--accent)", "var(--danger)", "var(--text-3)"];

function download(name, type, text) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const link = h("a", { href: url, download: name });
  document.body.append(link); link.click(); link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function exportRows(data) {
  const m = data.measurements;
  const rows = [
    ["files", data.files, "count"], ["symbols", data.symbols, "count"], ["snapshot_load", m.snapshotLoadMs, "ms"], ["duplicate_analysis", m.duplicateAnalysisMs, "ms"],
    ["dead_code_analysis", m.deadCodeAnalysisMs, "ms"], ["index_database_size", m.indexDatabaseBytes, "bytes"],
    ...m.apiLatency.flatMap((r) => [[`api_${r.route}_p50`, r.p50Us, "us"], [`api_${r.route}_p95`, r.p95Us, "us"], [`api_${r.route}_samples`, r.samples, "count"]]),
  ];
  return rows;
}

export async function render({ root }) {
  const host = h("div", { class: "stack" });
  root.append(pageHead(t("stats.title"), t("stats.subtitle"), [h("button", { class: "btn", onclick: () => draw() }, icon("refresh", 14), t("stats.remeasure"))]), host);

  async function draw() {
    clear(host).append(loading());
    let data;
    try { data = await api("stats"); } catch (error) { clear(host).append(errorBox(error, draw)); return; }
    const m = data.measurements;
    const search = m.apiLatency.find((r) => r.route === "search");
    const unavailable = (label, reason) => h("div", { class: "card metric", style: { cursor: "default", opacity: 0.85 } }, h("span", { class: "ico tone-info" }, icon("clock", 20)), h("span", {}, h("div", { class: "v" }, t("stats.notMeasured")), h("div", { class: "l" }, label), h("div", { class: "hint" }, reason)));
    const groups = new Map();
    for (const k of data.kinds) { const g = kindGroup(k.kind); groups.set(g, (groups.get(g) || 0) + k.count); }
    const order = ["function", "class", "variable", "import", "type", "module", "macro", "other"];
    const segments = order.filter((g) => groups.has(g)).map((g, i) => ({ name: t(`stats.group.${g}`), value: groups.get(g), color: PALETTE[i % PALETTE.length] }));
    clear(host).append(
      h("div", { class: "grid metrics" },
        metricCard({ icon: "file", tone: "accent", label: t("dash.files"), value: fmt(data.files) }),
        metricCard({ icon: "code", tone: "primary", label: t("dash.symbols"), value: fmt(data.symbols) }),
        metricCard({ icon: "clock", tone: "info", label: t("stats.load"), value: duration(m.snapshotLoadMs), hint: t("stats.loadHint") }),
        metricCard({ icon: "copy", tone: "warning", label: t("stats.dupTime"), value: duration(m.duplicateAnalysisMs) }),
        metricCard({ icon: "skull", tone: "danger", label: t("stats.deadTime"), value: duration(m.deadCodeAnalysisMs) }),
        metricCard({ icon: "layers", tone: "success", label: t("stats.dbSize"), value: bytes(m.indexDatabaseBytes) }),
        metricCard({ icon: "search", tone: "primary", label: t("stats.searchLatency"), value: search ? `${fmt(search.p50Us / 1000, { maximumFractionDigits: 1 })} ms` : "—", hint: search ? t("stats.samples", { n: search.samples }) : t("stats.noSamples") }),
        unavailable(t("stats.incremental"), t("stats.reason.noJobs")), unavailable(t("stats.tokens"), t("stats.reason.tokens")), unavailable(t("stats.mcpSize"), t("stats.reason.mcp")), unavailable(t("stats.memory"), t("stats.reason.memory"))),
      h("div", { class: "grid two" },
        h("section", { class: "card" }, h("h2", {}, t("stats.latencyTitle")), m.apiLatency.length ? [hbars({ rows: m.apiLatency.map((r) => ({ name: `${r.route} p50`, value: r.p50Us / 1000 })), format: (v) => `${fmt(v, { maximumFractionDigits: 2 })} ms`, label: t("stats.latencyTitle") }),
          hbars({ rows: m.apiLatency.map((r) => ({ name: `${r.route} p95`, value: r.p95Us / 1000, color: "var(--warning)" })), format: (v) => `${fmt(v, { maximumFractionDigits: 2 })} ms`, label: t("stats.latencyTitle") })] : emptyState({ title: t("stats.noSamples"), iconName: "chart" }),
          h("p", { class: "muted small" }, t("stats.latencyNote"))),
        h("section", { class: "card" }, h("h2", {}, t("stats.distribution")), donut({ segments, centerLabel: t("dash.symbols"), label: t("stats.distribution") }))),
      h("div", { class: "grid two" },
        h("section", { class: "card" }, h("h2", {}, t("dash.languages")), hbars({ rows: data.languages.map((l) => ({ name: l.language, value: l.symbols })), label: t("dash.languages") })),
        h("section", { class: "card" }, h("h2", {}, t("stats.export")), h("p", { class: "muted" }, t("stats.exportText")),
          h("div", { class: "row", style: { margin: "10px 0" } },
            h("button", { class: "btn", onclick: () => { download("mct-stats.csv", "text/csv", ["metric,value,unit", ...exportRows(data).map((r) => r.join(","))].join("\n")); toast(t("stats.exported"), "ok"); } }, icon("download", 14), "CSV"),
            h("button", { class: "btn", onclick: () => { download("mct-stats.json", "application/json", JSON.stringify({ measuredAt: new Date().toISOString(), files: data.files, symbols: data.symbols, measurements: m }, null, 2)); toast(t("stats.exported"), "ok"); } }, icon("download", 14), "JSON")),
          note(t("stats.noFilters"), "", "info"))),
      h("section", { class: "card" }, h("h2", {}, t("stats.methodTitle")),
        h("ul", { style: { margin: 0, paddingLeft: "20px", display: "grid", gap: "6px" } }, ["load", "dup", "dead", "db", "latency", "tokens"].map((k) => h("li", {}, t(`stats.method.${k}`)))),
        h("p", { class: "muted small", style: { marginTop: "10px" } }, t("stats.noClaims"))));
  }
  await draw();
}
