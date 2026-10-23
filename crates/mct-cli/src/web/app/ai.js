// Plain-text builders for the "copy for an AI" buttons. They only format data the page already holds.
const range = (start, end) => (end && end !== start ? `${start}-${end}` : `${start}`);
const fence = (lines, language) => ["```" + (language || ""), ...(lines || []), "```"].join("\n");

/** `path:start-end (name)`: where one copy lives and which function or method it is. */
export const memberLabel = (m) => `${m.path}:${range(m.start, m.end)}${m.name ? ` (${m.name})` : ""}`;

const whole = (m) => (m.code && !m.codeTruncated ? m.code : null);

/** The lines of `other` that differ from `base` as `line n: old → new`; null when the shapes differ too much to compare by line. */
function lineChanges(base, other) {
  if (base.length !== other.length) return null;
  const changes = [];
  base.forEach((line, index) => { if (line !== other[index]) changes.push(`  line ${index + 1}: \`${line.trim()}\` → \`${other[index].trim()}\``); });
  return changes;
}

/** Copies with the same body are printed once with all their locations; a variant says only what differs. */
function copySections(item) {
  const variants = new Map(), partial = [];
  for (const m of item.members) {
    const body = whole(m);
    if (!body) { partial.push(m); continue; }
    const key = body.join("\n");
    if (!variants.has(key)) variants.set(key, { body, members: [] });
    variants.get(key).members.push(m);
  }
  const out = [], list = [...variants.values()];
  list.forEach((variant, index) => {
    const count = variant.members.length, places = variant.members.map((m) => `- ${memberLabel(m)}`);
    if (index === 0) out.push(`### Code${count > 1 ? ` (identical in ${count} copies)` : ""}`, ...places, fence(variant.body, item.language), "");
    else {
      const changes = lineChanges(list[0].body, variant.body);
      out.push(`### Variant${count > 1 ? ` (identical in ${count} copies)` : ""} — ${changes ? "differs from the code above in:" : "different code:"}`, ...places,
        ...(changes ? changes : [fence(variant.body, item.language)]), "");
    }
  });
  partial.forEach((m) => out.push(`### ${memberLabel(m)} — ${m.code ? "body cut at the size limit" : "body not included (size budget)"}; only the start is shown, read the rest at ${m.path}:${range(m.start, m.end)}`, fence(m.code || m.excerpt, item.language), ""));
  return out;
}

export function duplicateText(item) {
  const out = [`Duplicate code group — ${item.title}`, `key: ${item.key}`, `language: ${item.language} · verdict: ${item.verdict} · ${item.lines} lines, ${item.tokens} tokens · state: ${item.state}`, "",
    `The same code appears in ${item.members.length} places.`, ""];
  return [...out, ...copySections(item)].join("\n").trimEnd();
}

export function deadText(item) {
  const s = item.detail.symbol;
  const out = [`Dead-code candidate — ${item.title} (${item.kind})`, `key: ${item.key}`, `${s.path}:${range(s.line, s.endLine)}`, `language: ${s.language} · state: ${item.state} · known references: 0 (heuristic, verify before deleting)`];
  if (item.detail.code) out.push("", fence(item.detail.code.lines, s.language));
  return out.join("\n");
}

/** `kind` picks the finding the text is about: "duplicate" or "dead". */
export function symbolText(s, data, kind) {
  const out = [`Symbol — ${s.parent ? `${s.parent}::` : ""}${s.name} (${s.kind})`, `id: ${s.id}`, `${s.path}:${range(s.line, s.endLine)}`, `language: ${s.language} · references: ${s.refs}`];
  if (kind === "duplicate") for (const group of data.duplicates || []) {
    const others = group.members.filter((m) => !(m.path === s.path && m.start === s.line));
    out.push("", `Duplicated in ${others.length} other places (group ${group.key}, ${group.verdict}, ${group.lines} lines):`, ...others.map((m) => `- ${memberLabel(m)}`));
  }
  if (kind === "dead" && data.dead) out.push("", `Dead-code candidate (key ${data.dead.key}): no known references, heuristic, verify before deleting`);
  if (data.code) out.push("", fence(data.code.lines, s.language));
  return out.join("\n");
}

// "Plantilla para IA": the same evidence as the copy buttons, preceded by a task and its rules, so the
// text can be pasted into an assistant as it is. Nothing here asks the assistant to change files unprompted.
const RULES = ["Rules:", "- The indexer is a heuristic: its verdicts and counts are hints, not proof. Check the code yourself.", "- Paths and line numbers are exact. Read the files before judging.", "- Do not edit anything. Answer first and wait for my confirmation."];
const template = (task, steps, evidence) => [`# Task: ${task}`, "", ...steps.map((step, index) => `${index + 1}. ${step}`), "", ...RULES, "", "## Evidence", "", evidence].join("\n");

export const duplicateTemplate = (item) => template(`decide whether this duplicate-code group should be merged`, [
  "Read every copy listed below and compare them line by line.",
  "Say whether it is real duplication worth removing, or a false positive (tests, generated code, parallel implementations that should stay separate).",
  "If it is real, propose one shared function or module: its name, the file it belongs in, and each copy that must call it.",
  "Answer with: verdict (merge / keep / unsure), up to 5 lines of reasoning, and the proposed change.",
], duplicateText(item));

export const deadTemplate = (item) => template(`check whether this dead-code candidate can be deleted`, [
  "Search the whole repository for the symbol name, including tests, macros, re-exports, string and reflection uses and other languages.",
  "The indexer found no references by name, which misses dynamic or public-API uses. Say what could still be using it.",
  "Answer with: verdict (delete / keep / unsure), the evidence you found, and the exact lines to remove if delete.",
], deadText(item));

/** The findings a symbol carries; one with none has nothing to ask an assistant about. */
export const findingKinds = (data) => [data.duplicates?.length && "duplicate", data.dead && "dead"].filter(Boolean);

export function symbolTemplate(s, data, kind) {
  if (kind === "duplicate") return template(`decide whether this symbol's duplicated code should be merged`, [
    "Read this symbol and every other copy listed under 'Duplicated in', and compare them.",
    "Say whether it is real duplication or a false positive, and if real, propose one shared function with the call sites to change.",
    "Answer with: verdict (merge / keep / unsure), up to 5 lines of reasoning, and the proposed change.",
  ], symbolText(s, data, kind));
  return template(`check whether this symbol can be deleted`, [
    "Search the whole repository for the name, including tests, macros, re-exports and dynamic uses.",
    "Answer with: verdict (delete / keep / unsure), the evidence you found, and the exact lines to remove if delete.",
  ], symbolText(s, data, kind));
}

// One line per row, then the ids on their own line so they can be pasted into a query.
export function listText({ title, total, columns, rows, idKey }) {
  const head = `${title} — ${rows.length} of ${total} rows (this page)`;
  const body = rows.map((r) => columns.map((c) => c(r)).join(" | "));
  return [head, ...body, "", `${idKey}s: ${rows.map((r) => r[idKey]).join(",")}`].join("\n");
}
