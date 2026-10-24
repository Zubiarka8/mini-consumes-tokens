// 01 · Resumen del proyecto
import { h, fmt, ago, shortDate, hrefTo, store } from "./core.js";
import { icon, pageHead, metricCard, selectBox, stateBadge, STATES, STATE_ORDER, stateLabel, emptyState, badge } from "./components.js";
import { lineChart, donut, hbars } from "./charts.js";
import { openIssues } from "./issues.js";
import { t } from "./i18n.js";

const DAY = 86400;
const LINE_SERIES = [["confirmed", "state.confirmed", "var(--danger)"], ["false-positive", "state.falsePositive", "var(--primary)"], ["resolved", "state.resolved", "var(--success)"], ["in-review", "state.inReview", "var(--info)"], ["dismissed", "state.dismissed", "var(--text-3)"]];

function activityChart(summary, range) {
  const events = summary.reviewActivity;
  if (!events.length) return emptyState({ iconName: "chart", title: t("dash.noHistoryTitle"), text: t("dash.noHistory") });
  const end = Math.floor(Date.now() / 1000 / DAY) * DAY;
  const days = Array.from({ length: range }, (_, i) => end - (range - 1 - i) * DAY);
  const byDay = new Map(events.map((e) => [e.day, e.counts]));
  const series = LINE_SERIES.map(([key, label, color]) => ({ name: t(label), color, values: days.map((day) => byDay.get(day)?.[key] || 0) }));
  if (series.every((s) => s.values.every((v) => v === 0))) return emptyState({ iconName: "chart", title: t("dash.noRecentTitle"), text: t("dash.noRecent", { n: range }) });
  return lineChart({ series, labels: days.map(shortDate), label: t("dash.activityTitle") });
}

function eventText(event) {
  const target = event.finding;
  const kind = target ? t(target.type === "duplicate" ? "common.duplicate" : "common.deadCode") : t("common.finding");
  return `${kind} · ${t(`dash.event.${event.to}`)}`;
}

export function render({ root, summary, meta }) {
  let range = store.get("dash-range", 30);
  const body = h("div", { class: "stack" });
  const draw = () => {
    const states = summary.findings.states;
    const pending = (states.unreviewed || 0) + (states["in-review"] || 0);
    const cards = [
      { icon: "code", tone: "primary", label: t("dash.symbols"), value: fmt(summary.symbols), href: hrefTo("codigo", "") },
      { icon: "file", tone: "accent", label: t("dash.files"), value: fmt(summary.files), href: hrefTo("codigo", "") },
      { icon: "copy", tone: "warning", label: t("dash.duplicates"), value: fmt(summary.duplicates.groups), hint: t("dash.groups"), href: hrefTo("duplicados", "") },
      { icon: "skull", tone: "danger", label: t("dash.dead"), value: fmt(summary.deadCode.candidates), hint: t("dash.deadHint"), href: hrefTo("codigo-muerto", "") },
      { icon: "check", tone: "info", label: t("dash.pending"), value: fmt(pending), href: hrefTo("revisiones", { state: "unreviewed,in-review" }) },
      { icon: "checkCircle", tone: "primary", label: t("dash.falsePositives"), value: fmt(states["false-positive"] || 0), href: hrefTo("revisiones", { state: "false-positive" }) },
      { icon: "note", tone: "success", label: t("dash.notes"), value: fmt(summary.notes), href: hrefTo("obsidian", "") },
      { icon: "clock", tone: "accent", label: t("dash.indexDuration"), value: "—", hint: t("dash.indexDurationHint"), href: hrefTo("estadisticas", "") },
    ];
    const segments = STATE_ORDER.map((state) => ({ name: stateLabel(state), value: states[state] || 0, color: STATES[state].color }));
    const lastIndexed = meta.indexing.lastIndexedAt;
    const recent = summary.recent;
    body.replaceChildren(
      h("div", { class: "grid metrics" }, cards.map(metricCard)),
      h("div", { class: "grid two" },
        h("section", { class: "card" }, h("h2", {}, t("dash.activityTitle")),
          h("p", { class: "muted small", style: { marginBottom: "8px" } }, t("dash.activityNote")), activityChart(summary, range)),
        h("section", { class: "card" }, h("h2", {}, t("dash.reviewState")), summary.findings.total ? donut({ segments, centerLabel: t("dash.findings"), label: t("dash.reviewState") }) : emptyState({ title: t("dash.noFindings"), iconName: "check" }))),
      h("div", { class: "grid three" },
        h("section", { class: "card" }, h("h2", {}, t("dash.languages")), summary.languages.length ? hbars({ rows: summary.languages.slice(0, 8).map((l) => ({ name: l.language, value: l.symbols })), label: t("dash.languages") }) : emptyState({ title: t("common.noData") })),
        h("section", { class: "card" }, h("h2", {}, t("dash.recent")),
          h("ul", { class: "timeline" },
            lastIndexed && h("li", {}, h("strong", {}, t("dash.scanDone")), h("div", { class: "muted small" }, ago(lastIndexed))),
            recent.map((event) => h("li", {}, h("div", {}, stateBadge(event.to, event.finding?.type), " ", event.finding ? h("a", { href: hrefTo(event.finding.type === "duplicate" ? "duplicados" : "codigo-muerto", { key: "" }) }, event.finding.title) : t("common.finding")),
              h("div", { class: "muted small" }, `${eventText(event)} · ${event.at ? ago(event.at) : t("review.unknownTime")}`))),
            !lastIndexed && !recent.length && h("li", { class: "muted" }, t("dash.noActivity")))),
        h("section", { class: "card" }, h("h2", {}, t("dash.health")),
          h("dl", { class: "kv" },
            h("dt", {}, t("dash.healthIndex")), h("dd", {}, badge(t(`status.${meta.indexing.state}`), meta.indexing.state === "ready" ? "success" : meta.indexing.state === "failed" ? "danger" : "warning")),
            h("dt", {}, t("dash.healthCoverage")), h("dd", {}, summary.duplicates.complete ? t("dash.coverageFull") : t("dash.coveragePartial", { n: summary.duplicates.skipped })),
            h("dt", {}, t("dash.healthPending")), h("dd", {}, h("a", { href: hrefTo("revisiones", { state: "unreviewed,in-review" }) }, fmt(pending))),
            h("dt", {}, t("dash.healthFailed")), h("dd", {}, summary.health.readFailures > 0 ? h("button", { class: "btn sm", onclick: () => openIssues("read", meta.indexing.readIssues || []) }, fmt(summary.health.readFailures)) : fmt(summary.health.readFailures)),
            h("dt", {}, t("dash.healthSyntax")), h("dd", {}, summary.health.syntaxErrors > 0 ? h("button", { class: "btn sm", onclick: () => openIssues("syntax", meta.indexing.syntaxIssues || []) }, fmt(summary.health.syntaxErrors)) : fmt(summary.health.syntaxErrors))),
          h("p", { class: "muted small", style: { marginTop: "12px" } }, t("dash.noScore")))));
  };
  root.append(pageHead(t("dash.title"), t("dash.subtitle"), [
    selectBox({ label: t("header.repo"), value: "repo", options: [["repo", meta.project]], onChange: () => {}, cls: "hide-sm" }),
    selectBox({ label: t("dash.range"), value: String(range), options: [["7", t("dash.last7")], ["30", t("dash.last30")], ["90", t("dash.last90")]], onChange: (v) => { range = Number(v); store.set("dash-range", range); draw(); } }),
  ]), body);
  draw();
}
