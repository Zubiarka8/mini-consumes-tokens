import { harness } from './harness.mjs';

// 5,000-node reduced-motion baselines can monopolize the test process for minutes.
// That pathological first render is itself recorded separately in the audit report.
const sizes = [100, 600, 1347];
const samples = Number(process.env.MCT_GRAPH_SAMPLES || 3);
const results = [];

for (const reduced of [false, true]) {
  for (const size of sizes) {
    for (let sample = 0; sample < samples; sample += 1) {
      const env = harness({ reduced });
      const { graphView } = await env.load('graph.js');
      const nodes = Array.from({ length: size }, (_, id) => ({ id: String(id), label: `node-${id}`, radius: 8 }));
      const edges = Array.from({ length: Math.max(0, size - 1) }, (_, id) => ({ from: String(id), to: String(id + 1) }));
      const beforeHeap = process.memoryUsage().heapUsed;
      const start = performance.now();
      const graph = graphView({ nodes, edges });
      const initialFrameMs = performance.now() - start;
      let maxFrameMs = initialFrameMs, frames = 0;
      const complete = performance.now();
      while (env.frames.size && frames < 15000) {
        const [id, frame] = env.frames.entries().next().value;
        env.frames.delete(id);
        const frameStart = performance.now();
        frame();
        maxFrameMs = Math.max(maxFrameMs, performance.now() - frameStart);
        frames += 1;
      }
      const totalMs = performance.now() - complete + initialFrameMs;
      const heapDelta = process.memoryUsage().heapUsed - beforeHeap;
      graph.destroy();
      results.push({ reducedMotion: reduced, nodes: size, sample, initialFrameMs: +initialFrameMs.toFixed(2), maxFrameMs: +maxFrameMs.toFixed(2), totalMs: +totalMs.toFixed(2), frames, completed: env.frames.size === 0, heapDeltaBytes: heapDelta });
      env.root.replaceChildren();
    }
  }
}

process.stdout.write(`${JSON.stringify({ node: process.version, platform: process.platform, arch: process.arch, samples, measurements: results }, null, 2)}\n`);
