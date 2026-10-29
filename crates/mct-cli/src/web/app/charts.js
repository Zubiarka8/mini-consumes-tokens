// Dependency-free SVG charts: multi-series line, donut and horizontal bars.
import { h, fmt } from "./core.js";
import { t } from "./i18n.js";

const svg = (tag, attrs, ...kids) => h(`svg:${tag}`, attrs, ...kids);

export function lineChart({ series, labels, height = 220, label }) {
  const width = 640;
  const pad = { l: 36, r: 12, t: 12, b: 26 };
  const max = Math.max(1, ...series.flatMap((s) => s.values));
  const top = Math.ceil(max / 4) * 4 || 4;
  const x = (i) =>
    pad.l +
    (labels.length <= 1 ? (width - pad.l - pad.r) / 2 : (i * (width - pad.l - pad.r)) / (labels.length - 1));
  const y = (v) => pad.t + (height - pad.t - pad.b) * (1 - v / top);
  const grid = [0, 1, 2, 3, 4].map((k) => {
    const value = (top * k) / 4;
    return [
      svg("line", { class: "grid-line", x1: pad.l, x2: width - pad.r, y1: y(value), y2: y(value) }),
      svg("text", { x: pad.l - 6, y: y(value) + 4, "text-anchor": "end" }, fmt(Math.round(value))),
    ];
  });
  const step = Math.max(1, Math.ceil(labels.length / 7));
  const ticks = labels.map((text, i) =>
    i % step === 0 ? svg("text", { x: x(i), y: height - 6, "text-anchor": "middle" }, text) : null,
  );
  const lines = series.map((s) => [
    svg("polyline", {
      fill: "none",
      stroke: s.color,
      "stroke-width": 2,
      points: s.values.map((v, i) => `${x(i)},${y(v)}`).join(" "),
    }),
    ...s.values.map((v, i) =>
      svg(
        "circle",
        { cx: x(i), cy: y(v), r: 3, fill: s.color },
        svg("title", {}, `${s.name}, ${labels[i]}: ${v}`),
      ),
    ),
  ]);
  return h(
    "div",
    {},
    svg(
      "svg",
      { class: "chart", viewBox: `0 0 ${width} ${height}`, width: "100%", role: "img", "aria-label": label },
      grid,
      ticks,
      lines,
    ),
    h(
      "div",
      { class: "legend mt-2" },
      series.map((s) => h("span", {}, h("i", { style: { background: s.color } }), s.name)),
    ),
  );
}

export function donut({ segments, centerLabel, size = 170, label }) {
  const total = segments.reduce((sum, s) => sum + s.value, 0);
  const r = 62;
  const c = 2 * Math.PI * r;
  let offset = 0;
  const arcs = total
    ? segments
        .filter((s) => s.value > 0)
        .map((s) => {
          const length = (s.value / total) * c;
          const arc = svg(
            "circle",
            {
              cx: 80,
              cy: 80,
              r,
              fill: "none",
              stroke: s.color,
              "stroke-width": 22,
              "stroke-dasharray": `${length} ${c - length}`,
              "stroke-dashoffset": -offset,
              transform: "rotate(-90 80 80)",
            },
            svg("title", {}, `${s.name}: ${s.value}`),
          );
          offset += length;
          return arc;
        })
    : [svg("circle", { cx: 80, cy: 80, r, fill: "none", stroke: "var(--border)", "stroke-width": 22 })];
  const chart = svg(
    "svg",
    { viewBox: "0 0 160 160", width: size, height: size, role: "img", "aria-label": label },
    arcs,
    svg("text", { x: 80, y: 80, "text-anchor": "middle", class: "donut-total" }, fmt(total)),
    svg(
      "text",
      { x: 80, y: 98, "text-anchor": "middle", class: "donut-label" },
      centerLabel || t("charts.total"),
    ),
  );
  const legend = h(
    "ul",
    { class: "plain-list chart-legend" },
    segments.map((s) =>
      h(
        "li",
        { class: "row chart-legend-item" },
        h("span", {}, h("i", { class: "sparkdot", style: { background: s.color } }), s.name),
        h(
          "span",
          { class: "muted" },
          `${fmt(s.value)}${total ? ` (${Math.round((s.value / total) * 100)}%)` : ""}`,
        ),
      ),
    ),
  );
  return h("div", { class: "row chart-donut" }, chart, legend);
}

export function hbars({ rows, color = "var(--primary)", label, format = fmt }) {
  const max = Math.max(1, ...rows.map((r) => r.value));
  return h(
    "div",
    { role: "list", "aria-label": label },
    rows.map((row) =>
      h(
        "div",
        { class: "bar-row", role: "listitem" },
        h("span", { class: "ellipsis", title: row.name }, row.name),
        h(
          "div",
          { class: "bar", "aria-hidden": "true" },
          h("i", { style: { width: `${(row.value / max) * 100}%`, background: row.color || color } }),
        ),
        h("span", { class: "muted text-right" }, format(row.value)),
      ),
    ),
  );
}
