// Plain-text builders for the "copy for an AI" buttons. They only format data the page already holds.
const range = (start, end) => (end && end !== start ? `${start}-${end}` : `${start}`);
const fence = (lines, language) => ["```" + (language || ""), ...(lines || []), "```"].join("\n");

/** `path:start-end (name)`: where one copy lives and which function or method it is. */
export const memberLabel = (m) => `${m.path}:${range(m.start, m.end)}${m.name ? ` (${m.name})` : ""}`;

export function duplicateText(item) {
  const out = [`Duplicate code group — ${item.title}`, `key: ${item.key}`, `language: ${item.language} · verdict: ${item.verdict} · ${item.lines} lines, ${item.tokens} tokens · state: ${item.state}`, "",
    `The same code appears in ${item.members.length} places:`, ...item.members.map((m) => `- ${memberLabel(m)}`), ""];
  item.members.forEach((m, index) => out.push(`### Member ${index + 1}: ${m.name}`, `${m.path}:${range(m.start, m.end)}`, fence(m.excerpt, item.language), ""));
  return out.join("\n").trimEnd();
}

export function deadText(item) {
  const s = item.detail.symbol;
  const out = [`Dead-code candidate — ${item.title} (${item.kind})`, `key: ${item.key}`, `${s.path}:${range(s.line, s.endLine)}`, `language: ${s.language} · state: ${item.state} · known references: 0 (heuristic, verify before deleting)`];
  if (item.detail.code) out.push("", fence(item.detail.code.lines, s.language));
  return out.join("\n");
}

export function symbolText(s, data) {
  const out = [`Symbol — ${s.parent ? `${s.parent}::` : ""}${s.name} (${s.kind})`, `id: ${s.id}`, `${s.path}:${range(s.line, s.endLine)}`, `language: ${s.language} · references: ${s.refs}`];
  for (const group of data.duplicates || []) {
    const others = group.members.filter((m) => !(m.path === s.path && m.start === s.line));
    out.push("", `Duplicated in ${others.length} other places (group ${group.key}, ${group.verdict}, ${group.lines} lines):`, ...others.map((m) => `- ${memberLabel(m)}`));
  }
  if (data.dead) out.push("", `Dead-code candidate (key ${data.dead.key}): no known references, heuristic, verify before deleting`);
  if (data.code) out.push("", fence(data.code.lines, s.language));
  return out.join("\n");
}

// One line per row, then the ids on their own line so they can be pasted into a query.
export function listText({ title, total, columns, rows, idKey }) {
  const head = `${title} — ${rows.length} of ${total} rows (this page)`;
  const body = rows.map((r) => columns.map((c) => c(r)).join(" | "));
  return [head, ...body, "", `${idKey}s: ${rows.map((r) => r[idKey]).join(",")}`].join("\n");
}
