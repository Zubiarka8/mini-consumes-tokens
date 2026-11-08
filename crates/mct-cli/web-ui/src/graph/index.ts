// WebGL graph for large note vaults: Sigma draws, ForceAtlas2 lays out, Louvain colours the
// communities. Same contract as the SVG graph in app/graph.js (`el`, `select`, `fit`,
// `clearSelection`, `destroy`), so the page picks one by size. Everything is drawn on a canvas
// and styled through classes or CSSOM, which the dashboard's CSP allows.
import Graph from "graphology";
import louvain from "graphology-communities-louvain";
import forceAtlas2 from "graphology-layout-forceatlas2";
import Sigma from "sigma";

export interface GraphNode {
  id: string;
  label: string;
  degree: number;
}
export interface GraphLabels {
  aria: string;
  zoomIn: string;
  zoomOut: string;
  fit: string;
  display: string;
  forces: string;
  nodeSize: string;
  linkThickness: string;
  textThreshold: string;
  center: string;
  repel: string;
}
export interface GraphOptions {
  nodes: GraphNode[];
  edges: { from: string; to: string }[];
  labels: GraphLabels;
  onSelect?: (node: GraphNode) => void;
  doc?: Document;
}
export interface GraphHandle {
  el: HTMLElement;
  select: (id: string) => void;
  fit: () => void;
  clearSelection: () => void;
  destroy: () => void;
}

// Community hues: distinguishable in both themes and for common colour-vision types.
const PALETTE = [
  "#4477aa",
  "#ee6677",
  "#228833",
  "#ccbb44",
  "#66ccee",
  "#aa3377",
  "#ee8866",
  "#44bb99",
  "#9970ab",
  "#bbbbbb",
];
// The same hues, darker, so the pale ones stay visible on a white background.
const PALETTE_LIGHT = [
  "#2b5b94",
  "#c2364b",
  "#1d7a2c",
  "#8f7f10",
  "#1b86ab",
  "#8a2a63",
  "#cc5a28",
  "#237f62",
  "#76508f",
  "#6b7280",
];
const LAYOUT_CHUNK = 40;
const LAYOUT_ROUNDS = 20;

// Panel sliders, like Obsidian's graph view: display (applies at once) and forces (re-run the layout).
const DISPLAY = {
  nodeSize: { min: 0.4, max: 3, step: 0.1, value: 1 },
  linkThickness: { min: 0.5, max: 4, step: 0.1, value: 1 },
  textThreshold: { min: 0, max: 20, step: 1, value: 7 },
} as const;
const FORCES = {
  center: { min: 0.1, max: 3, step: 0.1, value: 0.6 },
  repel: { min: 2, max: 60, step: 1, value: 12 },
} as const;

/** Any CSS colour (`var(--x)` already resolved by the caller) as `[r, g, b]`, via a canvas. */
function rgb(
  doc: Document,
  value: string,
  fallback: string,
): [number, number, number] {
  const ctx = doc.createElement("canvas").getContext("2d");
  if (!ctx) return [128, 128, 128];
  ctx.fillStyle = fallback;
  ctx.fillStyle = value;
  const out = ctx.fillStyle;
  if (out.startsWith("#")) {
    const n = Number.parseInt(out.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const m = /rgba?\((\d+)[ ,]+(\d+)[ ,]+(\d+)/.exec(out);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : [128, 128, 128];
}

export function createGraph({
  nodes,
  edges,
  labels,
  onSelect,
  doc = document,
}: GraphOptions): GraphHandle {
  const graph = new Graph({ type: "undirected", multi: false });
  const rankOf = new Map<string, number>();
  const byId = new Map(nodes.map((node) => [node.id, node]));
  nodes.forEach((node, i) => {
    const angle = (i / nodes.length) * Math.PI * 2;
    const radius = 20 * Math.sqrt(i + 1);
    graph.addNode(node.id, {
      label:
        node.label.length > 30 ? `${node.label.slice(0, 29)}…` : node.label,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      size: 1.2 + Math.sqrt(node.degree) * 0.55,
      color: PALETTE[0],
    });
  });
  for (const edge of edges) {
    if (
      edge.from !== edge.to &&
      graph.hasNode(edge.from) &&
      graph.hasNode(edge.to) &&
      !graph.hasEdge(edge.from, edge.to)
    )
      graph.addEdge(edge.from, edge.to);
  }
  if (graph.size > 0) {
    // Largest communities get the first palette colours; the long tail shares the last, grey one.
    const community = louvain(graph);
    const sizes = new Map<number, number>();
    for (const id of Object.keys(community))
      sizes.set(community[id], (sizes.get(community[id]) ?? 0) + 1);
    const rank = new Map(
      [...sizes].sort((a, b) => b[1] - a[1]).map(([c], i) => [c, i]),
    );
    for (const id of Object.keys(community))
      rankOf.set(
        id,
        Math.min(rank.get(community[id]) ?? 0, PALETTE.length - 1),
      );
  }

  const el = doc.createElement("div");
  el.className = "graph graph-gl";
  const stage = doc.createElement("div");
  stage.className = "graph-stage";
  stage.setAttribute("role", "img");
  stage.setAttribute("aria-label", labels.aria);
  const tools = doc.createElement("div");
  tools.className = "tools";
  const button = (
    className: string,
    text: string,
    label: string,
    run: () => void,
  ) => {
    const b = doc.createElement("button");
    b.type = "button";
    b.className = className;
    b.textContent = text;
    b.setAttribute("aria-label", label);
    b.addEventListener("click", run);
    return b;
  };
  const look = { nodeSize: 1, linkThickness: 1 };
  const forces: { center: number; repel: number } = {
    center: FORCES.center.value,
    repel: FORCES.repel.value,
  };
  let sigma: Sigma | null = null;
  let selected: string | null = null;
  let hovered: string | null = null;
  let stopped = false;
  let laidOut = false;
  let timer = 0;
  const camera = () => sigma?.getCamera();
  const fit = () => void camera()?.animatedReset({ duration: 250 });
  tools.append(
    button(
      "icon-btn graph-control",
      "+",
      labels.zoomIn,
      () => void camera()?.animatedZoom({ duration: 150 }),
    ),
    button(
      "icon-btn graph-control",
      "−",
      labels.zoomOut,
      () => void camera()?.animatedUnzoom({ duration: 150 }),
    ),
    button("btn graph-control-fit", labels.fit, labels.fit, fit),
  );
  const slider = (
    text: string,
    range: { min: number; max: number; step: number; value: number },
    apply: (v: number) => void,
  ) => {
    const row = doc.createElement("label");
    row.className = "graph-slider";
    const name = doc.createElement("span");
    name.textContent = text;
    const input = doc.createElement("input");
    input.type = "range";
    input.min = String(range.min);
    input.max = String(range.max);
    input.step = String(range.step);
    input.value = String(range.value);
    input.addEventListener("input", () => apply(Number(input.value)));
    row.append(name, input);
    return row;
  };
  const group = (title: string, ...rows: HTMLElement[]) => {
    const details = doc.createElement("details");
    details.className = "graph-group";
    details.open = false;
    const summary = doc.createElement("summary");
    summary.textContent = title;
    details.append(summary, ...rows);
    return details;
  };
  const panel = doc.createElement("div");
  panel.className = "graph-panel";
  let relayout = 0;
  const reflow = () => {
    window.clearTimeout(timer);
    window.clearTimeout(relayout);
    relayout = window.setTimeout(() => layout(0), 120);
  };
  panel.append(
    group(
      labels.display,
      slider(labels.nodeSize, DISPLAY.nodeSize, (v) => {
        look.nodeSize = v;
        sigma?.refresh({ skipIndexation: true });
      }),
      slider(labels.linkThickness, DISPLAY.linkThickness, (v) => {
        look.linkThickness = v;
        sigma?.refresh({ skipIndexation: true });
      }),
      slider(labels.textThreshold, DISPLAY.textThreshold, (v) => {
        sigma?.setSetting("labelRenderedSizeThreshold", v);
      }),
    ),
    group(
      labels.forces,
      slider(labels.center, FORCES.center, (v) => {
        forces.center = v;
        reflow();
      }),
      slider(labels.repel, FORCES.repel, (v) => {
        forces.repel = v;
        reflow();
      }),
    ),
  );
  el.append(stage, panel, tools);

  const focus = () => hovered ?? selected;
  // Whether the dashboard is showing a light surface right now (theme switches while the graph is open).
  const isLight = () => {
    const [r, g, b] = rgb(
      doc,
      getComputedStyle(el).getPropertyValue("--surface").trim() || "#fff",
      "#fff",
    );
    return (r * 299 + g * 587 + b * 114) / 1000 > 140;
  };
  const paint = () => {
    const palette = isLight() ? PALETTE_LIGHT : PALETTE;
    graph.forEachNode((id) =>
      graph.setNodeAttribute(id, "color", palette[rankOf.get(id) ?? 0]),
    );
  };
  const start = () => {
    if (stopped || sigma) return;
    paint();
    const light = isLight();
    const style = getComputedStyle(el);
    const read = (name: string, fallback: string) =>
      style.getPropertyValue(name).trim() || fallback;
    const [er, eg, eb] = rgb(doc, read("--text-3", "#888"), "#888");
    const text = read("--text", "#222");
    const dim = `rgba(${er}, ${eg}, ${eb}, ${light ? 0.08 : 0.04})`;
    const tones = new Map<string, [number, number, number]>();
    const tone = (color: string) => {
      let cached = tones.get(color);
      if (!cached) tones.set(color, (cached = rgb(doc, color, "#888")));
      return cached;
    };
    const line = `rgba(${er}, ${eg}, ${eb}, ${light ? 0.6 : 0.28})`;
    const surface = read("--surface", "#fff");
    const border = read("--border", "#ccc");
    sigma = new Sigma(graph, stage, {
      labelColor: { color: text },
      labelFont: style.fontFamily || "sans-serif",
      labelSize: 11,
      minEdgeThickness: 0.5,
      labelDensity: 0.35,
      labelGridCellSize: 110,
      labelRenderedSizeThreshold: DISPLAY.textThreshold.value,
      defaultEdgeColor: line,
      // Sigma's own hover box is white; draw it in the dashboard's surface and text colours.
      defaultDrawNodeHover: (context, data) => {
        const size = 12;
        context.font = `${size}px ${style.fontFamily || "sans-serif"}`;
        const label = data.label ?? "";
        const width = context.measureText(label).width + 16;
        const x = data.x + data.size + 4;
        context.fillStyle = surface;
        context.strokeStyle = border;
        context.lineWidth = 1;
        context.beginPath();
        context.roundRect(x, data.y - size, width, size * 2, 5);
        context.fill();
        context.stroke();
        context.fillStyle = text;
        context.textBaseline = "middle";
        context.fillText(label, x + 8, data.y);
        context.beginPath();
        context.arc(data.x, data.y, data.size + 1.5, 0, Math.PI * 2);
        context.strokeStyle = text;
        context.stroke();
      },
      minCameraRatio: 0.03,
      maxCameraRatio: 4,
      zIndex: true,
      // The focused node and the ones it connects to light up; everything else fades back.
      nodeReducer: (id, data) => {
        const f = focus();
        const sized = { ...data, size: data.size * look.nodeSize };
        if (!f) return sized;
        if (id === f)
          return {
            ...sized,
            size: sized.size * 1.8 + 2,
            zIndex: 3,
            forceLabel: true,
            highlighted: true,
          };
        if (graph.areNeighbors(id, f))
          return {
            ...sized,
            size: sized.size * 1.5 + 1.5,
            zIndex: 2,
            highlighted: true,
            forceLabel: graph.degree(f) <= 40,
          };
        const [nr, ng, nb] = tone(data.color);
        return {
          ...sized,
          color: `rgba(${nr}, ${ng}, ${nb}, ${light ? 0.35 : 0.14})`,
          label: null,
          zIndex: 0,
        };
      },
      edgeReducer: (edge, data) => {
        const f = focus();
        const sized = { ...data, size: data.size * look.linkThickness };
        if (!f) return sized;
        if (!graph.extremities(edge).includes(f))
          return { ...sized, color: dim };
        // The focused node's own colour, bright and thick, so the links to it read at a glance.
        const [cr, cg, cb] = tone(graph.getNodeAttribute(f, "color"));
        return {
          ...sized,
          size: Math.max(sized.size * 2.4, 1.6),
          color: `rgba(${cr}, ${cg}, ${cb}, 0.95)`,
          zIndex: 1,
        };
      },
    });
    sigma.on("enterNode", ({ node }) => {
      hovered = node;
      stage.style.cursor = "pointer";
      sigma?.refresh({ skipIndexation: true });
    });
    sigma.on("leaveNode", () => {
      hovered = null;
      stage.style.cursor = "";
      sigma?.refresh({ skipIndexation: true });
    });
    sigma.on("clickNode", ({ node }) => choose(node));
    sigma.on("doubleClickStage", (event) => {
      event.preventSigmaDefault();
      clearSelection();
    });
    if (!laidOut) {
      laidOut = true;
      layout(0);
    } else fit();
  };
  // ForceAtlas2 in short slices, so the page stays responsive and the graph settles in view.
  function layout(round: number) {
    if (stopped || !sigma) return;
    const settings = {
      ...forceAtlas2.inferSettings(graph),
      barnesHutOptimize: graph.order > 300,
      slowDown: 2,
      gravity: forces.center,
      scalingRatio: forces.repel,
    };
    forceAtlas2.assign(graph, { iterations: LAYOUT_CHUNK, settings });
    sigma.refresh();
    if (round + 1 < LAYOUT_ROUNDS)
      timer = window.setTimeout(() => layout(round + 1), 0);
    else void sigma.getCamera().animatedReset({ duration: 300 });
  }
  function choose(id: string) {
    const node = byId.get(id);
    if (!node || !graph.hasNode(id) || !sigma) return;
    selected = id;
    sigma.refresh({ skipIndexation: true });
    const pos = sigma.getNodeDisplayData(id);
    if (pos)
      void sigma
        .getCamera()
        .animate(
          { x: pos.x, y: pos.y, ratio: Math.min(sigma.getCamera().ratio, 0.5) },
          { duration: 300 },
        );
    onSelect?.(node);
  }
  function clearSelection() {
    selected = null;
    sigma?.refresh({ skipIndexation: true });
  }

  // Sigma needs a sized container: start once the element is attached and laid out.
  const watcher = new ResizeObserver(() => {
    if (stage.clientWidth > 0 && stage.clientHeight > 0 && !sigma) start();
    else if (sigma) {
      sigma.resize();
      sigma.refresh();
    }
  });
  watcher.observe(stage);
  // A theme switch changes every colour Sigma was given: rebuild the renderer, keep the layout.
  const restyle = () => {
    if (!sigma) return;
    sigma.kill();
    sigma = null;
    stage.replaceChildren();
    start();
  };
  const themeWatcher = new MutationObserver(restyle);
  themeWatcher.observe(doc.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme", "class"],
  });
  const scheme = window.matchMedia("(prefers-color-scheme: dark)");
  scheme.addEventListener("change", restyle);

  return {
    el,
    select: choose,
    fit,
    clearSelection,
    destroy: () => {
      stopped = true;
      window.clearTimeout(timer);
      window.clearTimeout(relayout);
      watcher.disconnect();
      themeWatcher.disconnect();
      scheme.removeEventListener("change", restyle);
      sigma?.kill();
      sigma = null;
    },
  };
}
