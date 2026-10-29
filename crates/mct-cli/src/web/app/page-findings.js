// 03 · Duplicados, 05 · Código muerto and 07 · Revisiones share one findings workbench.
import {
  h,
  clear,
  api,
  cancelApi,
  isAbort,
  patchQuery,
  onQuery,
  parseHash,
  listParam,
  fmt,
  ago,
  hrefTo,
  review,
  toast,
  confirmDialog,
  noteDialog,
  copy,
  forget,
  store,
  dateTime,
  basename,
  FINDING_PARAMS,
} from "./core.js";
import {
  icon,
  pageHead,
  tabsBar,
  dataTable,
  pager,
  badge,
  stateBadge,
  STATE_ORDER,
  stateLabel,
  searchBox,
  filterGroup,
  activeChips,
  noResults,
  loading,
  errorBox,
  note,
  drawerButton,
} from "./components.js";
import { openSource } from "./code.js";
import { t } from "./i18n.js";
import { duplicateBrief, deadBrief, listText, memberLabel } from "./ai.js";

// Everything that still needs a person: also findings whose code changed
// after a decision, and resolved findings the detector still reports.
const ACTIONABLE = ["unreviewed", "needs-reevaluation", "in-review", "confirmed", "resolved-but-detected"];
// `stacked`: the findings are blocks under a filter bar, and the other filters wait in a panel.
const CONFIG = {
  duplicados: {
    type: "duplicate",
    title: "dup.title",
    subtitle: "dup.subtitle",
    defaultStates: ACTIONABLE,
    views: true,
    stacked: true,
  },
  "codigo-muerto": {
    type: "dead-code",
    title: "dead.title",
    subtitle: "dead.subtitle",
    defaultStates: ACTIONABLE,
    stacked: true,
  },
  revisiones: { type: "", title: "rev.title", subtitle: "rev.subtitle", defaultStates: null, tabs: true },
};
const BUILTIN_PRESETS = [
  { id: "b-dup", name: "preset.dupPending", query: "type=duplicate&state=unreviewed" },
  { id: "b-dead", name: "preset.deadPending", query: "type=dead-code&state=unreviewed" },
  { id: "b-fp", name: "preset.recentFp", query: "state=false-positive&sort=updated&order=desc" },
  { id: "b-pending", name: "preset.pending", query: "state=unreviewed,in-review" },
];
const UNSUPPORTED_NOTE = { duplicate: "dup.unsupported", "dead-code": "dead.unsupported" };
const CHIP_LABEL = {
  verdict: (verdict) => t(`dup.verdict.${verdict}`),
  scope: (scope) => t(`dup.scope.${scope}`),
  confidence: (confidence) => t(`dead.confidence.${confidence}`),
};
const SCOPES = ["same-file", "cross-file", "cross-module"];
const ZONES = ["source", "tests", "benchmarks", "examples", "website"];
// The bar's orders, as [sort, order]; the first is the default.
const SORTS = {
  duplicate: [
    ["rank", "asc"],
    ["number", "asc"],
    ["title", "asc"],
    ["memberCount", "desc"],
    ["lines", "desc"],
    ["state", "asc"],
  ],
  "dead-code": [
    ["rank", "asc"],
    ["number", "asc"],
    ["title", "asc"],
    ["path", "asc"],
    ["state", "asc"],
  ],
};
// A block's review state: an icon and a tone, so it reads without colour too.
const STATE_PILL = {
  unreviewed: ["circle", ""],
  "needs-reevaluation": ["refresh", "warning"],
  "in-review": ["clock", "warning"],
  confirmed: ["checkCircle", "success"],
  "resolved-but-detected": ["alert", "warning"],
  "false-positive": ["unused", ""],
  resolved: ["checkCircle", "info"],
  dismissed: ["minusCircle", "muted"],
};
const CONFIDENCE_TONE = { high: "danger", medium: "warning" };
const confidenceBadge = (confidence) =>
  badge(t(`dead.confidence.${confidence}`), CONFIDENCE_TONE[confidence] || "");

/** Saves a decision (null reopens) and reports it; a 409 hands `onChange` the review someone else saved. */
async function saveDecision(item, state, onChange, message) {
  try {
    const saved = await review(item.key, state, item.note || "", item.revision);
    toast(message || t("review.saved", { state: stateLabel(saved.state, item.type) }), "ok");
    onChange(saved);
  } catch (error) {
    if (error.status === 409 && error.body?.key) {
      toast(t("review.conflict"), "err");
      onChange(error.body);
    } else toast(t("review.failed", { message: error.message }), "err");
  }
}

/** One-click ignore: no dialog and no reason. The finding stays in the history and can be reopened. */
export function ignoreFinding(item, onChange) {
  return saveDecision(item, "dismissed", onChange, t("review.ignored"));
}

// Any decision can replace any other, as in the review panel; "reopen" clears the stored one.
const DECISIONS = ["in-review", "confirmed", "false-positive", "dismissed", "resolved"];
const DECIDED = ["confirmed", "false-positive", "resolved", "dismissed"];

async function changeState(item, target, onChange) {
  if (
    target === "false-positive" &&
    !(await confirmDialog({
      title: t("review.fpTitle"),
      body: t("review.fpBody"),
      confirm: t("review.markFp"),
    }))
  )
    return;
  if (
    DECIDED.includes(item.state) &&
    target !== "dismissed" &&
    target !== "false-positive" &&
    !(await confirmDialog({
      title: t("review.changeTitle"),
      body: t("review.changeBody", {
        from: stateLabel(item.state, item.type),
        to: target === "reopen" ? t("state.unreviewed") : stateLabel(target, item.type),
      }),
      confirm: t("review.changeConfirm"),
    }))
  )
    return;
  await saveDecision(item, target === "reopen" ? null : target, onChange);
}

/**
 * The state of a finding as a button: it opens the other decisions and saves the pick. A duplicate
 * has one state for the whole group, however many files it has, and the menu says so.
 */
function stateMenu(item, face, onChange) {
  if (item.readOnly) return face();
  const targets = [
    ...DECISIONS.filter((s) => s !== item.state),
    ...(item.state === "unreviewed" ? [] : ["reopen"]),
  ];
  const group = item.type === "duplicate" && item.memberCount > 1;
  let menu = null;
  const close = () => {
    if (!menu) return;
    menu.remove();
    menu = null;
    button.setAttribute("aria-expanded", "false");
    document.removeEventListener("click", away, true);
    document.removeEventListener("keydown", keys);
    window.removeEventListener("resize", close);
    window.removeEventListener("scroll", close, true);
  };
  const away = (event) => {
    if (!menu?.contains(event.target) && !button.contains(event.target)) close();
  };
  const keys = (event) => {
    if (event.key === "Escape") {
      close();
      button.focus();
    } else if (event.key === "Tab") close();
    else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const items = [...menu.querySelectorAll("[role=menuitem]")];
      const step = event.key === "ArrowDown" ? 1 : -1;
      items[(items.indexOf(document.activeElement) + step + items.length) % items.length].focus();
    }
  };
  const open = () => {
    const box = button.getBoundingClientRect();
    menu = h(
      "div",
      {
        class: "pop state-menu",
        role: "menu",
        "aria-label": t("review.stateLabel"),
        style: { position: "fixed", top: `${box.bottom + 4}px`, left: `${box.left}px` },
      },
      group && h("p", { class: "state-menu-note" }, t("review.groupScope", { n: item.memberCount })),
      targets.map((target) =>
        h(
          "button",
          {
            class: "choice-item",
            role: "menuitem",
            onclick: () => {
              close();
              changeState(item, target, onChange);
            },
          },
          icon(target === "reopen" ? "refresh" : STATE_PILL[target][0], 15),
          h("span", {}, target === "reopen" ? t("review.reopenOption") : stateLabel(target, item.type)),
        ),
      ),
    );
    document.body.append(menu);
    // Near the bottom edge it opens upwards, and never past the right edge.
    const size = menu.getBoundingClientRect();
    if (size.bottom > window.innerHeight) menu.style.top = `${Math.max(8, box.top - size.height - 4)}px`;
    menu.style.left = `${Math.max(8, Math.min(box.left, window.innerWidth - size.width - 8))}px`;
    button.setAttribute("aria-expanded", "true");
    menu.querySelector("[role=menuitem]").focus();
    document.addEventListener("click", away, true);
    document.addEventListener("keydown", keys);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
  };
  const button = h(
    "button",
    {
      class: "state-btn",
      "aria-haspopup": "menu",
      "aria-expanded": "false",
      "aria-label": t("review.changeState", { title: item.title, state: stateLabel(item.state, item.type) }),
      title: group ? t("review.groupScope", { n: item.memberCount }) : t("review.stateLabel"),
      onclick: () => (menu ? close() : open()),
    },
    face(),
    icon("down", 12),
  );
  return button;
}

/** Readable number shared by the list and the detail: "Duplicado #3". */
export const findingNumber = (item) =>
  item.number
    ? t(`find.number.${item.type}`, { n: item.number })
    : t(item.type === "duplicate" ? "common.duplicate" : "common.deadCode");

// Links name a finding by its number when it has one; the key still opens older links.
export const findingHref = (item) =>
  hrefTo(
    item.type === "duplicate" ? "duplicados" : "codigo-muerto",
    item.number ? { n: item.number } : { key: item.key },
  );

const verdictBadge = (verdict) =>
  badge(t(`dup.verdict.${verdict}`), verdict === "real" ? "success" : "warning");

const statePill = (state) => {
  const [name, tone] = STATE_PILL[state] || ["circle", ""];
  return h("span", { class: `pill ${tone}` }, icon(name, 15), stateLabel(state));
};

/** A labelled select for the filter bar; `set` redraws its options in place, so focus stays. */
function pick(label, onChange) {
  const select = h("select", { onchange: () => onChange(select.value) });
  const box = h("label", { class: "pick" }, h("span", {}, label), select, icon("down", 14));
  box.set = (options, value) => {
    clear(select).append(options.map((o) => h("option", { value: o.value, disabled: o.disabled }, o.label)));
    select.value = value;
  };
  return box;
}

// The review workspace of an open finding: a React bundle, loaded the first time a finding opens.
const loadReview = () => import("./review.js");
function reviewStyles() {
  if (!document.querySelector("link[data-review-css]"))
    document.head.append(h("link", { rel: "stylesheet", href: "review.css", "data-review-css": "" }));
}

export async function render({ root, page, params: initial }) {
  const cfg = CONFIG[page];
  const results = h("div", {});
  const filtersHost = h("div", {});
  const tabsHost = h("div", {});
  const chipsHost = h("div", {});
  // Where review.js draws the open finding, in place of the list.
  const host = h("div", { class: "review-host", hidden: true });
  const drawer = { current: null };
  let facets = {};
  // `selected` is the finding the URL names ("#3" or a key); `selectedKey` its key once loaded.
  let selected = null;
  let selectedKey = null;
  let lastQuery = "";
  let currentItem = null;
  // The mounted workspace ({ update, unmount }); `closed` once the page is left.
  let workspace = null;
  let closed = false;
  // A finding opens by its number: `n=3` on Duplicados and Código muerto, `n=duplicate-3` on
  // Revisiones, which lists both kinds.
  const openParams = (r) => {
    if (!r.number) return { key: r.key, n: null };
    return { n: cfg.type ? r.number : `${r.type}-${r.number}`, key: null };
  };
  // The workspace lives on Duplicados and Código muerto only: Revisiones opens a finding there.
  const open = (r, params = {}) => {
    if (cfg.type) patchQuery({ ...openParams(r), ...params });
    else location.hash = findingHref(r);
  };
  const q = () => parseHash().params;
  // Filters shown as chips; on Duplicados the scope is the bar's "Archivos" instead.
  const panelKeys = ["language", "kind", "verdict", "scope", "confidence"].filter(
    (key) => !(cfg.stacked && key === "scope"),
  );

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
    type: cfg.type || params.get("type") || "",
    state: effectiveStates(params),
    language: listParam(params, "language"),
    kind: listParam(params, "kind"),
    verdict: listParam(params, "verdict"),
    scope: listParam(params, "scope"),
    zone: effectiveZones(params),
    confidence: listParam(params, "confidence"),
    q: params.get("q") || "",
    dir: params.get("dir") || "",
    sort: params.get("sort") || "rank",
    order: params.get("order") || "asc",
    page: Number(params.get("page") || 0),
    size: 25,
  });
  const clearAll = () => {
    forget(page);
    patchQuery({
      state: cfg.defaultStates ? "all" : null,
      language: null,
      kind: null,
      verdict: null,
      scope: null,
      zone: cfg.type === "dead-code" ? "all" : null,
      confidence: null,
      q: null,
      dir: null,
      type: null,
      sort: null,
      order: null,
    });
  };

  const opts = (name, labeler = (v) => v) =>
    (facets[name] || []).map(([value, count]) => ({ value, label: labeler(value), count }));

  // ----- presets (Revisiones) -----
  const presets = () => store.get("presets", []);
  const presetBox = () => {
    const mine = presets();
    const applyPreset = (query) => {
      forget(page);
      history.replaceState(null, "", `#/${page}?${query}`);
      patchQuery({ page: null });
    };
    return h(
      "details",
      { class: "fgroup", open: true },
      h("summary", {}, t("preset.title")),
      [...BUILTIN_PRESETS.map((p) => ({ ...p, name: t(p.name), builtin: true })), ...mine].map((p) =>
        h(
          "div",
          { class: "row spread py-1" },
          h("button", { class: "btn ghost sm ellipsis", onclick: () => applyPreset(p.query) }, p.name),
          !p.builtin &&
            h(
              "span",
              { class: "row gap-1" },
              h(
                "button",
                {
                  class: "icon-btn sm",

                  "aria-label": t("preset.rename", { name: p.name }),
                  onclick: async () => {
                    const name = await noteDialog({
                      title: t("preset.renameTitle"),
                      required: true,
                      initial: p.name,
                    });
                    if (name) {
                      store.set(
                        "presets",
                        presets().map((x) => (x.id === p.id ? { ...x, name } : x)),
                      );
                      drawFilters(q());
                    }
                  },
                },
                icon("edit", 14),
              ),
              h(
                "button",
                {
                  class: "icon-btn sm",

                  "aria-label": t("preset.delete", { name: p.name }),
                  onclick: async () => {
                    if (
                      await confirmDialog({
                        title: t("preset.deleteTitle"),
                        body: p.name,
                        confirm: t("common.delete"),
                        danger: true,
                      })
                    ) {
                      store.set(
                        "presets",
                        presets().filter((x) => x.id !== p.id),
                      );
                      drawFilters(q());
                    }
                  },
                },
                icon("trash", 14),
              ),
            ),
        ),
      ),
      h(
        "button",
        {
          class: "btn sm mt-2",

          onclick: async () => {
            const name = await noteDialog({
              title: t("preset.saveTitle"),
              body: t("preset.saveBody"),
              required: true,
              confirm: t("common.save"),
            });
            if (!name) return;
            store.set("presets", [...presets(), { id: `p${Date.now()}`, name, query: q().toString() }]);
            toast(t("preset.saved"), "ok");
            drawFilters(q());
          },
        },
        icon("plus", 14),
        t("preset.save"),
      ),
    );
  };

  function drawFilters(params) {
    const states = effectiveStates(params);
    const groups = [
      {
        id: "state",
        label: t("filters.state"),
        options: STATE_ORDER.map((s) => ({
          value: s,
          label: stateLabel(s),
          count: (facets.state || []).find(([v]) => v === s)?.[1] || 0,
        })),
        selected: states,
        onChange: (v) => patchQuery({ state: v.length ? v : "all" }),
      },
      cfg.type === "duplicate" && {
        id: "verdict",
        label: t("filters.verdict"),
        options: opts("verdict", (v) => t(`dup.verdict.${v}`)),
        selected: listParam(params, "verdict"),
        onChange: (v) => patchQuery({ verdict: v }),
      },
      cfg.type === "duplicate" && {
        id: "scope",
        label: t("filters.scope"),
        options: opts("scope", (v) => t(`dup.scope.${v}`)),
        selected: listParam(params, "scope"),
        onChange: (v) => patchQuery({ scope: v }),
      },
      cfg.type === "dead-code" && {
        id: "confidence",
        label: t("filters.confidence"),
        options: ["high", "medium", "low"].map((v) => ({
          value: v,
          label: t(`dead.confidence.${v}`),
          count: (facets.confidence || []).find(([x]) => x === v)?.[1] || 0,
        })),
        selected: listParam(params, "confidence"),
        onChange: (v) => patchQuery({ confidence: v }),
      },
      cfg.type === "dead-code" && {
        id: "zone",
        label: t("filters.zone"),
        options: ZONES.map((v) => ({
          value: v,
          label: t(`dead.zone.${v}`),
          count: (facets.zone || []).find(([x]) => x === v)?.[1] || 0,
        })),
        selected: effectiveZones(params),
        onChange: (v) => patchQuery({ zone: v.length ? v : "all" }),
      },
      {
        id: "language",
        label: t("filters.language"),
        options: opts("language"),
        selected: listParam(params, "language"),
        onChange: (v) => patchQuery({ language: v }),
      },
      {
        id: "kind",
        label: t("filters.symbolKind"),
        options: opts("kind"),
        selected: listParam(params, "kind"),
        onChange: (v) => patchQuery({ kind: v }),
        open: false,
        search: true,
      },
      !cfg.type && {
        id: "type",
        label: t("filters.type"),
        options: opts("type", (v) => t(v === "duplicate" ? "common.duplicate" : "common.deadCode")),
        selected: listParam(params, "type"),
        onChange: (v) => patchQuery({ type: v.length === 1 ? v[0] : null }),
      },
    ].filter(Boolean);
    clear(filtersHost).append(
      h(
        "div",
        { class: "card filters" },
        h(
          "div",
          { class: "head" },
          h("h2", {}, t("filters.title")),
          h(
            "div",
            { class: "row" },
            h("button", { class: "btn ghost sm", onclick: clearAll }, t("filters.clear")),
            h(
              "button",
              {
                class: "icon-btn only-mobile",
                "aria-label": t("common.close"),
                onclick: () => {
                  drawer.current?.classList.remove("open");
                  if (cfg.stacked) moreButton.focus();
                },
              },
              icon("x"),
            ),
          ),
        ),
        groups.map((g) => filterGroup(g)),
        cfg.tabs ? presetBox() : null,
        h("p", { class: "logic" }, t("filters.logic")),
        note(t(UNSUPPORTED_NOTE[cfg.type] || "rev.unsupported"), "", "info"),
      ),
    );
  }

  function drawTabs(params) {
    if (!cfg.tabs) return;
    const states = effectiveStates(params);
    const count = (s) => (facets.state || []).find(([v]) => v === s)?.[1] || 0;
    const total = (facets.state || []).reduce((n, [, c]) => n + c, 0);
    // One tab per state: several selected states match no single tab, so none is active.
    let active = "";
    if (states.length === 0) active = "all";
    if (states.length === 1) active = states[0];
    clear(tabsHost).append(
      tabsBar({
        label: t("rev.tabs"),
        active,
        onSelect: (id) => patchQuery({ state: id === "all" ? null : id }),
        tabs: [
          { id: "all", label: t("rev.tab.all"), count: total },
          ...STATE_ORDER.map((s) => ({ id: s, label: stateLabel(s), count: count(s) })),
        ],
      }),
    );
  }

  // The state, the zone and the search a chip names; on a stacked page the bar shows them instead.
  function barChips(params, chips) {
    const states = effectiveStates(params);
    if (cfg.defaultStates && params.get("state") !== "all" && !params.get("state"))
      chips.push({
        label: t("filters.hiding", {
          states: STATE_ORDER.filter((s) => !cfg.defaultStates.includes(s))
            .map(stateLabel)
            .join(", "),
        }),
        remove: () => patchQuery({ state: "all" }),
      });
    else if (!cfg.tabs)
      for (const s of states)
        chips.push({
          label: stateLabel(s),
          remove: () => patchQuery({ state: states.filter((x) => x !== s).join(",") || "all" }),
        });
    if (cfg.type === "dead-code" && !params.get("zone"))
      chips.push({ label: t("filters.zoneOnly"), remove: () => patchQuery({ zone: "all" }) });
    else if (cfg.type === "dead-code" && params.get("zone") !== "all")
      for (const v of effectiveZones(params))
        chips.push({
          label: t(`dead.zone.${v}`),
          remove: () =>
            patchQuery({
              zone:
                effectiveZones(params)
                  .filter((x) => x !== v)
                  .join(",") || "all",
            }),
        });
    if (params.get("q")) chips.push({ label: `“${params.get("q")}”`, remove: () => patchQuery({ q: null }) });
  }

  function drawChips(params) {
    const chips = [];
    if (!cfg.stacked) barChips(params, chips);
    for (const key of panelKeys)
      for (const v of listParam(params, key))
        chips.push({
          label: (CHIP_LABEL[key] || String)(v),
          remove: () => patchQuery({ [key]: listParam(params, key).filter((x) => x !== v) }),
        });
    if (params.get("dir")) chips.push({ label: params.get("dir"), remove: () => patchQuery({ dir: null }) });
    clear(chipsHost).append(activeChips(chips, clearAll) || "");
  }

  const placeOf = (r) =>
    r.type === "duplicate"
      ? r.members
          .map((m) => `${m.path}:${m.start}`)
          .slice(0, 2)
          .join("  ↔  ")
      : `${r.path}:${r.line}`;
  function columns() {
    const common = {
      state: {
        key: "state",
        label: t("col.state"),
        sortable: true,
        render: (r) =>
          stateMenu(
            r,
            () => stateBadge(r.state, r.type),
            () => refresh(),
          ),
      },
      updated: {
        key: "updated",
        label: t("col.updated"),
        sortable: true,
        render: (r) => (r.updated ? ago(r.updated) : "—"),
        cls: "hide-sm",
      },
      actions: {
        key: "actions",
        label: t("col.actions"),
        cls: "sticky-end",
        render: (r) =>
          h(
            "span",
            { class: "row gap-1" },
            r.type === "duplicate" &&
              h(
                "button",
                {
                  class: "btn sm",
                  "aria-label": `${t("dup.compare")}: ${r.title}`,
                  onclick: () => open(r, { file: null }),
                },
                icon("split", 14),
                t("dup.compareShort"),
              ),
            r.type === "dead-code" &&
              h(
                "button",
                {
                  class: "btn sm",
                  "aria-label": `${t("code.viewSource")}: ${r.title}`,
                  onclick: () => open(r, { context: null }),
                },
                icon("code", 14),
                t("code.view"),
              ),
            !r.readOnly &&
              r.state !== "dismissed" &&
              h(
                "button",
                {
                  class: "btn sm",
                  "aria-label": `${t("review.ignoreNow")}: ${r.title}`,
                  onclick: () => ignoreFinding(r, () => refresh()),
                },
                icon("x", 14),
                t("review.ignoreNow"),
              ),
          ),
      },
      // Readable and stable: the same number in the list, the detail and the URL.
      number: {
        key: "number",
        label: t("col.number"),
        sortable: true,
        render: (r) => h("span", { class: "finding-no" }, r.number ? `#${r.number}` : "—"),
      },
    };
    // Compact rows: how many files a group touches, with every fragment in the cell's tooltip.
    if (cfg.type === "duplicate")
      return [
        common.number,
        {
          key: "title",
          label: t("col.symbols"),
          sortable: true,
          render: (r) =>
            h(
              "span",
              { class: "name" },
              h("span", { class: "ellipsis" }, r.title),
              r.memberCount > 2 && badge(`+${r.memberCount - 2}`),
            ),
        },
        {
          key: "path",
          label: t("col.affectedFiles"),
          sortable: true,
          render: (r) =>
            h(
              "span",
              { title: r.members.map(memberLabel).join("\n") },
              h("span", {}, t("dup.filesCount", { n: new Set(r.members.map((m) => m.path)).size })),
              h("span", { class: "path" }, r.members[0] ? r.members[0].path : ""),
            ),
          cls: "hide-sm",
        },
        {
          key: "lines",
          label: t("col.lines"),
          sortable: true,
          num: true,
          render: (r) => r.lines,
        },
        {
          key: "verdict",
          label: t("col.verdict"),
          sortable: true,
          render: (r) => verdictBadge(r.verdict),
          cls: "hide-sm",
        },
        common.state,
        common.actions,
      ];
    if (cfg.type === "dead-code")
      return [
        common.number,
        {
          key: "title",
          label: t("col.symbol"),
          sortable: true,
          render: (r) => h("span", { class: "name" }, h("span", { class: "ellipsis" }, r.title)),
        },
        { key: "kind", label: t("code.col.kind"), sortable: true, render: (r) => r.kind, cls: "hide-sm" },
        {
          key: "path",
          label: t("col.file"),
          sortable: true,
          render: (r) => h("span", { class: "path", title: r.path }, `${r.path}:${r.line}`),
        },
        {
          key: "zone",
          label: t("col.zone"),
          sortable: true,
          render: (r) => h("span", { class: "nowrap" }, t(`dead.zone.${r.zone}`)),
          cls: "hide-sm",
        },
        {
          key: "confidence",
          label: t("col.confidence"),
          sortable: true,
          render: (r) => confidenceBadge(r.confidence),
          cls: "hide-sm",
        },
        common.state,
        common.actions,
      ];
    return [
      common.number,
      {
        key: "title",
        label: t("col.finding"),
        sortable: true,
        render: (r) => h("span", { class: "name" }, h("span", { class: "ellipsis" }, r.title)),
      },
      {
        key: "type",
        label: t("col.type"),
        sortable: true,
        render: (r) =>
          badge(
            t(r.type === "duplicate" ? "common.duplicate" : "common.deadCode"),
            r.type === "duplicate" ? "warning" : "danger",
          ),
      },
      {
        key: "path",
        label: t("col.location"),
        sortable: true,
        render: (r) => h("span", { class: "path", title: placeOf(r) }, placeOf(r)),
        cls: "hide-sm",
      },
      common.state,
      common.updated,
      common.actions,
    ];
  }

  // ----- stacked pages: the filter bar and the blocks -----
  /** The bar's selects follow the URL; a combination only the panel can make reads "Varios". */
  function drawBar(params) {
    const count = (name, value) => (facets[name] || []).find(([v]) => v === value)?.[1] || 0;
    const counted = (label, n) => `${label} (${fmt(n)})`;
    const several = (n) => ({ value: "several", label: t("list.several", { n }), disabled: true });
    const valueOf = (raw, values, fallback) => {
      if (!raw) return fallback;
      if (raw === "all") return "all";
      return values.length === 1 ? values[0] : "several";
    };

    const states = effectiveStates(params);
    const state = valueOf(params.get("state"), states, "pending");
    stateSelect.set(
      [
        { value: "pending", label: t("list.pending") },
        { value: "all", label: t("list.all") },
        ...STATE_ORDER.map((s) => ({ value: s, label: counted(stateLabel(s), count("state", s)) })),
        state === "several" && several(states.length),
      ].filter(Boolean),
      state,
    );

    // "Archivos": how far a duplicate reaches, or which part of the project a candidate is in.
    const [key, values, fallback, chosen] =
      cfg.type === "duplicate"
        ? ["scope", SCOPES, "all", listParam(params, "scope")]
        : ["zone", ZONES, "source", effectiveZones(params)];
    const files = valueOf(params.get(key), chosen, fallback);
    filesSelect.set(
      [
        { value: "all", label: t("list.all") },
        ...values.map((v) => ({
          value: v,
          label: counted(t(cfg.type === "duplicate" ? `dup.scope.${v}` : `dead.zone.${v}`), count(key, v)),
        })),
        files === "several" && several(chosen.length),
      ].filter(Boolean),
      files,
    );

    const sorts = SORTS[cfg.type];
    const sort = `${params.get("sort") || "rank"}:${params.get("order") || "asc"}`;
    sortSelect.set(
      [
        ...sorts.map(([field, order]) => ({ value: `${field}:${order}`, label: t(`list.sort.${field}`) })),
        // An older link may sort by a column the bar does not offer.
        !sorts.some(([field, order]) => `${field}:${order}` === sort) && {
          value: sort,
          label: t("list.sort.other"),
          disabled: true,
        },
      ].filter(Boolean),
      sort,
    );

    const active = panelKeys.reduce((n, key) => n + listParam(params, key).length, 0);
    clear(moreButton).append(
      icon("filter", 16),
      active ? t("list.moreFiltersN", { n: active }) : t("list.moreFilters"),
    );
  }

  function stackRows(rows) {
    return h(
      "ul",
      { class: "stack-rows", "aria-label": t(cfg.title) },
      rows.map((r) =>
        h(
          "li",
          { class: "stack-row", "data-key": r.key },
          h(
            "button",
            {
              class: "stack-open",
              title: r.type === "duplicate" ? r.members.map(memberLabel).join("\n") : `${r.path}:${r.line}`,
              onclick: () => open(r),
            },
            h("span", { class: "stack-no" }, r.number ? `#${r.number}` : "—"),
            h("span", { class: "stack-title" }, r.title),
            h(
              "span",
              { class: "stack-where" },
              // A candidate names its file, whose full path is in the block's tooltip.
              r.type === "duplicate"
                ? t("dup.filesCount", { n: new Set(r.members.map((m) => m.path)).size })
                : `${basename(r.path)}:${r.line}`,
            ),
          ),
          h(
            "span",
            { class: "stack-state", title: stateLabel(r.state, r.type) },
            stateMenu(
              r,
              () => statePill(r.state),
              () => refresh(),
            ),
          ),
          !r.readOnly && r.state !== "dismissed"
            ? h(
                "button",
                {
                  class: "icon-btn sm",
                  "aria-label": `${t("review.ignoreNow")}: ${r.title}`,
                  title: t("review.ignoreNow"),
                  onclick: () => ignoreFinding(r, () => refresh()),
                },
                icon("x", 14),
              )
            : h("span", {}),
        ),
      ),
    );
  }

  async function refresh(params = q()) {
    drawChips(params);
    const query = queryFor(params);
    clear(results).append(loading());
    try {
      const data = await api("findings", query, "findings");
      facets = data.facets;
      drawTabs(params);
      drawFilters(params);
      if (cfg.stacked) drawBar(params);
      clear(results);
      if (!data.total) {
        results.append(
          noResults({
            onClear: clearAll,
            extra: [
              h("button", { class: "btn", onclick: () => patchQuery({ q: null }) }, t("filters.editSearch")),
            ],
          }),
        );
        return;
      }
      // Rows start with the readable number; the keys still close the text for queries.
      const listColumns =
        cfg.type === "duplicate"
          ? [findingNumber, (r) => r.title, (r) => r.members.map(memberLabel).join(" ↔ ")]
          : [
              findingNumber,
              (r) => r.title,
              (r) => r.type || cfg.type,
              (r) => `${r.path || ""}${r.line ? `:${r.line}` : ""}`,
            ];
      results.append(
        h(
          "div",
          { class: "row spread m-0 mb-2" },
          cfg.type === "duplicate"
            ? h("p", { class: "muted m-0" }, t("dup.found", { n: fmt(data.total) }))
            : h("span", {}),
          h(
            "button",
            {
              class: "btn sm",
              onclick: () =>
                copy(
                  listText({
                    title: t(cfg.title),
                    total: data.total,
                    columns: listColumns,
                    rows: data.items,
                    idKey: "key",
                  }),
                  t("ai.listCopied"),
                ),
            },
            icon("copy", 14),
            t("ai.copyList"),
          ),
        ),
      );
      results.append(
        cfg.stacked
          ? stackRows(data.items)
          : dataTable({
              caption: t(cfg.title),
              rows: data.items,
              rowKey: (r) => r.key,
              selected: selectedKey,
              sort: query.sort,
              order: query.order,
              onSort: (sort, order) => patchQuery({ sort, order }),
              onSelect: (row) => open(row),
              columns: columns(),
            }),
        pager({
          page: data.page,
          size: data.size,
          total: data.total,
          onPage: (page) => patchQuery({ page }),
        }),
      );
    } catch (error) {
      if (!isAbort(error)) clear(results).append(errorBox(error, () => refresh(params)));
    }
  }

  // ----- the open finding: the React review workspace (review.js) -----
  // The open finding is `n` (its number, see openParams) or `key` (older links).
  const targetOf = (params) => params.get("key") || (params.get("n") ? `#${params.get("n")}` : null);
  const lookup = (target) => {
    if (!target.startsWith("#")) return { key: target };
    const typed = /^(duplicate|dead-code)-(\d+)$/.exec(target.slice(1));
    return typed ? { type: typed[1], number: typed[2] } : { type: cfg.type, number: target.slice(1) };
  };
  const markRows = () => {
    root
      .querySelectorAll("tbody tr")
      .forEach((row) => row.setAttribute("aria-selected", String(row.dataset.key === selectedKey)));
    root
      .querySelectorAll(".stack-row")
      .forEach((row) => row.classList.toggle("current", row.dataset.key === selectedKey));
  };
  // The file a duplicate shows first; `left` is what older comparison links used.
  const fileIndex = (params, item) => {
    const index = Number(params.get("file") ?? params.get("left") ?? 0);
    return Number.isInteger(index) && index >= 0 && index < (item.members?.length || 0) ? index : 0;
  };
  // Lines around a dead-code candidate; the workspace falls back to 3 for any other value.
  const contextOf = (params) => (params.get("context") === null ? 3 : Number(params.get("context")));
  // `view` is what older links used to pick the comparison or the code; it is cleared, not read.
  const closeDetail = () =>
    patchQuery({ key: null, n: null, view: null, file: null, left: null, right: null, context: null });

  // What the workspace needs from this page: translations, the API and the shared dialogs.
  const bridge = {
    t,
    stateLabel,
    ago,
    dateTime,
    source: (params, key) => api("source", params, key),
    cancel: cancelApi,
    save: review,
    openSource,
    confirm: confirmDialog,
    aiBrief: (item) => (item.type === "duplicate" ? duplicateBrief(item) : deadBrief(item)),
    noteHref: (path) => hrefTo("obsidian", { note: path }),
  };
  const workspaceProps = (item, params) => ({
    item,
    file: fileIndex(params, item),
    context: contextOf(params),
    bridge,
    onBack: closeDetail,
    onSelectFile: (index) => patchQuery({ file: index }),
    onContext: (lines) => patchQuery({ context: lines }),
    // The decision is shown at once; the list and the full finding (history, revision) follow.
    onSaved: (saved) => {
      currentItem = { ...currentItem, ...saved };
      drawWorkspace();
      refresh(q());
      reload(selected);
    },
  });

  // An open finding works like an editor: the shell's sidebar folds to its icons, and unfolds
  // on the way back unless it was already folded. The person's stored choice is not touched.
  let folded = false;
  function fold(on) {
    const shell = root.closest(".shell");
    if (on && shell && !shell.classList.contains("collapsed")) {
      shell.classList.add("collapsed");
      folded = true;
    } else if (!on && folded) {
      shell?.classList.remove("collapsed");
      folded = false;
    }
  }

  function hideWorkspace() {
    workspace?.unmount();
    workspace = null;
    clear(host).hidden = true;
    layout.hidden = false;
    head.hidden = false;
    fold(false);
  }

  /** Mounts review.js for `currentItem`, or hands the mounted one the current URL state. */
  async function drawWorkspace() {
    if (!currentItem) return;
    if (workspace) {
      workspace.update(workspaceProps(currentItem, q()));
      return;
    }
    reviewStyles();
    let module;
    try {
      module = await loadReview();
    } catch (error) {
      if (currentItem) clear(host).append(backButton(), errorBox(error, drawWorkspace));
      return;
    }
    if (!currentItem || closed) return;
    if (workspace) workspace.update(workspaceProps(currentItem, q()));
    else workspace = module.mountReview(clear(host), workspaceProps(currentItem, q()));
  }

  const backButton = () =>
    h("button", { class: "btn mb-2", onclick: closeDetail }, icon("left", 14), t("dup.backToList"));

  async function showDetail(target) {
    selected = target;
    currentItem = null;
    cancelApi("finding");
    workspace?.unmount();
    workspace = null;
    if (!target) {
      selectedKey = null;
      markRows();
      hideWorkspace();
      return;
    }
    // Older Revisiones links (`n=duplicate-3`, `key=…`) move to the finding's own page, in place;
    // Revisiones forgets the finding, so its menu link opens the list again.
    if (!cfg.type) {
      try {
        const item = await api("finding", lookup(target), "finding");
        if (selected !== target || closed) return;
        closeDetail();
        location.replace(findingHref(item));
      } catch (error) {
        if (!isAbort(error) && selected === target) toast(error.message, "err");
      }
      return;
    }
    // The list gives way to the workspace, whose bottom bar names the finding; Back brings the
    // list back as it was.
    layout.hidden = true;
    head.hidden = true;
    host.hidden = false;
    fold(true);
    clear(host).append(loading());
    try {
      const item = await api("finding", lookup(target), "finding");
      if (selected !== target) return;
      currentItem = item;
      selectedKey = item.key;
      markRows();
      await drawWorkspace();
    } catch (error) {
      if (!isAbort(error) && selected === target)
        clear(host).append(
          backButton(),
          errorBox(error, () => showDetail(target)),
        );
    }
  }

  /** Fetches the open finding again after a decision, keeping the workspace mounted. */
  async function reload(target) {
    try {
      const item = await api("finding", lookup(target), "finding");
      if (selected !== target || !currentItem) return;
      currentItem = item;
      drawWorkspace();
    } catch (error) {
      if (!isAbort(error)) toast(t("review.reloadFailed", { message: error.message }), "err");
    }
  }

  const search = searchBox({
    value: initial.get("q") || "",
    placeholder: t("find.searchPlaceholder"),
    label: t("find.searchPlaceholder"),
    onInput: (v) => patchQuery({ q: v }),
  });
  // Stacked pages: the bar's selects (filled by drawBar) and the button that opens the panel.
  const stateSelect = pick(t("list.state"), (value) =>
    patchQuery({ state: value === "pending" ? null : value }),
  );
  const filesSelect = pick(t("list.files"), (value) =>
    cfg.type === "duplicate"
      ? patchQuery({ scope: value === "all" ? null : value })
      : patchQuery({ zone: value === "source" ? null : value }),
  );
  const sortSelect = pick(t("list.sort"), (value) => {
    const [sort, order] = value.split(":");
    patchQuery(sort === "rank" ? { sort: null, order: null } : { sort, order });
  });
  const openPanel = () => {
    drawer.current?.classList.add("open");
    drawer.current?.querySelector("button")?.focus();
  };
  const moreButton = h("button", { class: "btn", "aria-haspopup": "true", onclick: openPanel });
  const listArea = cfg.stacked
    ? h(
        "section",
        { class: "card stack-list", "aria-label": t(cfg.title) },
        h(
          "div",
          { class: "toolbar stack-bar", role: "group", "aria-label": t("filters.title") },
          h("span", { class: "stack-bar-label", "aria-hidden": "true" }, t("filters.title")),
          stateSelect,
          filesSelect,
          search,
          sortSelect,
          moreButton,
        ),
        chipsHost,
        results,
      )
    : h(
        "div",
        {},
        h(
          "div",
          { class: "toolbar" },
          search,
          drawerButton(() => drawer.current),
        ),
        chipsHost,
        tabsHost,
        results,
      );
  const layout = h(
    "div",
    { class: cfg.stacked ? "workbench nofilter stacked" : "workbench" },
    h("aside", { class: "filters-col" }, filtersHost),
    listArea,
  );
  drawer.current = layout.querySelector("aside");
  // The panel closes with Escape too, and gives focus back to the button that opened it.
  drawer.current.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !drawer.current.classList.contains("open")) return;
    drawer.current.classList.remove("open");
    if (cfg.stacked) moreButton.focus();
  });
  const head = pageHead(t(cfg.title), t(cfg.subtitle));
  root.append(head, layout, host);
  const stable = (params) => [...params].filter(([k]) => !FINDING_PARAMS.includes(k)).join("&");
  lastQuery = stable(initial);
  selected = targetOf(initial);
  await refresh(initial);
  if (selected) showDetail(selected);
  const off = onQuery((params) => {
    search.sync(params.get("q") || "");
    const target = targetOf(params);
    if (target !== selected) showDetail(target);
    // Same finding, another file or context: the mounted workspace keeps its drafts and code.
    else if (currentItem) drawWorkspace();
    if (stable(params) !== lastQuery) {
      lastQuery = stable(params);
      refresh(params);
    }
  });
  return () => {
    off();
    closed = true;
    selected = null;
    currentItem = null;
    cancelApi("finding");
    cancelApi("findings");
    workspace?.unmount();
    workspace = null;
    fold(false);
    search.destroy();
  };
}
