// Dumps a running `mct-cli web` dashboard into website/demo/snapshot.json, the
// frozen data behind the public demo. Usage:
//   node scripts/export-demo-snapshot.mjs <dashboard-url> <project-root>
// The URL is the private one `mct-cli web` prints. Nothing is written outside
// website/demo/, and only files the findings or notes point at are embedded.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [, , dashboardUrl, projectRoot] = process.argv;
if (!dashboardUrl || !projectRoot) {
  console.error(
    "usage: export-demo-snapshot.mjs <dashboard-url> <project-root>",
  );
  process.exit(2);
}
const base = dashboardUrl.endsWith("/") ? dashboardUrl : `${dashboardUrl}/`;
const out = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "demo",
  "snapshot.json",
);
const MAX_FILE_BYTES = 200_000;

async function get(path, params = {}) {
  const query = new URLSearchParams(params).toString();
  const response = await fetch(`${base}api/${path}${query ? `?${query}` : ""}`);
  if (!response.ok) throw new Error(`${path} ${query}: ${response.status}`);
  return response.json();
}

async function pages(path, params = {}, size = 200) {
  const items = [];
  for (let page = 0; ; page++) {
    const data = await get(path, { ...params, size, page });
    items.push(...data.items);
    if (items.length >= data.total || data.items.length === 0) return items;
  }
}

const meta = await get("meta");
const summary = await get("summary");
const findings = await pages("findings");
const notes = await pages("notes");
const graph = await get("notes-graph", { all: 1 });

const findingDetails = {};
for (const row of findings)
  findingDetails[row.key] = await get("finding", { key: row.key });
const noteDetails = {};
for (const row of notes)
  noteDetails[row.path] = await get("note", { path: row.path });

// The vault has thousands of notes; the demo keeps the best-connected ones and
// those of files that have findings, so the page stays a few megabytes.
const MAX_NOTES = 350;
const findingPaths = new Set();
for (const detail of Object.values(findingDetails)) {
  findingPaths.add(detail.path);
  for (const member of detail.members ?? []) findingPaths.add(member.path);
}
const degree = new Map(graph.nodes.map((node) => [node.id, node.degree]));
const ranked = [...notes].sort(
  (a, b) => (degree.get(b.path) ?? 0) - (degree.get(a.path) ?? 0),
);
const keep = new Set(ranked.slice(0, MAX_NOTES).map((note) => note.path));
for (const note of notes)
  if (note.codePath && findingPaths.has(note.codePath)) keep.add(note.path);
const keptNotes = notes.filter((note) => keep.has(note.path));
const keptDetails = {};
for (const note of keptNotes) {
  const detail = noteDetails[note.path];
  detail.linksTo = detail.linksTo.filter((link) => keep.has(link.path));
  detail.backlinkList = detail.backlinkList.filter((link) =>
    keep.has(link.path),
  );
  keptDetails[note.path] = detail;
}
const keptGraph = {
  ...graph,
  nodes: graph.nodes.filter((node) => keep.has(node.id)),
  edges: graph.edges.filter((edge) => keep.has(edge.from) && keep.has(edge.to)),
};
const sources = {};
for (const path of findingPaths) {
  try {
    const text = readFileSync(join(projectRoot, path), "utf8");
    if (text.length <= MAX_FILE_BYTES) sources[path] = text;
  } catch {
    /* a file that is gone simply has no source view */
  }
}

// Real review decisions and notes belong to the project, not to a public demo:
// every finding starts unreviewed, and the demo keeps new decisions in memory.
const clean = (row) =>
  Object.assign(row, {
    state: "unreviewed",
    previousState: "unreviewed",
    note: "",
    updated: null,
    source: null,
    revision: "none",
    readOnly: false,
    history: row.history ? [] : undefined,
  });
findings.forEach(clean);
Object.values(findingDetails).forEach(clean);
meta.csrf = "demo";
meta.project = "mini-consumes-tokens";
meta.reviewsReadOnly = null;
meta.indexing.readIssues = [];
meta.indexing.syntaxIssues = [];
meta.indexing.syntaxErrors = 0;
meta.indexing.readFailures = 0;
meta.indexing.staleFiles = 0;
meta.indexing.state = "ready";

const text = JSON.stringify({
  exportedAt: Math.floor(Date.now() / 1000),
  meta,
  summary,
  findings,
  findingDetails,
  notes: keptNotes,
  noteDetails: keptDetails,
  graph: keptGraph,
  sources,
});
if (text.includes(projectRoot)) {
  console.error(
    `refusing to write: the snapshot contains the absolute project path ${projectRoot}`,
  );
  process.exit(1);
}
mkdirSync(dirname(out), { recursive: true });
const snapshot = text;
writeFileSync(out, snapshot);
console.log(
  `${findings.length} findings, ${keptNotes.length} notes, ${Object.keys(sources).length} sources -> ${out}`,
);
