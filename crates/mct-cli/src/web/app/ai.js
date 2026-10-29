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
  base.forEach((line, index) => {
    if (line !== other[index])
      changes.push(`  line ${index + 1}: \`${line.trim()}\` → \`${other[index].trim()}\``);
  });
  return changes;
}

/** Copies with the same body are printed once with all their locations; a variant says only what differs. */
function copySections(item) {
  const variants = new Map();
  const partial = [];
  for (const m of item.members) {
    const body = whole(m);
    if (!body) {
      partial.push(m);
      continue;
    }
    const key = body.join("\n");
    if (!variants.has(key)) variants.set(key, { body, members: [] });
    variants.get(key).members.push(m);
  }
  const out = [];
  const list = [...variants.values()];
  list.forEach((variant, index) => {
    const count = variant.members.length;
    const places = variant.members.map((m) => `- ${memberLabel(m)}`);
    if (index === 0)
      out.push(
        `### Code${count > 1 ? ` (identical in ${count} copies)` : ""}`,
        ...places,
        fence(variant.body, item.language),
        "",
      );
    else {
      const changes = lineChanges(list[0].body, variant.body);
      out.push(
        `### Variant${count > 1 ? ` (identical in ${count} copies)` : ""} — ${changes ? "differs from the code above in:" : "different code:"}`,
        ...places,
        ...(changes ? changes : [fence(variant.body, item.language)]),
        "",
      );
    }
  });
  partial.forEach((m) =>
    out.push(
      `### ${memberLabel(m)} — ${m.code ? "body cut at the size limit" : "body not included (size budget)"}; only the start is shown, read the rest at ${m.path}:${range(m.start, m.end)}`,
      fence(m.code || m.excerpt, item.language),
      "",
    ),
  );
  return out;
}

// ---------- the brief: one text with everything an assistant needs to check a finding ----------
// Context, information, paths and files, the code, the validations to run and the report to write.
// The report asks for evidence per claim and an explicit check of this text against the files, so a
// stale index or a wrong line shows up instead of being trusted.

const RULES = [
  "- The indexer is a heuristic: its verdicts, counts and references are hints, not proof.",
  "- Paths are relative to the repository root. The index can be older than the files: read them before judging.",
  "- Do not edit anything. Answer with the report and wait for my confirmation.",
  "- If you could not run a validation, say so. Do not fill a gap with a guess.",
];

const REPORT = (verdicts, change) => [
  "Answer with exactly these sections:",
  `1. **Verdict**: ${verdicts}.`,
  "2. **Validations**: every validation above, numbered, with `done` or `not done` and what you found.",
  "3. **Evidence**: each claim with the `path:line` it rests on, marked `verified` (you read it) or `assumption` (you inferred it).",
  "4. **Information quality**: whether the paths, lines, names and code in this text match the files. List every mismatch (moved lines, missing or extra copies, renamed symbols); write `none found` only after checking every file listed.",
  `5. **Proposed change**: ${change}, and the commands that prove it (build, tests).`,
  "6. **Risks and open questions**.",
  "7. **Confidence**: high, medium or low, and why.",
];

/** Drops the blank lines a list of sections ends with, leaving the code inside untouched. */
const trimEnd = (lines) => {
  const out = [...lines];
  while (out.length && out.at(-1) === "") out.pop();
  return out;
};

/** Joins the sections; empty entries are dropped so optional facts leave no blank bullets. */
const brief = ({ task, context, information, files, code, validations, verdicts, change }) =>
  [
    `# Task: ${task}`,
    "",
    "## 1. Context",
    ...context.filter(Boolean),
    "",
    "## 2. Information",
    ...information.filter(Boolean),
    "",
    "## 3. Paths and files",
    ...files.filter(Boolean),
    "",
    "## 4. Code",
    "",
    ...trimEnd(code),
    "",
    "## 5. Validations required",
    ...validations.map((step, index) => `${index + 1}. ${step}`),
    "",
    "## 6. Report",
    ...REPORT(verdicts, change),
    "",
    "## Rules",
    ...RULES,
  ].join("\n");

const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
const reviewLine = (item) =>
  `- Review state: ${item.state}${item.note ? ` — reviewer's note: ${item.note}` : ""}`;

const DUPLICATE_VALIDATIONS = [
  "Open every file in section 3 and confirm each range still holds the code in section 4.",
  "Compare the copies line by line and list every difference, including any the detector did not report.",
  "Check whether the copies must stay apart: tests, generated code, other crates or languages, public API, platform-specific code.",
  "Find the callers and the tests of each copy.",
  "If they should be merged, check that one shared place is reachable from every copy without a new dependency cycle.",
];
const DUPLICATE_VERDICTS = "`merge`, `keep` or `unsure`";
const DUPLICATE_CHANGE = "the shared function or module (name and file) and each call site that changes";

const DEAD_VALIDATIONS = [
  "Confirm the symbol is still at the path and lines in section 3.",
  "Search the whole repository for its name: tests, macros, re-exports, strings and reflection, configuration, build scripts, other languages and docs.",
  "Check its visibility: a public or exported item can be used outside this repository (library API, FFI, plugins).",
  "Check the files that import its file (section 3).",
  "Check whether something calls it without naming it: an entry point, a trait or interface implementation, a derive or serialisation hook, a callback, or code behind a feature flag.",
];
const DEAD_VERDICTS = "`delete`, `keep` or `unsure`";
const DEAD_CHANGE = "the exact lines to remove in each file";

export function duplicateBrief(item) {
  const ev = item.evidence;
  const files = new Set(item.members.map((m) => m.path)).size;
  return brief({
    task: "decide whether this duplicate-code group should be merged",
    context: [
      `A local code index (mct) grouped ${item.members.length} functions or methods whose bodies match after normalising names and literals.`,
      "A person reviews each group and records a decision; this text is the evidence for that decision.",
    ],
    information: [
      `- Finding: duplicate${item.number ? ` #${item.number}` : ""} — ${item.title} (key ${item.key})`,
      reviewLine(item),
      `- Language: ${item.language} · ${item.members.length} copies in ${plural(files, "file")} · ${item.lines} lines and ${item.tokens} tokens per copy`,
      `- Detector's verdict: ${item.verdict}`,
      item.priority &&
        `- Priority: ${item.priority}${item.priorityReason ? ` — ${item.priorityReason}` : ""}`,
      item.scope && `- Scope: ${item.scope}`,
      item.method?.normalization && `- Matching: tokens, ${item.method.normalization}`,
      item.reasons?.length && `- Reasons: ${item.reasons.join(", ")}`,
      ev &&
        `- Differences found by the detector: ${ev.distinct}${ev.differences.length < ev.distinct ? ` (the first ${ev.differences.length} below)` : ""}`,
      ...(ev?.differences || []).map(
        (d) =>
          `  - ${d.kind}: ${d.values.map((value, i) => `\`${value}\` at ${item.members[i]?.path}:${d.lines[i]}`).join(" · ")}`,
      ),
      ...(ev?.warnings || []).map((warning) => `- Warning: ${warning}`),
      // The detector's own record of the group, as it wrote it.
      ...(item.analysis ? ["", "Detector's full analysis:", fence(item.analysis.split("\n"), "")] : []),
    ],
    files: item.members.map((m) => `- ${memberLabel(m)}${m.kind ? ` — ${m.kind}` : ""}`),
    code: copySections(item),
    validations: DUPLICATE_VALIDATIONS,
    verdicts: DUPLICATE_VERDICTS,
    change: DUPLICATE_CHANGE,
  });
}

export function deadBrief(item) {
  const s = item.detail.symbol;
  const end = Math.max(s.endLine || s.line, s.line);
  const importedBy = item.detail.importedBy || [];
  return brief({
    task: "check whether this dead-code candidate can be deleted",
    context: [
      "A local code index (mct) found no reference to this symbol by name anywhere in the indexed project.",
      "That misses dynamic, reflective and outside uses, so the candidate must be checked before anything is deleted.",
    ],
    information: [
      `- Finding: dead code${item.number ? ` #${item.number}` : ""} — ${item.title} (key ${item.key})`,
      reviewLine(item),
      `- Symbol: ${item.title} · ${item.kind} · ${s.language || item.language}`,
      `- Visibility: ${item.visibility} · zone: ${item.zone} · confidence: ${item.confidence}`,
      "- Known references: 0",
      `- Its file imports ${plural(item.detail.imports?.length || 0, "file")} and is imported by ${plural(importedBy.length, "file")}`,
    ],
    files: [
      `- ${s.path}:${range(s.line, end)} — the candidate`,
      ...importedBy.map((path) => `- ${path} — imports the candidate's file`),
    ],
    code: item.detail.code
      ? [fence(item.detail.code.lines, s.language)]
      : [`Not included: read ${s.path}:${range(s.line, end)}.`],
    validations: DEAD_VALIDATIONS,
    verdicts: DEAD_VERDICTS,
    change: DEAD_CHANGE,
  });
}

/** The findings a symbol carries; one with none has nothing to ask an assistant about. */
export const findingKinds = (data) =>
  [data.duplicates?.length && "duplicate", data.dead && "dead"].filter(Boolean);

/** A symbol's brief about one of its findings; `kind` is "duplicate" or "dead". */
export function symbolBrief(s, data, kind) {
  const where = `${s.path}:${range(s.line, s.endLine)}`;
  const name = `${s.parent ? `${s.parent}::` : ""}${s.name}`;
  const code = data.code ? [fence(data.code.lines, s.language)] : [`Not included: read ${where}.`];
  const symbolLines = [
    `- Symbol: ${name} · ${s.kind} · ${s.language} (id ${s.id})`,
    `- Known references: ${s.refs}`,
  ];
  if (kind === "duplicate") {
    const groups = data.duplicates || [];
    const others = (group) => group.members.filter((m) => !(m.path === s.path && m.start === s.line));
    return brief({
      task: "decide whether this symbol's duplicated code should be merged",
      context: [
        "A local code index (mct) found other functions or methods whose bodies match this symbol's after normalising names and literals.",
      ],
      information: [
        ...symbolLines,
        ...groups.map(
          (group) =>
            `- Duplicate group ${group.key}: ${group.verdict}, ${group.lines} lines, ${others(group).length} other copies`,
        ),
      ],
      files: [
        `- ${where} — this symbol`,
        ...groups.flatMap((group) =>
          others(group).map((m) => `- ${memberLabel(m)} — copy (group ${group.key})`),
        ),
      ],
      code,
      validations: DUPLICATE_VALIDATIONS,
      verdicts: DUPLICATE_VERDICTS,
      change: DUPLICATE_CHANGE,
    });
  }
  return brief({
    task: "check whether this symbol can be deleted",
    context: [
      "A local code index (mct) found no reference to this symbol by name anywhere in the indexed project.",
      "That misses dynamic, reflective and outside uses, so it must be checked before anything is deleted.",
    ],
    information: [...symbolLines, data.dead && `- Dead-code candidate: key ${data.dead.key}`],
    files: [`- ${where} — the candidate`],
    code,
    validations: DEAD_VALIDATIONS,
    verdicts: DEAD_VERDICTS,
    change: DEAD_CHANGE,
  });
}

// One line per row, then the ids on their own line so they can be pasted into a query.
export function listText({ title, total, columns, rows, idKey }) {
  const head = `${title} — ${rows.length} of ${total} rows (this page)`;
  const body = rows.map((r) => columns.map((c) => c(r)).join(" | "));
  return [head, ...body, "", `${idKey}s: ${rows.map((r) => r[idKey]).join(",")}`].join("\n");
}

/** One line per file the indexer could not fully handle: `path:line (inside symbol) — message`. */
export function issuesText(title, rows) {
  return [
    title,
    ...rows.map(
      (r) =>
        `${r.path}${r.line ? `:${r.line}` : ""}${r.symbol ? ` (inside ${r.symbol.name}, ${r.symbol.kind} ${r.symbol.line}-${r.symbol.endLine ?? r.symbol.line})` : ""} — ${r.message}`,
    ),
  ].join("\n");
}
