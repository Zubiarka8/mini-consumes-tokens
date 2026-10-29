// 03 · Duplicados, 05 · Código muerto and 07 · Revisiones share one findings workbench.
import { h, clear, api, cancelApi, isAbort, patchQuery, onQuery, parseHash, listParam, fmt, ago, hrefTo, review, toast, confirmDialog, noteDialog, copy, forget, store, dateTime } from "./core.js";
import { icon, pageHead, tabsBar, dataTable, pager, badge, stateBadge, STATE_ORDER, stateLabel, searchBox, filterGroup, activeChips, noResults, loading, errorBox, note,
  drawerButton, segmented, emptyState, selectBox } from "./components.js";
import { codeBlock, openSource, compareView, normalizer } from "./code.js";
import { t } from "./i18n.js";
import { duplicateText, deadText, duplicateTemplate, deadTemplate, listText, memberLabel } from "./ai.js";

// Everything that still needs a person: also findings whose code changed
// after a decision, and resolved findings the detector still reports.
const ACTIONABLE = ["unreviewed", "needs-reevaluation", "in-review", "confirmed", "resolved-but-detected"];
const CONFIG = {
  duplicados: { type: "duplicate", title: "dup.title", subtitle: "dup.subtitle", defaultStates: ACTIONABLE, views: true },
  "codigo-muerto": { type: "dead-code", title: "dead.title", subtitle: "dead.subtitle", defaultStates: ACTIONABLE },
  revisiones: { type: "", title: "rev.title", subtitle: "rev.subtitle", defaultStates: null, tabs: true },
};
const BUILTIN_PRESETS = [
  { id: "b-dup", name: "preset.dupPending", query: "type=duplicate&state=unreviewed" },
  { id: "b-dead", name: "preset.deadPending", query: "type=dead-code&state=unreviewed" },
  { id: "b-fp", name: "preset.recentFp", query: "state=false-positive&sort=updated&order=desc" },
  { id: "b-pending", name: "preset.pending", query: "state=unreviewed,in-review" },
];
// Any decision can be replaced by any other at any time; the server keeps every change in the history.
const DECISIONS = ["in-review", "confirmed", "false-positive", "dismissed", "resolved"];
const DECIDED = ["confirmed", "false-positive", "resolved", "dismissed"];
// Lines shown around a dead-code candidate in its code view; 3 when the URL names none.
const CONTEXT_CHOICES = [0, 3, 10, 25];
const actionsFor = (state) => [...DECISIONS.filter((d) => d !== state), ...(state === "unreviewed" ? [] : ["reopen"])];
/** Button text that names both the decision and what it is about ("Confirmar duplicado"). */
const actionLabel = (target, type) => t(`review.action.${target}.${type}`);

/** One-click ignore: no dialog and no reason. The finding stays in the history and can be reopened. */
export async function ignoreFinding(item, onChange) {
  try {
    const saved = await review(item.key, "dismissed", item.note || "", item.revision);
    toast(t("review.ignored"), "ok");
    onChange(saved);
  } catch (error) {
    if (error.status === 409 && error.body?.key) { toast(t("review.conflict"), "err"); onChange(error.body); }
    else toast(t("review.failed", { message: error.message }), "err");
  }
}

export const findingHref = (item) => hrefTo(item.type === "duplicate" ? "duplicados" : "codigo-muerto", { key: item.key });

/** Review controls shared by every finding view; `onChange(updatedReview)` after a saved decision. */
export function reviewPanel(item, onChange) {
  const noteField = h("textarea", { class: "input wide", rows: 6, maxlength: 500, "aria-label": t("review.noteLabel"), placeholder: t("review.notePlaceholder"), value: item.note || "" });
  const apply = async (target) => {
    let note = noteField.value.trim();
    if (target === "false-positive") {
      const ok = await confirmDialog({ title: t("review.fpTitle"), body: t("review.fpBody"), confirm: actionLabel("false-positive", item.type), danger: false });
      if (!ok) return;
    }
    if (DECIDED.includes(item.state) && target !== "dismissed" && target !== "false-positive") {
      const ok = await confirmDialog({ title: t("review.changeTitle"), body: t("review.changeBody", { from: stateLabel(item.state, item.type), to: target === "reopen" ? t("state.unreviewed") : stateLabel(target, item.type) }), confirm: actionLabel(target, item.type) });
      if (!ok) return;
    }
    try {
      const saved = await review(item.key, target === "reopen" ? null : target, note, item.revision);
      toast(t("review.saved", { state: stateLabel(saved.state, item.type) }), "ok");
      onChange(saved);
    } catch (error) {
      if (error.status === 409 && error.body?.key) {
        toast(t("review.conflict"), "err");
        onChange(error.body);
      } else toast(t("review.failed", { message: error.message }), "err");
    }
  };
  // One button per available decision, each naming the decision and the kind of finding it applies to.
  const decisions = item.readOnly ? null : h("div", { class: "stack", style: { gap: "6px" } },
    actionsFor(item.state).map((target) => h("button", { class: "btn", style: { justifyContent: "flex-start" }, onclick: () => apply(target) }, actionLabel(target, item.type))));
  return h("div", { class: "section" }, h("h4", {}, t("review.title")),
    h("div", { class: "decision-now" }, h("span", { class: "muted small" }, t("review.current")), stateBadge(item.state, item.type), item.updated ? h("span", { class: "muted small" }, `${t("review.updated")} ${ago(item.updated)}`) : null),
    item.state === "needs-reevaluation" ? h("p", { class: "muted small" }, t("review.previous", { state: stateLabel(item.previousState, item.type) })) : null,
    item.readOnly ? h("p", { class: "small" }, t("review.readOnly")) : h("label", { class: "field" }, t("review.noteOptional"), noteField),
    decisions ? h("div", { class: "field", style: { marginTop: "10px" } }, h("span", {}, DECIDED.includes(item.state) ? t("review.change") : t("review.decide")), decisions) : null,
    h("p", { class: "muted small", style: { marginTop: "8px" } }, t("review.reviewerNote")));
}

export function historyList(history) {
  if (!history?.length) return h("p", { class: "muted" }, t("review.noHistory"));
  // Short generic labels keep each step on one line; the type is already clear from the page.
  return h("ol", { class: "timeline" }, [...history].reverse().map((event) => h("li", {},
    h("div", { class: "step" }, stateBadge(event.from), h("span", { "aria-hidden": "true" }, "→"), stateBadge(event.to)), h("div", { class: "muted small" }, event.at ? dateTime(event.at) : t("review.unknownTime")), event.note && h("div", {}, `“${event.note}”`))));
}

/** Collapsed by default: the current decision matters more than how it got there. */
const historySection = (item) => h("details", { class: "section history" }, h("summary", {}, `${t("review.history")} (${item.history?.length || 0})`), historyList(item.history));

function duplicateDetail(item, onChange, setView) {
  const members = item.members;
  return h("div", { class: "card inspector" },
    h("header", {}, h("div", {}, h("h3", {}, item.title), h("div", { class: "row", style: { marginTop: "6px" } }, badge(item.language), stateBadge(item.state, item.type), badge(t(`dup.verdict.${item.verdict}`), item.verdict === "real" ? "success" : "warning"))),
      h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => patchQuery({ key: null, view: null }) }, icon("x"))),
    note(t("dup.caution"), "warn", "alert"),
    h("div", { class: "actions" }, h("button", { class: "btn primary sm", onclick: () => setView("side") }, icon("split", 14), t("dup.compare")),
      h("button", { class: "btn sm", onclick: () => copy(duplicateText(item), t("ai.copied")) }, icon("copy", 14), t("ai.copy.duplicate")),
      h("button", { class: "btn sm", onclick: () => copy(duplicateTemplate(item), t("ai.templateCopied")) }, icon("copy", 14), t("ai.template.duplicate"))),
    reviewPanel(item, onChange),
    historySection(item),
    h("div", { class: "section" }, h("h4", {}, t("dup.evidence")),
      h("dl", { class: "kv" },
        h("dt", {}, t("dup.method")), h("dd", {}, t("dup.methodTokens", { mode: item.method?.normalization || "—" })),
        h("dt", {}, t("dup.verdictLabel")), h("dd", {}, t(`dup.verdict.${item.verdict}`)),
        h("dt", {}, t("dup.size")), h("dd", {}, t("dup.sizeValue", { lines: item.lines, tokens: item.tokens })),
        h("dt", {}, t("dup.priority")), h("dd", {}, t(`dup.priority.${item.priority}`), item.priorityReason && [" — ", h("span", { lang: "en" }, item.priorityReason)]),
        h("dt", {}, t("dup.scope")), h("dd", {}, t(`dup.scope.${item.scope}`))),
      h("ul", { style: { margin: "8px 0 0", paddingLeft: "18px" } }, (item.reasons || []).map((reason) => h("li", {}, t(`dup.reason.${reason}`)))),
      h("p", { class: "muted small" }, t("dup.noSimilarity"))),
    h("div", { class: "section" }, h("h4", {}, t("dup.members", { n: members.length })),
      members.map((m) => h("div", { style: { marginBottom: "10px" } },
        h("div", { class: "row spread" }, h("strong", {}, m.name), h("span", { class: "row" }, h("button", { class: "btn ghost sm", "aria-label": `${t("ai.copyCode")}: ${m.name}`, onclick: () => copy(`${m.path}:${m.start}-${m.end}\n${(m.code || m.excerpt).join("\n")}`, t("ai.codeCopied")) }, icon("copy", 13), t("ai.copyCode")), h("button", { class: "btn ghost sm", "aria-label": `${t("code.openSource")}: ${m.name}`, onclick: () => openSource({ path: m.path, line: m.start, language: item.language }) }, icon("external", 13), t("code.openSource")))),
        h("div", { class: "mono muted small" }, `${m.path}:${m.start}–${m.end}`),
        codeBlock({ lines: m.excerpt, start: m.start, language: item.language, maxHeight: "140px", label: `${m.name} — ${m.path}:${m.start}` })))),
    item.analysis && h("details", { class: "section" }, h("summary", {}, t("dup.technical")), h("pre", { class: "mono", style: { whiteSpace: "pre-wrap", overflow: "auto", maxHeight: "300px" } }, item.analysis)));
}

function deadDetail(item, onChange, setView) {
  const d = item.detail;
  const s = d.symbol;
  return h("div", { class: "card inspector" },
    h("header", {}, h("div", {}, h("h3", {}, item.title), h("div", { class: "row", style: { marginTop: "6px" } }, badge(item.kind), badge(item.language), stateBadge(item.state, item.type), badge(t(`dead.confidence.${item.confidence}`), item.confidence === "high" ? "danger" : item.confidence === "medium" ? "warning" : ""))),
      h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => patchQuery({ key: null }) }, icon("x"))),
    note(t("dead.caution"), "warn", "alert"),
    h("dl", { class: "kv", style: { margin: "10px 0" } }, h("dt", {}, t("dead.definition")), h("dd", { class: "mono" }, `${s.path}:${s.line}${s.endLine ? `–${s.endLine}` : ""}`),
      h("dt", {}, t("dead.zone")), h("dd", {}, t(`dead.zone.${item.zone}`)), h("dt", {}, t("dead.visibility")), h("dd", {}, t(`dead.visibility.${item.visibility}`)), h("dt", {}, t("dead.knownRefs")), h("dd", {}, "0"), h("dt", {}, t("dup.method")), h("dd", {}, t("dead.method"))),
    h("div", { class: "actions" },
      h("button", { class: "btn primary sm", onclick: () => setView("code") }, icon("code", 14), t("dead.openCode")),
      h("button", { class: "btn sm", onclick: () => openSource({ path: s.path, line: s.line, language: s.language }) }, icon("external", 14), t("code.openSource")),
      h("button", { class: "btn sm", onclick: () => copy(`${s.path}:${s.line}${s.endLine ? `-${s.endLine}` : ""}`, t("common.pathCopied")) }, icon("copy", 14), t("code.copyPath")),
      h("button", { class: "btn sm", onclick: () => copy(deadText(item), t("ai.copied")) }, icon("copy", 14), t("ai.copy.dead")),
      h("button", { class: "btn sm", onclick: () => copy(deadTemplate(item), t("ai.templateCopied")) }, icon("copy", 14), t("ai.template.dead"))),
    reviewPanel(item, onChange),
    historySection(item),
    d.code ? codeBlock({ lines: d.code.lines, start: d.code.start, language: s.language, maxHeight: "240px", label: t("code.preview") }) : null,
    h("div", { class: "section" }, h("h4", {}, t("dead.why")), h("p", {}, t("dead.whyText", { name: item.title }))),
    h("div", { class: "section" }, h("h4", {}, t("dead.limits")), h("ul", { style: { margin: 0, paddingLeft: "18px" } }, ["byName", "dynamic", "macros", "ffi", "public", "tests", "generated", "config"].map((k) => h("li", {}, t(`dead.limit.${k}`))))),
    h("div", { class: "section" }, h("h4", {}, t("dead.deps")),
      h("p", { class: "muted" }, t("dead.imports", { n: d.imports.length, m: d.importedBy.length })),
      d.importedBy.slice(0, 6).map((p) => h("div", { class: "mono small" }, p))),
    h("div", { class: "section" }, h("h4", {}, t("code.notes")), d.notes.length ? d.notes.map((n) => h("p", {}, h("a", { href: hrefTo("obsidian", { note: n.path }) }, n.title))) : h("p", { class: "muted" }, t("code.noNotes"))),
    h("p", { class: "muted small" }, t("dead.neverDelete")));
}

export async function render({ root, page, params: initial }) {
  const cfg = CONFIG[page];
  const results = h("div", {}), inspector = h("div", { class: "inspector-col sticky", hidden: true }), filtersHost = h("div", {});
  const tabsHost = h("div", {}), chipsHost = h("div", {}), main = h("div", {});
  const drawer = { current: null };
  let facets = {}, selected = null, view = "list", lastQuery = "", currentItem = null, comparisonGeneration = 0;
  const q = () => parseHash().params;

  const effectiveStates = (params) => {
    const raw = params.get("state");
    if (raw === "all") return [];
    if (raw) return raw.split(",").filter(Boolean);
    return cfg.defaultStates || [];
  };
  // Dead code starts on the source zone only: tests, benchmarks, examples and the website are reached by tooling, not by name.
  const effectiveZones = (params) => {
    if (cfg.type !== "dead-code") return [];
    const raw = params.get("zone");
    if (raw === "all") return [];
    return raw ? raw.split(",").filter(Boolean) : ["source"];
  };
  const queryFor = (params) => ({
    type: cfg.type || params.get("type") || "", state: effectiveStates(params), language: listParam(params, "language"), kind: listParam(params, "kind"),
    verdict: listParam(params, "verdict"), scope: listParam(params, "scope"), zone: effectiveZones(params), confidence: listParam(params, "confidence"), q: params.get("q") || "", dir: params.get("dir") || "",
    sort: params.get("sort") || "rank", order: params.get("order") || "asc", page: Number(params.get("page") || 0), size: 25,
  });
  const clearAll = () => { forget(page); patchQuery({ state: cfg.defaultStates ? "all" : null, language: null, kind: null, verdict: null, scope: null, zone: cfg.type === "dead-code" ? "all" : null, confidence: null, q: null, dir: null, type: null, sort: null, order: null }); };

  const opts = (name, labeler = (v) => v) => (facets[name] || []).map(([value, count]) => ({ value, label: labeler(value), count }));

  // ----- presets (Revisiones) -----
  const presets = () => store.get("presets", []);
  const presetBox = () => {
    const mine = presets();
    const applyPreset = (query) => { forget(page); history.replaceState(null, "", `#/${page}?${query}`); patchQuery({ page: null }); };
    return h("details", { class: "fgroup", open: true }, h("summary", {}, t("preset.title")),
      [...BUILTIN_PRESETS.map((p) => ({ ...p, name: t(p.name), builtin: true })), ...mine].map((p) => h("div", { class: "row spread", style: { padding: "2px 0" } },
        h("button", { class: "btn ghost sm ellipsis", onclick: () => applyPreset(p.query) }, p.name),
        !p.builtin && h("span", { class: "row", style: { gap: "2px" } },
          h("button", { class: "icon-btn", style: { width: "28px", height: "28px" }, "aria-label": t("preset.rename", { name: p.name }), onclick: async () => { const name = await noteDialog({ title: t("preset.renameTitle"), required: true, initial: p.name, confirm: t("preset.renameConfirm") }); if (name) { store.set("presets", presets().map((x) => (x.id === p.id ? { ...x, name } : x))); drawFilters(q()); } } }, icon("edit", 14)),
          h("button", { class: "icon-btn", style: { width: "28px", height: "28px" }, "aria-label": t("preset.delete", { name: p.name }), onclick: async () => { if (await confirmDialog({ title: t("preset.deleteTitle"), body: p.name, confirm: t("preset.deleteConfirm"), danger: true })) { store.set("presets", presets().filter((x) => x.id !== p.id)); drawFilters(q()); } } }, icon("trash", 14))))),
      h("button", { class: "btn sm", style: { marginTop: "6px" }, onclick: async () => {
        const name = await noteDialog({ title: t("preset.saveTitle"), body: t("preset.saveBody"), required: true, confirm: t("preset.saveConfirm") });
        if (!name) return;
        store.set("presets", [...presets(), { id: `p${Date.now()}`, name, query: q().toString() }]);
        toast(t("preset.saved"), "ok"); drawFilters(q());
      } }, icon("plus", 14), t("preset.save")));
  };

  function drawFilters(params) {
    const states = effectiveStates(params);
    const groups = [
      { id: "state", label: t("filters.state"), options: STATE_ORDER.map((s) => ({ value: s, label: stateLabel(s), count: (facets.state || []).find(([v]) => v === s)?.[1] || 0 })), selected: states, onChange: (v) => patchQuery({ state: v.length ? v : "all" }) },
      cfg.type === "duplicate" && { id: "verdict", label: t("filters.verdict"), options: opts("verdict", (v) => t(`dup.verdict.${v}`)), selected: listParam(params, "verdict"), onChange: (v) => patchQuery({ verdict: v }) },
      cfg.type === "duplicate" && { id: "scope", label: t("filters.scope"), options: opts("scope", (v) => t(`dup.scope.${v}`)), selected: listParam(params, "scope"), onChange: (v) => patchQuery({ scope: v }) },
      cfg.type === "dead-code" && { id: "confidence", label: t("filters.confidence"), options: ["high", "medium", "low"].map((v) => ({ value: v, label: t(`dead.confidence.${v}`), count: (facets.confidence || []).find(([x]) => x === v)?.[1] || 0 })), selected: listParam(params, "confidence"), onChange: (v) => patchQuery({ confidence: v }) },
      cfg.type === "dead-code" && { id: "zone", label: t("filters.zone"), options: ["source", "tests", "benchmarks", "examples", "website"].map((v) => ({ value: v, label: t(`dead.zone.${v}`), count: (facets.zone || []).find(([x]) => x === v)?.[1] || 0 })), selected: effectiveZones(params), onChange: (v) => patchQuery({ zone: v.length ? v : "all" }) },
      { id: "language", label: t("filters.language"), options: opts("language"), selected: listParam(params, "language"), onChange: (v) => patchQuery({ language: v }) },
      { id: "kind", label: t("filters.symbolKind"), options: opts("kind"), selected: listParam(params, "kind"), onChange: (v) => patchQuery({ kind: v }), open: false, search: true },
      !cfg.type && { id: "type", label: t("filters.type"), options: opts("type", (v) => t(v === "duplicate" ? "common.duplicate" : "common.deadCode")), selected: listParam(params, "type"), onChange: (v) => patchQuery({ type: v.length === 1 ? v[0] : null }) },
    ].filter(Boolean);
    clear(filtersHost).append(h("div", { class: "card filters" },
      h("div", { class: "head" }, h("h2", {}, t("filters.title")), h("div", { class: "row" }, h("button", { class: "btn ghost sm", onclick: clearAll }, t("filters.clear")),
        h("button", { class: "icon-btn only-mobile", "aria-label": t("common.close"), onclick: () => drawer.current?.classList.remove("open") }, icon("x")))),
      groups.map((g) => filterGroup(g)), cfg.tabs ? presetBox() : null,
      h("p", { class: "logic" }, t("filters.logic")), note(t(cfg.type === "duplicate" ? "dup.unsupported" : cfg.type === "dead-code" ? "dead.unsupported" : "rev.unsupported"), "", "info")));
  }

  function drawTabs(params) {
    if (!cfg.tabs) return;
    const states = effectiveStates(params);
    const count = (s) => (facets.state || []).find(([v]) => v === s)?.[1] || 0;
    const total = (facets.state || []).reduce((n, [, c]) => n + c, 0);
    const active = states.length === 1 ? states[0] : states.length === 0 ? "all" : "";
    clear(tabsHost).append(tabsBar({ label: t("rev.tabs"), active, onSelect: (id) => patchQuery({ state: id === "all" ? null : id }),
      tabs: [{ id: "all", label: t("rev.tab.all"), count: total }, ...STATE_ORDER.map((s) => ({ id: s, label: stateLabel(s), count: count(s) }))] }));
  }

  function drawChips(params) {
    const chips = [];
    const states = effectiveStates(params);
    if (cfg.defaultStates && params.get("state") !== "all" && !params.get("state")) chips.push({ label: t("filters.hiding", { states: STATE_ORDER.filter((s) => !cfg.defaultStates.includes(s)).map(stateLabel).join(", ") }), remove: () => patchQuery({ state: "all" }) });
    else if (!cfg.tabs) for (const s of states) chips.push({ label: stateLabel(s), remove: () => patchQuery({ state: states.filter((x) => x !== s).join(",") || "all" }) });
    if (cfg.type === "dead-code" && !params.get("zone")) chips.push({ label: t("filters.zoneOnly"), remove: () => patchQuery({ zone: "all" }) });
    else if (cfg.type === "dead-code" && params.get("zone") !== "all") for (const v of effectiveZones(params)) chips.push({ label: t(`dead.zone.${v}`), remove: () => patchQuery({ zone: effectiveZones(params).filter((x) => x !== v).join(",") || "all" }) });
    for (const key of ["language", "kind", "verdict", "scope", "confidence"]) for (const v of listParam(params, key)) chips.push({ label: key === "verdict" ? t(`dup.verdict.${v}`) : key === "scope" ? t(`dup.scope.${v}`) : key === "confidence" ? t(`dead.confidence.${v}`) : v, remove: () => patchQuery({ [key]: listParam(params, key).filter((x) => x !== v) }) });
    if (params.get("q")) chips.push({ label: `“${params.get("q")}”`, remove: () => patchQuery({ q: null }) });
    if (params.get("dir")) chips.push({ label: params.get("dir"), remove: () => patchQuery({ dir: null }) });
    clear(chipsHost).append(activeChips(chips, clearAll) || "");
  }

  const location = (r) => (r.type === "duplicate" ? r.members.map((m) => `${m.path}:${m.start}`).slice(0, 2).join("  ↔  ") : `${r.path}:${r.line}`);
  function columns() {
    const common = {
      state: { key: "state", label: t("col.state"), sortable: true, render: (r) => stateBadge(r.state, r.type) },
      updated: { key: "updated", label: t("col.updated"), sortable: true, render: (r) => (r.updated ? ago(r.updated) : "—"), cls: "hide-sm" },
      actions: { key: "actions", label: t("col.actions"), cls: "sticky-end", render: (r) => h("span", { class: "row", style: { gap: "4px" } },
        r.type === "duplicate" && h("button", { class: "btn sm", "aria-label": `${t("dup.compare")}: ${r.title}`, onclick: () => patchQuery({ key: r.key, view: "side" }) }, icon("split", 14), t("dup.compareShort")),
        r.type === "dead-code" && h("button", { class: "btn sm", "aria-label": `${t("code.viewSource")}: ${r.title}`, onclick: () => patchQuery({ key: r.key, view: "code" }) }, icon("code", 14), t("code.view")),
        !r.readOnly && r.state !== "dismissed" && h("button", { class: "btn sm", "aria-label": `${actionLabel("dismissed", r.type)}: ${r.title}`, onclick: () => ignoreFinding(r, () => refresh()) }, icon("x", 14), actionLabel("dismissed", r.type))) },
    };
    if (cfg.type === "duplicate") return [
      { key: "verdict", label: t("col.verdict"), sortable: true, render: (r) => badge(t(`dup.verdict.${r.verdict}`), r.verdict === "real" ? "success" : "warning") },
      { key: "title", label: t("col.symbols"), sortable: true, render: (r) => h("span", { class: "name" }, h("span", { class: "ellipsis" }, r.title), r.memberCount > 2 && badge(`+${r.memberCount - 2}`)) },
      { key: "path", label: t("col.files"), sortable: true, render: (r) => [...r.members.slice(0, 4).map((m) => h("span", { class: "path", title: memberLabel(m) }, memberLabel(m))), r.members.length > 4 && h("span", { class: "muted small" }, `+${r.members.length - 4}`)], cls: "hide-sm" },
      { key: "language", label: t("code.col.language"), sortable: true, render: (r) => badge(r.language), cls: "hide-sm" },
      { key: "lines", label: t("col.lines"), sortable: true, num: true, render: (r) => r.lines, cls: "hide-sm" },
      { key: "method", label: t("dup.method"), render: () => t("dup.methodShort"), cls: "hide-sm" }, common.state, common.actions];
    if (cfg.type === "dead-code") return [
      { key: "title", label: t("col.symbol"), sortable: true, render: (r) => h("span", { class: "name" }, h("span", { class: "ellipsis" }, r.title)) },
      { key: "kind", label: t("code.col.kind"), sortable: true, render: (r) => r.kind, cls: "hide-sm" },
      { key: "path", label: t("col.file"), sortable: true, render: (r) => h("span", { class: "path", title: r.path }, `${r.path}:${r.line}`) },
      { key: "zone", label: t("col.zone"), sortable: true, render: (r) => h("span", { style: { whiteSpace: "nowrap" } }, t(`dead.zone.${r.zone}`)), cls: "hide-sm" },
      { key: "confidence", label: t("col.confidence"), sortable: true, render: (r) => badge(t(`dead.confidence.${r.confidence}`), r.confidence === "high" ? "danger" : r.confidence === "medium" ? "warning" : ""), cls: "hide-sm" }, common.state, common.actions];
    return [
      { key: "title", label: t("col.finding"), sortable: true, render: (r) => h("span", { class: "name" }, h("span", { class: "ellipsis" }, r.title)) },
      { key: "type", label: t("col.type"), sortable: true, render: (r) => badge(t(r.type === "duplicate" ? "common.duplicate" : "common.deadCode"), r.type === "duplicate" ? "warning" : "danger") },
      { key: "path", label: t("col.location"), sortable: true, render: (r) => h("span", { class: "path", title: location(r) }, location(r)), cls: "hide-sm" },
      common.state, common.updated, common.actions];
  }

  async function refresh(params = q()) {
    drawChips(params);
    const query = queryFor(params);
    clear(results).append(loading());
    try {
      const data = await api("findings", query, "findings");
      facets = data.facets;
      drawTabs(params); drawFilters(params);
      clear(results);
      if (!data.total) {
        results.append(noResults({ onClear: clearAll, extra: [h("button", { class: "btn", onclick: () => patchQuery({ q: null }) }, t("filters.editSearch"))] }));
        return;
      }
      if (cfg.type === "duplicate") results.append(h("p", { class: "muted", style: { margin: "0 0 8px" } }, t("dup.found", { n: fmt(data.total) })));
      const listColumns = cfg.type === "duplicate" ? [(r) => r.key, (r) => r.title, (r) => r.members.map(memberLabel).join(" ↔ ")] : [(r) => r.key, (r) => r.title, (r) => r.type || cfg.type, (r) => `${r.path || ""}${r.line ? `:${r.line}` : ""}`];
      results.append(h("div", { class: "row", style: { justifyContent: "flex-end", margin: "0 0 8px" } },
        h("button", { class: "btn sm", onclick: () => copy(listText({ title: t(cfg.title), total: data.total, columns: listColumns, rows: data.items, idKey: "key" }), t("ai.listCopied")) }, icon("copy", 14), t("ai.copyList"))));
      results.append(dataTable({ caption: t(cfg.title), rows: data.items, rowKey: (r) => r.key, selected, sort: query.sort, order: query.order,
        onSort: (sort, order) => patchQuery({ sort, order }), onSelect: (row) => patchQuery({ key: row.key }), columns: columns() }),
        pager({ page: data.page, size: data.size, total: data.total, onPage: (page) => patchQuery({ page }) }));
    } catch (error) { if (!isAbort(error)) clear(results).append(errorBox(error, () => refresh(params))); }
  }

  async function showDetail(key, params = q()) {
    selected = key;
    currentItem = null;
    stopComparison();
    cancelApi("finding");
    root.querySelectorAll("tbody tr").forEach((row) => row.setAttribute("aria-selected", String(row.dataset.key === key)));
    view = viewOf(params);
    const bench = root.querySelector(".workbench");
    if (!key) { inspector.hidden = true; bench?.classList.remove("inspect"); showMain(null); return; }
    inspector.hidden = false; bench?.classList.add("inspect");
    clear(inspector).append(h("div", { class: "card inspector" }, loading()));
    try {
      const item = await api("finding", { key }, "finding");
      if (selected !== key) return;
      currentItem = item;
      const changed = () => { refresh(q()); showDetail(key, q()); };
      const setView = (v) => patchQuery({ view: v === "side" || v === "code" ? v : null, key });
      clear(inspector).append(item.type === "duplicate" ? duplicateDetail(item, changed, setView) : deadDetail(item, changed, setView));
      showMain(item, params);
    } catch (error) { if (!isAbort(error) && selected === key) { clear(inspector).append(h("div", { class: "card inspector" }, errorBox(error, () => showDetail(key, q())))); showMain(null); } }
  }

  // The main area shows the list, a duplicate comparison ("side") or a dead-code candidate's code ("code").
  const viewOf = (params) => (["side", "code"].includes(params.get("view")) ? params.get("view") : "list");

  // ----- side-by-side comparison (duplicates) -----
  const compareState = { ignoreSpace: false, ignoreComments: false, normalizeIds: false };
  function stopComparison() {
    comparisonGeneration++;
    cancelApi("comparison-left");
    cancelApi("comparison-right");
    cancelApi("dead-code-source");
  }
  async function showMain(item, params = q()) {
    stopComparison();
    const generation = comparisonGeneration;
    const side = item?.type === "duplicate" && params.get("view") === "side";
    const code = item?.type === "dead-code" && params.get("view") === "code";
    results.hidden = side || code; tabsHost.hidden = side || code; chipsHost.hidden = side || code;
    root.querySelector(".workbench")?.classList.toggle("compare-mode", side || code);
    clear(main);
    if (code) return showDeadCode(item, params, generation);
    if (!side) return;
    const validIndex = (name, fallback) => {
      const raw = params.get(name), index = raw === null ? fallback : Number(raw);
      return Number.isInteger(index) && index >= 0 && index < item.members.length ? index : fallback;
    };
    const left = validIndex("left", 0);
    let right = validIndex("right", 1);
    if (left === right) right = left === 0 ? 1 : 0;
    const a = item.members[left], b = item.members[right];
    const pick = (name, index, other) => h("label", { class: "field grow" }, t(`dup.${name}Member`),
      selectBox({ label: t(`dup.${name}Member`), value: String(index),
        options: item.members.map((member, i) => [String(i), `${member.name} — ${member.path}:${member.start}`]).filter(([i]) => Number(i) !== other),
        onChange: (value) => patchQuery({ [name]: value }) }));
    const body = h("div", {}, loading());
    main.append(h("div", { class: "row wrap", style: { marginBottom: "10px" } },
      h("button", { class: "btn", onclick: () => patchQuery({ view: null, left: null, right: null }) }, icon("left", 14), t("dup.backToList")),
      pick("left", left, right), pick("right", right, left)), body);
    try {
      const sources = await Promise.all([a, b].map((m, index) => api("source", { path: m.path, start: m.start, end: m.end }, index === 0 ? "comparison-left" : "comparison-right")));
      if (generation !== comparisonGeneration) return;
      const draw = () => {
        const view = compareView({ left: { lines: sources[0].lines, start: a.start, path: a.path, language: item.language }, right: { lines: sources[1].lines, start: b.start, path: b.path, language: item.language }, normalize: normalizer(compareState) });
        const box = (name, label) => h("label", { class: "chip" }, h("input", { type: "checkbox", checked: compareState[name], onchange: (e) => { compareState[name] = e.target.checked; draw(); } }), label);
        clear(body).append(
          h("div", { class: "row spread", style: { marginBottom: "10px" } },
            h("div", { class: "row" }, h("h2", {}, t("dup.compareTitle")), badge(t(`dup.verdict.${item.verdict}`), item.verdict === "real" ? "success" : "warning")),
            h("div", { class: "row", role: "group", "aria-label": t("dup.ignoreDiffs") }, h("span", { class: "muted small" }, t("dup.ignoreDiffs")), box("ignoreSpace", t("dup.ignoreSpace")), box("ignoreComments", t("dup.ignoreComments")), box("normalizeIds", t("dup.normalizeIds")))),
          h("p", { class: "muted small", style: { marginBottom: "8px" } }, t("dup.viewOnly")),
          h("div", { class: "compare" },
            h("div", { class: "pane" }, h("h3", {}, h("span", { class: "mono ellipsis" }, a.path), h("span", { class: "muted" }, a.name)), view.a),
            h("div", { class: "pane" }, h("h3", {}, h("span", { class: "mono ellipsis" }, b.path), h("span", { class: "muted" }, b.name)), view.b)),
          h("div", { class: "card", style: { marginTop: "12px" } }, h("h3", {}, t("dup.diffs", { n: view.changes })), h("p", { class: "muted" }, view.changes ? t("dup.diffsText") : t("dup.noDiffs")),
            null));
      };
      draw();
    } catch (error) { if (!isAbort(error) && generation === comparisonGeneration) { cancelApi("comparison-left"); cancelApi("comparison-right"); clear(body).append(errorBox(error, () => showMain(item, q()))); } }
  }

  // ----- code view (dead code): the comparison layout with one pane and the symbol marked -----
  async function showDeadCode(item, params, generation) {
    const s = item.detail.symbol;
    const end = Math.max(s.endLine || s.line, s.line);
    const asked = params.get("context") === null ? 3 : Number(params.get("context"));
    const context = CONTEXT_CHOICES.includes(asked) ? asked : 3;
    const body = h("div", {}, loading());
    main.append(h("div", { class: "row wrap", style: { marginBottom: "10px" } },
      h("button", { class: "btn", onclick: () => patchQuery({ view: null, context: null }) }, icon("left", 14), t("dup.backToList")),
      h("label", { class: "field grow" }, t("dead.context"),
        selectBox({ label: t("dead.context"), value: String(context),
          options: CONTEXT_CHOICES.map((n) => [String(n), n ? t("dead.contextLines", { n }) : t("dead.contextNone")]),
          onChange: (value) => patchQuery({ context: value }) }))), body);
    try {
      const source = await api("source", { path: s.path, start: Math.max(1, s.line - context), end: end + context }, "dead-code-source");
      if (generation !== comparisonGeneration) return;
      const start = source.start || Math.max(1, s.line - context);
      const marks = {};
      for (let n = s.line; n <= end; n += 1) marks[n] = "del";
      const own = source.lines.slice(s.line - start, end - start + 1);
      clear(body).append(
        h("div", { class: "row spread", style: { marginBottom: "10px" } },
          h("div", { class: "row" }, h("h2", {}, t("dead.codeTitle")), badge(t(`dead.confidence.${item.confidence}`), item.confidence === "high" ? "danger" : item.confidence === "medium" ? "warning" : "")),
          h("div", { class: "row" },
            h("button", { class: "btn sm", onclick: () => copy(`${s.path}:${s.line}-${end}\n${own.join("\n")}`, t("ai.codeCopied")) }, icon("copy", 14), t("ai.copyCode")),
            h("button", { class: "btn sm", onclick: () => openSource({ path: s.path, line: s.line, language: s.language }) }, icon("external", 14), t("code.openSource")))),
        h("p", { class: "muted small", style: { marginBottom: "8px" } }, t("dead.codeViewOnly")),
        h("div", { class: "compare single" },
          h("div", { class: "pane" }, h("h3", {}, h("span", { class: "mono ellipsis" }, s.path), h("span", { class: "muted" }, `${item.title} · ${item.kind}`)),
            codeBlock({ lines: source.lines, start, language: source.language || s.language, marks, label: `${t("compare.code")}: ${s.path}` }))),
        source.truncated ? note(t("dead.codeTruncated"), "", "info") : null,
        h("div", { class: "card", style: { marginTop: "12px" } }, h("h3", {}, t("dead.unreferenced", { n: end - s.line + 1 })),
          h("p", { class: "muted" }, t("dead.whyText", { name: item.title }))));
    } catch (error) { if (!isAbort(error) && generation === comparisonGeneration) { cancelApi("dead-code-source"); clear(body).append(errorBox(error, () => showMain(item, q()))); } }
  }

  const search = searchBox({ value: initial.get("q") || "", placeholder: t("find.searchPlaceholder"), label: t("find.searchPlaceholder"), onInput: (v) => patchQuery({ q: v }) });
  const layout = h("div", { class: "workbench" }, h("aside", { class: "filters-col" }, filtersHost), h("div", {},
    h("div", { class: "toolbar" }, search, drawerButton(() => drawer.current)),
    chipsHost, tabsHost, results, main), inspector);
  drawer.current = layout.querySelector("aside");
  root.append(pageHead(t(cfg.title), t(cfg.subtitle)), layout);
  const stable = (params) => [...params].filter(([k]) => !["key", "view", "left", "right", "context"].includes(k)).join("&");
  lastQuery = stable(initial);
  selected = initial.get("key");
  await refresh(initial);
  if (selected) showDetail(selected, initial);
  const pairOf = (params) => `${params.get("left")}:${params.get("right")}:${params.get("context")}`;
  let lastPair = pairOf(initial);
  const off = onQuery((params) => {
    search.sync(params.get("q") || "");
    const key = params.get("key");
    const pair = pairOf(params);
    if (key !== selected || viewOf(params) !== view) showDetail(key, params);
    else if (pair !== lastPair && currentItem) showMain(currentItem, params);
    lastPair = pair;
    if (stable(params) !== lastQuery) { lastQuery = stable(params); refresh(params); }
  });
  return () => { off(); selected = null; cancelApi("finding"); cancelApi("findings"); stopComparison(); search.destroy(); };
}
