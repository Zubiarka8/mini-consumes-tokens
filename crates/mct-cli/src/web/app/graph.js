// Force-directed graph on SVG: zoom, pan, drag, focus, fit. Layout is bounded and progressive.
import { h } from "./core.js";
import { icon } from "./components.js";
import { t } from "./i18n.js";

const svg = (tag, attrs, ...kids) => h(`svg:${tag}`, attrs, ...kids);
const BASE_TICKS = 220;

/**
 * nodes: [{id,label,color,radius,...}], edges: [{from,to,color?}].
 * Returns {el, select(id), fit(), destroy()}. `onSelect(node)` fires on click/Enter.
 */
export function graphView({ nodes, edges, onSelect, directed = true, label }) {
  const narrow = window.innerWidth < 700, width = narrow ? 400 : 900, height = narrow ? 600 : 560;
  const byId = new Map(nodes.map((node, i) => [node.id, { ...node, x: width / 2 + Math.cos(i) * 180 * Math.sqrt((i + 1) / nodes.length), y: height / 2 + Math.sin(i) * 140 * Math.sqrt((i + 1) / nodes.length), vx: 0, vy: 0, r: node.radius || 9 }]));
  const links = edges.map((edge) => ({ ...edge, s: byId.get(edge.from), t: byId.get(edge.to) })).filter((edge) => edge.s && edge.t);
  const list = [...byId.values()];
  // Repulsion is O(n²) per tick, so big graphs get fewer ticks to stay responsive.
  const MAX_TICKS = list.length <= 600 ? BASE_TICKS : Math.max(30, Math.round(BASE_TICKS * (600 / list.length) ** 2));
  const PAIR_BUDGET = 1_000_000;
  const PAIR_QUANTUM = 1024;
  const FRAME_BUDGET_MS = 6;
  const view = { x: 0, y: 0, k: 1 };
  let selected = null, ticks = 0, raf = 0, dragging = null;
  let pairI = 0, pairJ = 1, tickStarted = false;
  const visibleLabels = list.length <= 60
    ? new Set(list.map((node) => node.id))
    : new Set([...list].sort((a, b) => b.r - a.r || String(a.id).localeCompare(String(b.id))).slice(0, 25).map((node) => node.id));

  const root = svg("g", {});
  const marker = svg("marker", { id: "arrow", viewBox: "0 0 10 10", refX: 16, refY: 5, markerWidth: 6, markerHeight: 6, orient: "auto-start-reverse" }, svg("path", { d: "M0 0L10 5L0 10z", fill: "var(--text-3)" }));
  const edgeEls = links.map((edge) => svg("line", { class: "gedge", "stroke-width": Math.min(4, 1 + Math.log2(1 + (edge.weight || 1))), "marker-end": directed ? "url(#arrow)" : null, style: edge.color ? `stroke:${edge.color}` : undefined }));
  const nodeEls = new Map(list.map((node) => {
    const el = svg("g", { class: "gnode", tabindex: 0, role: "button", "aria-label": node.label },
      svg("circle", { r: node.r, fill: node.color || "var(--primary)", stroke: "var(--bg)", "stroke-width": 2 }), svg("text", { x: node.r + 4, y: 4, class: visibleLabels.has(node.id) ? "" : "faint-label" }, node.label.length > 26 ? `${node.label.slice(0, 25)}…` : node.label), svg("title", {}, node.title || node.label));
    el.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); choose(node.id); } });
    el.addEventListener("pointerdown", (event) => { event.stopPropagation(); dragging = { node, moved: false, id: event.pointerId }; el.setPointerCapture(event.pointerId); });
    el.addEventListener("pointermove", (event) => {
      if (!dragging || dragging.node !== node) return;
      const p = toGraph(event); node.x = p.x; node.y = p.y; dragging.moved = true; paint();
    });
    el.addEventListener("pointerup", () => { if (dragging && !dragging.moved) choose(node.id); dragging = null; });
    return [node.id, el];
  }));
  root.append(svg("defs", {}, marker), ...edgeEls, ...nodeEls.values());
  const canvas = svg("svg", { viewBox: `0 0 ${width} ${height}`, role: "group", "aria-label": label || t("graph.label"), preserveAspectRatio: "xMidYMid meet" }, root);
  const el = h("div", { class: "graph" }, canvas,
    h("div", { class: "tools" },
      h("button", { class: "icon-btn", style: { background: "var(--surface)", border: "1px solid var(--border)" }, "aria-label": t("graph.zoomIn"), onclick: () => zoom(1.25) }, icon("plus")),
      h("button", { class: "icon-btn", style: { background: "var(--surface)", border: "1px solid var(--border)" }, "aria-label": t("graph.zoomOut"), onclick: () => zoom(0.8) }, h("span", { "aria-hidden": "true" }, "−")),
      h("button", { class: "btn", style: { background: "var(--surface)" }, onclick: () => fit() }, t("graph.fit"))));

  function toGraph(event) {
    const box = canvas.getBoundingClientRect();
    const scale = Math.min(box.width / width, box.height / height);
    const ox = (box.width - width * scale) / 2, oy = (box.height - height * scale) / 2;
    return { x: ((event.clientX - box.left - ox) / scale - view.x) / view.k, y: ((event.clientY - box.top - oy) / scale - view.y) / view.k };
  }
  function paint() {
    root.setAttribute("transform", `translate(${view.x} ${view.y}) scale(${view.k})`);
    links.forEach((edge, i) => { const e = edgeEls[i]; e.setAttribute("x1", edge.s.x); e.setAttribute("y1", edge.s.y); e.setAttribute("x2", edge.t.x); e.setAttribute("y2", edge.t.y); });
    list.forEach((node) => nodeEls.get(node.id).setAttribute("transform", `translate(${node.x} ${node.y})`));
  }
  function beginTick() {
    for (const node of list) { node.vx = 0; node.vy = 0; }
    pairI = 0; pairJ = 1; tickStarted = true;
  }
  function applyTickForces() {
    const cool = 1 - ticks / MAX_TICKS;
    for (const edge of links) {
      const dx = edge.t.x - edge.s.x, dy = edge.t.y - edge.s.y, d = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const force = (d - 130) * 0.04;
      edge.s.vx += (dx / d) * force; edge.s.vy += (dy / d) * force; edge.t.vx -= (dx / d) * force; edge.t.vy -= (dy / d) * force;
    }
    for (const node of list) {
      node.vx += (width / 2 - node.x) * 0.012; node.vy += (height / 2 - node.y) * 0.012;
      if (dragging?.node === node) continue;
      node.x += Math.max(-12, Math.min(12, node.vx)) * cool; node.y += Math.max(-12, Math.min(12, node.vy)) * cool;
    }
  }
  function run() {
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const frame = () => {
      const deadline = performance.now() + FRAME_BUDGET_MS;
      const tickLimit = reduce ? MAX_TICKS : list.length > 1500 ? 1 : 4;
      let completed = 0, pairs = 0;
      while (ticks < MAX_TICKS && completed < tickLimit && performance.now() < deadline) {
        if (!tickStarted) beginTick();
        while (pairI < list.length - 1 && pairs < PAIR_BUDGET && performance.now() < deadline) {
          const quantumEnd = Math.min(PAIR_BUDGET, pairs + PAIR_QUANTUM);
          while (pairI < list.length - 1 && pairs < quantumEnd) {
            const a = list[pairI], b = list[pairJ];
            let dx = a.x - b.x, dy = a.y - b.y, d2 = dx * dx + dy * dy || 0.01;
            const force = 9000 / d2, d = Math.sqrt(d2);
            dx /= d; dy /= d;
            a.vx += dx * force; a.vy += dy * force; b.vx -= dx * force; b.vy -= dy * force;
            pairs += 1;
            pairJ += 1;
            if (pairJ >= list.length) { pairI += 1; pairJ = pairI + 1; }
          }
        }
        if (pairI >= list.length - 1) {
          applyTickForces();
          ticks += 1; completed += 1; tickStarted = false;
        } else break;
      }
      paint();
      if (ticks < MAX_TICKS) raf = requestAnimationFrame(frame); else fit();
    };
    frame();
  }
  function zoom(factor) { view.k = Math.max(0.2, Math.min(5, view.k * factor)); paint(); }
  function fit() {
    if (!list.length) return;
    const xs = list.map((n) => n.x), ys = list.map((n) => n.y);
    const [minX, maxX, minY, maxY] = [Math.min(...xs) - 60, Math.max(...xs) + 120, Math.min(...ys) - 30, Math.max(...ys) + 30];
    view.k = Math.min(2, Math.min(width / (maxX - minX), height / (maxY - minY)));
    view.x = (width - (maxX + minX) * view.k) / 2; view.y = (height - (maxY + minY) * view.k) / 2;
    paint();
  }
  function choose(id) {
    selected = id;
    const neighbours = new Set([id]);
    for (const edge of links) { if (edge.s.id === id) neighbours.add(edge.t.id); if (edge.t.id === id) neighbours.add(edge.s.id); }
    nodeEls.forEach((node, key) => node.classList.toggle("dim", !neighbours.has(key)));
    links.forEach((edge, i) => edgeEls[i].classList.toggle("dim", edge.s.id !== id && edge.t.id !== id));
    onSelect?.(byId.get(id));
  }
  function clearSelection() { selected = null; nodeEls.forEach((n) => n.classList.remove("dim")); edgeEls.forEach((e) => e.classList.remove("dim")); }

  let pan = null;
  canvas.addEventListener("pointerdown", (event) => { pan = { x: event.clientX, y: event.clientY, vx: view.x, vy: view.y }; canvas.classList.add("grabbing"); canvas.setPointerCapture(event.pointerId); });
  canvas.addEventListener("pointermove", (event) => {
    if (!pan || dragging) return;
    const box = canvas.getBoundingClientRect(), scale = Math.min(box.width / width, box.height / height);
    view.x = pan.vx + (event.clientX - pan.x) / scale; view.y = pan.vy + (event.clientY - pan.y) / scale; paint();
  });
  canvas.addEventListener("pointerup", () => { if (pan && selected && !dragging) { /* keep focus */ } pan = null; canvas.classList.remove("grabbing"); });
  canvas.addEventListener("dblclick", clearSelection);
  canvas.addEventListener("wheel", (event) => { event.preventDefault(); zoom(event.deltaY < 0 ? 1.1 : 0.9); }, { passive: false });
  run();
  return { el, select: choose, fit, destroy: () => cancelAnimationFrame(raf), clearSelection };
}
