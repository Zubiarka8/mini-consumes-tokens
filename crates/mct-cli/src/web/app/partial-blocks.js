// Duplicados · repeated blocks inside different functions (`mct-cli duplicates --partial-blocks`).
// The search is costly, so it runs only when asked: the server searches on its own thread and keeps
// the result with its snapshot; this panel starts it, polls until it is done and pages what it kept.
import { h, clear, api, cancelApi, isAbort, startPartialBlocks, fmt, duration } from "./core.js";
import { loading, errorBox, note, pager, emptyState, icon } from "./components.js";
import { openSource } from "./code.js";
import { t } from "./i18n.js";

// The first check comes soon, since a small project is searched in milliseconds; later ones are spaced.
const FIRST_POLL_MS = 150;
const POLL_MS = 700;
const KEY = "partial-blocks";

const occurrenceRow = (occurrence, language) =>
  h(
    "li",
    {},
    h(
      "button",
      {
        class: "pblock-open",
        title: t("pblocks.openContext"),
        onclick: () => openSource({ path: occurrence.path, line: occurrence.start, language }),
      },
      h("span", { class: "mono" }, `${occurrence.path}:${occurrence.start}–${occurrence.end}`),
      h("span", { class: "muted" }, occurrence.function),
    ),
  );

const blockItem = (block) =>
  h(
    "li",
    { class: "pblock" },
    h(
      "div",
      { class: "pblock-head" },
      h("strong", {}, t("pblocks.block", { n: block.id })),
      h(
        "span",
        { class: "muted" },
        t("pblocks.blockMeta", {
          language: block.language,
          lines: fmt(block.lines),
          tokens: fmt(block.tokens),
          n: block.occurrences.length,
        }),
      ),
    ),
    h(
      "ul",
      { class: "pblock-places", "aria-label": t("pblocks.places", { n: block.id }) },
      block.occurrences.map((occurrence) => occurrenceRow(occurrence, block.language)),
    ),
  );

// What keeps the list from being the whole answer; an empty list with none of these is a real zero.
function limits(state) {
  const out = [];
  if (state.budget.exhausted) {
    out.push(note(t("pblocks.budget", { budget: state.budget.exhausted }), "warn", "alert"));
  }
  if (state.indexCoverage === "partial") {
    out.push(
      note(
        t("pblocks.indexPartial", { syntax: state.indexSyntaxErrors, read: state.indexReadFailures }),
        "warn",
        "alert",
      ),
    );
  } else if (state.indexCoverage === "unknown") {
    out.push(note(t("pblocks.indexUnknown"), "warn", "alert"));
  }
  if (state.skipped > 0) out.push(note(t("pblocks.skipped", { n: state.skipped }), "warn", "alert"));
  return out;
}

/** The panel's element and `destroy`, which stops its polling. */
export function partialBlocksPanel() {
  const body = h("div", { class: "pblocks-body", "aria-live": "polite" });
  const el = h(
    "section",
    { class: "card pblocks", "aria-labelledby": "pblocks-title" },
    h("h2", { id: "pblocks-title" }, t("pblocks.title")),
    h("p", { class: "muted" }, t("pblocks.subtitle")),
    body,
  );
  let timer = null;
  let closed = false;

  const runButton = (label, cls = "btn primary") =>
    h("button", { class: cls, onclick: start }, icon("search", 16), label);

  function draw(state, wait = POLL_MS) {
    clearTimeout(timer);
    clear(body);
    if (state.status === "running") {
      body.append(loading(t("pblocks.running")));
      timer = setTimeout(() => show(), wait);
    } else if (state.status === "done") {
      drawDone(state);
    } else if (state.status === "outdated") {
      body.append(note(t("pblocks.outdated"), "warn", "alert"), runButton(t("pblocks.again")));
    } else if (state.status === "failed") {
      body.append(errorBox(new Error(state.error || t("pblocks.failed")), start));
    } else {
      body.append(runButton(t("pblocks.run")));
    }
  }

  function drawDone(state) {
    const partial = limits(state);
    body.append(
      h(
        "div",
        { class: "toolbar pblocks-summary" },
        h(
          "span",
          {},
          t(partial.length ? "pblocks.foundPartial" : "pblocks.found", {
            n: fmt(state.total),
            functions: fmt(state.budget.functionsSearched),
            time: duration(state.elapsedMs),
          }),
        ),
        runButton(t("pblocks.again"), "btn sm"),
      ),
      ...partial,
    );
    if (state.total === 0) {
      body.append(
        emptyState({
          title: t(partial.length ? "pblocks.noneSoFar" : "pblocks.none"),
          text: t("pblocks.noneText", { lines: state.budget.minLines, tokens: state.budget.minTokens }),
        }),
      );
      return;
    }
    body.append(note(t("pblocks.warning")), h("ol", { class: "pblock-list" }, state.items.map(blockItem)));
    const pages = pager({
      page: state.page,
      size: state.size,
      total: state.total,
      onPage: (page) => show(page),
    });
    if (pages) body.append(pages);
  }

  // Reads the search's state; a page of a finished search is cut from what the server kept.
  async function show(page = 0) {
    try {
      const state = await api("partial-blocks", { page: page || null }, KEY);
      if (!closed) draw(state);
    } catch (error) {
      if (!isAbort(error) && !closed) clear(body).append(errorBox(error, () => show(page)));
    }
  }

  async function start() {
    clear(body).append(loading(t("pblocks.running")));
    try {
      const state = await startPartialBlocks();
      if (!closed) draw(state, FIRST_POLL_MS);
    } catch (error) {
      if (!closed) clear(body).append(errorBox(error, start));
    }
  }

  show();
  return {
    el,
    destroy() {
      closed = true;
      clearTimeout(timer);
      cancelApi(KEY);
    },
  };
}
