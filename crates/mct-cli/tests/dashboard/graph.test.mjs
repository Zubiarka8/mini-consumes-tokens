import test from 'node:test';
import assert from 'node:assert/strict';
import { harness } from './harness.mjs';

const chain = size => ({
  nodes: Array.from({ length: size }, (_, id) => ({ id: String(id), label: `node-${id}`, radius: 8 })),
  edges: Array.from({ length: Math.max(0, size - 1) }, (_, id) => ({ from: String(id), to: String(id + 1) })),
});

// Runs queued animation frames until the layout stops asking for more.
function settleFrames(env, onFrame = () => {}) {
  let frames = 0;
  while (env.frames.size && frames < 5000) {
    const [id, frame] = env.frames.entries().next().value;
    env.frames.delete(id);
    frame();
    frames += 1;
    onFrame();
  }
  return frames;
}

const positions = env => [...env.root.querySelectorAll('g.gnode')].map(node => node.getAttribute('transform'));

test('a 5,000-node graph settles in bounded frames and repaints only at the start and end', async () => {
  const env = harness();
  const { graphView } = await env.load('graph.js');
  const graph = graphView(chain(5000));
  env.root.append(graph.el);
  const first = () => env.root.querySelector('g.gnode').getAttribute('transform');
  const seen = new Set([first()]);
  // O(n²) repulsion took 3,154 frames here; Barnes–Hut needs about 130 on a laptop.
  const frames = settleFrames(env, () => seen.add(first()));
  assert.equal(env.frames.size, 0);
  assert.ok(frames <= 400, `${frames} frames`);
  // Repainting 5,000 SVG nodes takes about 100 ms in a browser: no animation at this size.
  assert.ok(seen.size <= 2, `${seen.size} distinct positions shown`);
  graph.destroy();
});

test('reduced motion shows only the starting and the finished layout', async () => {
  const env = harness({ reduced: true });
  const { graphView } = await env.load('graph.js');
  const graph = graphView(chain(1000));
  env.root.append(graph.el);
  const first = () => env.root.querySelector('g.gnode').getAttribute('transform');
  const seen = new Set([first()]);
  settleFrames(env, () => seen.add(first()));
  assert.equal(env.frames.size, 0);
  assert.ok(seen.size <= 2, `${seen.size} distinct positions shown`);
  graph.destroy();
});

test('the same graph always gets the same layout', async () => {
  const layout = async () => {
    const env = harness();
    const { graphView } = await env.load('graph.js');
    const graph = graphView(chain(900));
    env.root.append(graph.el);
    settleFrames(env);
    graph.destroy();
    return positions(env);
  };
  assert.deepEqual(await layout(), await layout());
});

test('Barnes–Hut repulsion stays close to the exact pairwise sum', async () => {
  const env = harness();
  const { quadtree, repel } = await env.load('graph.js');
  // A deterministic scatter with clusters, as a layout in progress looks.
  const nodes = Array.from({ length: 400 }, (_, i) => ({
    x: 450 + Math.cos(i * 2.4) * (40 + (i % 7) * 30), y: 280 + Math.sin(i * 1.7) * (30 + (i % 5) * 35), vx: 0, vy: 0,
  }));
  const tree = quadtree(nodes);
  let error = 0;
  let magnitude = 0;
  for (const node of nodes) {
    let ex = 0;
    let ey = 0;
    for (const other of nodes) {
      if (other === node) continue;
      const dx = node.x - other.x;
      const dy = node.y - other.y;
      const d2 = dx * dx + dy * dy || 0.01;
      const d = Math.sqrt(d2);
      ex += (dx / d) * (9000 / d2);
      ey += (dy / d) * (9000 / d2);
    }
    node.vx = 0;
    node.vy = 0;
    repel(node, tree);
    error += Math.hypot(node.vx - ex, node.vy - ey);
    magnitude += Math.hypot(ex, ey);
  }
  assert.ok(error / magnitude < 0.05, `relative error ${error / magnitude}`);
});

test('coincident nodes share a leaf instead of splitting the tree without end', async () => {
  const env = harness();
  const { quadtree, repel } = await env.load('graph.js');
  const nodes = Array.from({ length: 50 }, () => ({ x: 10, y: 10, vx: 0, vy: 0 }));
  nodes.push({ x: 110, y: 10, vx: 0, vy: 0 });
  const tree = quadtree(nodes);
  for (const node of nodes) repel(node, tree);
  assert.ok(nodes.every(node => Number.isFinite(node.vx) && Number.isFinite(node.vy)));
  // The lone node is pushed away from the 50 stacked ones.
  assert.ok(nodes[50].vx > 0);
});
