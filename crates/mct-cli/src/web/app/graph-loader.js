// Loads the WebGL graph on first use. The bundle is built from crates/mct-cli/web-ui/src/graph
// (Sigma + ForceAtlas2, see ADR-033) and committed in src/web/graph-dist/ (graph.js). It is served as
// `graph-gl.js` so it does not collide with the SVG graph module `graph.js`.
let pending;

/** Resolves to `createGraph(options)`, or `null` when the bundle cannot load (use the SVG graph). */
export async function glGraph() {
  pending ||= import("./graph-gl.js");
  try {
    return (await pending).createGraph;
  } catch {
    pending = undefined;
    return null;
  }
}
