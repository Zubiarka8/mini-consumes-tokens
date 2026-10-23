// In-browser stand-in for the `mct-cli web` JSON API, backed by a frozen snapshot.
// It answers the same `api/...` URLs the dashboard already calls, with the same
// filter, sort and page rules as crates/mct-cli/src/web/api/{findings,notes}.rs.
// Review decisions live in memory only; reloading the page starts over.

const DAY = 86_400;
const FILTER_FIELDS = ["type", "state", "language", "kind", "verdict", "scope", "confidence", "zone"];
const UNAVAILABLE = { status: 404, error: "Not part of the demo" };

const csv = (params, key) => (params.get(key) || "").split(",").filter(Boolean);
const text = (params, key) => (params.get(key) || "").trim();
const uint = (params, key, fallback) => {
  const value = Number.parseInt(params.get(key) ?? "", 10);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
};
const has = (haystack, needle) => !needle || String(haystack).toLowerCase().includes(needle.toLowerCase());

function facet(values) {
  const counts = new Map();
  for (const value of values) counts.set(value, (counts.get(value) || 0) + 1);
  return [...counts].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1));
}

function bounds(params, total, fallback) {
  const size = Math.min(Math.max(uint(params, "size", fallback), 1), 200);
  const last = Math.floor(Math.max(total - 1, 0) / size);
  const page = Math.min(uint(params, "page", 0), last);
  return { page, size, start: page * size };
}

function sortKey(row, field) {
  const value = row[field];
  if (typeof value === "number") return [value, ""];
  if (typeof value === "string") return [0, value.toLowerCase()];
  return [0, ""];
}
const compare = (a, b) => (a[0] - b[0]) || (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0);

export function createBackend(snapshot) {
  const decisions = new Map();
  const events = [];
  const rows = snapshot.findings;
  const details = snapshot.findingDetails;
  let revision = 0;

  const withDecision = (row) => {
    const decision = decisions.get(row.key);
    if (!decision) return row;
    return { ...row, state: decision.state, previousState: decision.state, note: decision.note, updated: decision.at, source: "demo", revision: decision.revision };
  };
  const currentRows = () => rows.map(withDecision);

  function summary() {
    const states = {};
    const byType = {};
    for (const row of currentRows()) {
      states[row.state] = (states[row.state] || 0) + 1;
      (byType[row.type] ||= {})[row.state] = (byType[row.type][row.state] || 0) + 1;
    }
    const recent = events.slice(-8).reverse().map((event) => ({ at: event.at, to: event.to, note: event.note, finding: event.finding }));
    const perDay = new Map();
    for (const event of events) {
      const day = Math.floor(event.at / DAY) * DAY;
      const counts = perDay.get(day) || {};
      const to = event.to === "unreviewed" ? "reopened" : event.to;
      counts[to] = (counts[to] || 0) + 1;
      perDay.set(day, counts);
    }
    return {
      ...snapshot.summary,
      findings: { ...snapshot.summary.findings, states, byType },
      reviewActivity: [...perDay].map(([day, counts]) => ({ day, counts })),
      recent,
      health: { ...snapshot.summary.health, pendingReviews: (states.unreviewed || 0) + (states["in-review"] || 0) },
    };
  }

  function findings(params) {
    const wanted = text(params, "type");
    let list = currentRows().filter((row) => (wanted === "dead-code" ? row.type !== "duplicate" : wanted === "duplicate" ? row.type !== "dead-code" : true));
    const query = text(params, "q");
    const dir = text(params, "dir").replace(/\/+$/, "");
    const inDir = (path) => !dir || path === dir || path.startsWith(`${dir}/`);
    list = list.filter((row) => {
      if (row.type === "duplicate") {
        const members = details[row.key].members;
        return members.some((member) => inDir(member.path)) && (!query || has(`${row.title} ${row.path}`, query) || members.some((m) => has(m.name, query) || has(m.path, query)));
      }
      return inDir(row.path) && has(`${row.title} ${row.path}`, query);
    });
    const facets = Object.fromEntries(FILTER_FIELDS.map((field) => [field, facet(list.map((row) => row[field]).filter((v) => typeof v === "string"))]));
    for (const field of FILTER_FIELDS) {
      const allowed = csv(params, field);
      if (allowed.length) list = list.filter((row) => allowed.includes(row[field] ?? ""));
    }
    const sort = text(params, "sort") || "rank";
    list.sort((a, b) => compare(sortKey(a, sort), sortKey(b, sort)) || compare(sortKey(a, "rank"), sortKey(b, "rank")));
    if (text(params, "order") === "desc") list.reverse();
    const { page, size, start } = bounds(params, list.length, 25);
    return { total: list.length, page, size, items: list.slice(start, start + size), facets };
  }

  function finding(params) {
    const detail = details[text(params, "key")];
    if (!detail) return UNAVAILABLE;
    const row = withDecision(detail);
    const key = row.key;
    return { ...row, history: events.filter((event) => event.finding.key === key).map((event) => ({ from: event.from, to: event.to, at: event.at, note: event.note, source: "demo" })) };
  }

  function review(body) {
    const row = rows.find((candidate) => candidate.key === body.key);
    if (!row) return { status: 404, error: "unknown finding" };
    const from = decisions.get(row.key)?.state || "unreviewed";
    const state = body.state || "unreviewed";
    revision += 1;
    const at = Math.floor(Date.now() / 1000);
    if (body.state) decisions.set(row.key, { state, note: body.note || "", at, revision: `demo-${revision}` });
    else decisions.delete(row.key);
    events.push({ at, from, to: state, note: body.note || "", finding: { key: row.key, type: row.type, title: row.title, path: row.path } });
    return { ...finding(new URLSearchParams({ key: row.key })), readOnly: false };
  }

  const noteByPath = new Map(snapshot.notes.map((note) => [note.path, note]));
  function notes(params) {
    const query = text(params, "q");
    const folders = csv(params, "folder");
    const tags = csv(params, "tag");
    const relations = csv(params, "rel");
    const days = uint(params, "days", 0);
    const now = snapshot.meta.generated;
    const base = snapshot.notes.filter((note) => has(note.title, query) || has(note.path, query) || note.tags.some((tag) => has(tag, query)));
    const facets = { folder: facet(base.map((note) => note.folder)), tag: facet(base.flatMap((note) => note.tags)) };
    const duplicatePaths = new Set(rows.filter((row) => row.type === "duplicate").flatMap((row) => details[row.key].members.map((m) => m.path)));
    const deadPaths = new Set(rows.filter((row) => row.type === "dead-code").map((row) => row.path));
    let list = base.filter((note) => (!folders.length || folders.includes(note.folder))
      && (!tags.length || note.tags.some((tag) => tags.includes(tag)))
      && (days === 0 || note.modified + days * DAY >= now)
      && relations.every((relation) => ({
        "with-code": !!note.codePath, "no-code": !note.codePath, "with-backlinks": note.backlinks > 0, "no-backlinks": note.backlinks === 0,
        duplicates: !!note.codePath && duplicatePaths.has(note.codePath), "dead-code": !!note.codePath && deadPaths.has(note.codePath),
      })[relation] ?? true));
    const sort = text(params, "sort");
    const order = { modified: (n) => n.modified, backlinks: (n) => n.backlinks, symbols: (n) => n.symbolCount ?? 0 }[sort];
    list.sort((a, b) => (order ? order(a) - order(b) : a.title.toLowerCase() < b.title.toLowerCase() ? -1 : a.title.toLowerCase() > b.title.toLowerCase() ? 1 : 0) || (a.path < b.path ? -1 : 1));
    if (text(params, "order") === "desc") list.reverse();
    const { page, size, start } = bounds(params, list.length, 50);
    return { total: list.length, page, size, items: list.slice(start, start + size), facets, all: snapshot.notes.length, truncated: snapshot.meta.vault.truncated };
  }

  function graph(params) {
    const folders = csv(params, "folder");
    const tags = csv(params, "tag");
    const focus = text(params, "focus");
    const hops = Math.min(Math.max(uint(params, "depth", 2), 1), 6);
    const orphans = text(params, "orphans");
    const full = snapshot.graph;
    const byId = new Map(full.nodes.map((node) => [node.id, node]));
    const eligible = (node) => node && (!folders.length || folders.includes(node.folder)) && (!tags.length || node.tags.some((tag) => tags.includes(tag)))
      && (orphans === "only" ? node.degree === 0 : orphans === "hide" ? node.degree > 0 : true);
    let selected;
    if (byId.has(focus)) {
      const neighbours = new Map();
      for (const edge of full.edges) {
        (neighbours.get(edge.from) || neighbours.set(edge.from, []).get(edge.from)).push(edge.to);
        (neighbours.get(edge.to) || neighbours.set(edge.to, []).get(edge.to)).push(edge.from);
      }
      selected = new Set([focus]);
      const queue = [[focus, 0]];
      while (queue.length) {
        const [id, distance] = queue.shift();
        if (distance === hops) continue;
        for (const next of neighbours.get(id) || []) {
          if (eligible(byId.get(next)) && !selected.has(next)) { selected.add(next); queue.push([next, distance + 1]); }
        }
      }
    } else {
      selected = new Set(full.nodes.filter(eligible).map((node) => node.id));
    }
    return {
      nodes: full.nodes.filter((node) => selected.has(node.id)),
      edges: full.edges.filter((edge) => selected.has(edge.from) && selected.has(edge.to)),
      total: full.total, truncated: false, edgeKinds: full.edgeKinds,
    };
  }

  function source(params) {
    const path = text(params, "path");
    const lines = snapshot.sources[path]?.split("\n");
    if (!lines) return { status: 404, error: "source not available" };
    const start = Math.max(uint(params, "start", 1), 1);
    const end = uint(params, "end", start + 79);
    const slice = lines.slice(start - 1, Math.min(end, start + 1499));
    const language = snapshot.findings.find((row) => row.path === path)?.language ?? null;
    return { path, start, lines: slice, truncated: end - start + 1 > 1500, language };
  }

  const routes = {
    meta: () => snapshot.meta,
    summary,
    stats: () => ({ ...summary(), measurements: null }),
    findings, finding, notes, graph, source,
    "notes-graph": graph,
    note: (params) => snapshot.noteDetails[text(params, "path")] || UNAVAILABLE,
    ignore: () => ({ path: ".mctignore", exists: false, content: "", template: "", directive: "@import-gitignore", command: "", importsGitignore: false, gitignorePresent: false, maxBytes: 0 }),
  };

  /** Returns `{ status, body }` for an `api/...` request, or null when it is not an API call. */
  return function handle(url, method, bodyText) {
    const parsed = new URL(url, "http://demo.invalid/");
    const name = parsed.pathname.split("/api/").pop();
    if (!parsed.pathname.includes("/api/")) return null;
    if (method === "POST") {
      if (name === "review") return respond(review(JSON.parse(bodyText || "{}")));
      return respond({ status: 403, error: "Saving is disabled in the demo." });
    }
    const route = routes[name];
    return respond(route ? route(parsed.searchParams) : UNAVAILABLE);
  };
}

function respond(value) {
  const failed = typeof value?.status === "number" && typeof value.error === "string";
  return { status: failed ? value.status : 200, body: failed ? { error: value.error } : value };
}
