// 10 · Configuración: theme, local server facts, backend capability matrix, local data
import { h, store, applyTheme, toast, confirmDialog, dateTime, saveIdle } from "./core.js";
import { power, MINUTES, MAX_MINUTES } from "./power.js";
import { icon, pageHead, badge, note } from "./components.js";
import { ignoreCard } from "./ignore-card.js";
import { t, getLang, setLang, LANGUAGES } from "./i18n.js";

const CAPABILITIES = [
  ["symbols", () => true],
  ["duplicates", (c) => c.duplicates],
  ["deadCode", (c) => c.deadCode],
  ["reviews", (c) => c.reviews],
  ["history", (c) => c.history],
  ["importGraph", (c) => c.dependencies.imports],
  ["callGraph", (c) => c.dependencies.calls],
  ["cycles", (c) => c.dependencies.cycles],
  ["keyword", (c) => c.search.keyword],
  ["literals", (c) => c.search.literals],
  ["semantic", (c) => c.search.semantic],
  ["obsidianRead", (c) => c.obsidian.read],
  ["obsidianEdit", (c) => c.obsidian.edit],
  ["obsidianOpen", (c) => c.obsidian.openInObsidian],
  ["reindex", (c) => c.reindex],
  ["tokens", (c) => c.tokens],
  ["similarity", (c) => c.similarityScore],
  ["confidence", (c) => c.confidence],
  ["severity", (c) => c.severity],
  ["reviewers", (c) => c.reviewers],
];

const label = (minutes) =>
  minutes === 0
    ? t("power.never")
    : minutes % 60 === 0 && minutes >= 60
      ? t("power.hours", { n: minutes / 60 })
      : t("power.minutes", { n: minutes });

/** Ready-made times as a segmented choice, plus "Custom" with a number of minutes. A sentence under
 *  it says what the current choice does, so nobody has to guess what "30" means. */
function durationPicker({ name, labelledBy, onChange, summary }) {
  const presets = [0, ...MINUTES];
  const current = () => power.get(name);
  const isCustom = () => !presets.includes(current());
  const group = h("div", { class: "choice-group", role: "radiogroup", "aria-labelledby": labelledBy });
  const input = h("input", {
    class: "input narrow",
    type: "number",
    min: "1",
    max: String(MAX_MINUTES),
    step: "1",
    inputmode: "numeric",
    "aria-label": t("power.customLabel"),
  });
  const custom = h("div", { class: "row" }, input, h("span", { class: "muted small" }, t("power.minutesUnit")));
  const note = h("p", { class: "muted small", "aria-live": "polite" });
  const choice = (value, text) =>
    h("button", {
      type: "button",
      class: "choice",
      role: "radio",
      "data-value": value,
      onclick: () => {
        if (value === "custom") {
          custom.hidden = false;
          input.focus();
          paint("custom");
        } else apply(Number(value));
      },
    }, text);
  const buttons = [...presets.map((m) => choice(String(m), label(m))), choice("custom", t("power.custom"))];
  group.append(...buttons);
  function paint(forced) {
    const active = forced || (isCustom() ? "custom" : String(current()));
    for (const button of buttons) {
      const on = button.dataset.value === active;
      button.setAttribute("aria-checked", String(on));
      // One tab stop for the group; arrows move inside it.
      button.tabIndex = on ? 0 : -1;
    }
    custom.hidden = active !== "custom";
    if (active === "custom" && isCustom()) input.value = String(current());
    note.textContent = summary(current());
  }
  function apply(minutes) {
    power.set(name, minutes);
    paint();
    onChange(minutes);
  }
  group.addEventListener("keydown", (event) => {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (!step) return;
    event.preventDefault();
    const at = buttons.findIndex((b) => b === document.activeElement);
    const next = buttons[(at + step + buttons.length) % buttons.length];
    next.focus();
    next.click();
  });
  input.addEventListener("change", () => {
    const minutes = Math.round(Number(input.value));
    if (minutes >= 1 && minutes <= MAX_MINUTES) apply(minutes);
    else {
      toast(t("power.customInvalid", { max: MAX_MINUTES }), "error");
      paint();
    }
  });
  paint();
  return h("div", { class: "stack-sm" }, group, custom, note);
}

export function render({ root, meta }) {
  const theme = store.get("theme", "system");
  const caps = h(
    "tbody",
    {},
    CAPABILITIES.map(([id, test]) =>
      h(
        "tr",
        {},
        h("td", {}, t(`cap.${id}`)),
        h("td", {}, test(meta.capabilities) ? badge(t("cap.yes"), "success") : badge(t("cap.no"), "")),
        h("td", { class: "muted" }, t(`cap.${id}.note`)),
      ),
    ),
  );
  const presets = store.get("presets", []);
  const appearance = h(
    "section",
    { class: "card" },
    h("h2", {}, t("settings.appearance")),
    h(
      "div",
      { role: "radiogroup", "aria-label": t("theme.label"), class: "stack" },
      [
        ["dark", "theme.dark"],
        ["light", "theme.light"],
        ["system", "theme.system"],
      ].map(([id, label]) =>
        h(
          "label",
          { class: "opt" },
          h("input", {
            type: "radio",
            name: "theme",
            checked: theme === id,
            onchange: () => {
              store.set("theme", id);
              applyTheme(id);
            },
          }),
          t(label),
        ),
      ),
    ),
    h(
      "label",
      { class: "field mt-3" },
      t("settings.language"),
      h(
        "select",
        { class: "select", onchange: (event) => setLang(event.target.value) },
        LANGUAGES.map(([id, label]) => h("option", { value: id, selected: id === getLang() }, label)),
      ),
    ),
    h("p", { class: "muted small" }, t("settings.languageNote")),
  );
  const stopSummary = (m) => (m === 0 ? t("power.stopNever") : t("power.stopAfter", { time: label(m) }));
  const pauseSummary = (m) => (m === 0 ? t("power.pauseNever") : t("power.pauseAfter", { time: label(m) }));
  const background = h(
    "section",
    { class: "card" },
    h("h2", {}, t("power.title")),
    h("p", { class: "muted mb-3" }, t("power.note")),
    h(
      "div",
      { class: "stack" },
      h(
        "div",
        { class: "setting" },
        h("h3", { id: "power-stop" }, t("power.stop")),
        h("p", { class: "muted small" }, t("power.stopHelp")),
        durationPicker({
          name: "stop",
          labelledBy: "power-stop",
          summary: stopSummary,
          onChange: (minutes) =>
            saveIdle(minutes).then(
              () => toast(t("power.stopSaved"), "ok"),
              () => toast(t("power.stopFailed"), "error"),
            ),
        }),
      ),
      h(
        "div",
        { class: "setting" },
        h("h3", { id: "power-pause" }, t("power.pause")),
        h("p", { class: "muted small" }, t("power.pauseHelp")),
        durationPicker({
          name: "pause",
          labelledBy: "power-pause",
          summary: pauseSummary,
          onChange: () => toast(t("power.pauseSaved"), "ok"),
        }),
      ),
    ),
  );
  const server = h(
    "section",
    { class: "card" },
    h("h2", {}, t("settings.server")),
    h(
      "dl",
      { class: "kv" },
      h("dt", {}, t("header.repo")),
      h("dd", {}, meta.project),
      h("dt", {}, t("settings.snapshot")),
      h("dd", {}, dateTime(meta.generated)),
      h("dt", {}, t("settings.lastIndexed")),
      h("dd", {}, dateTime(meta.indexing.lastIndexedAt)),
      h("dt", {}, t("settings.binding")),
      h("dd", {}, location.host),
      h("dt", {}, t("settings.reviewsFile")),
      h("dd", { class: "mono" }, meta.reviewsFile || "—"),
      h("dt", {}, t("settings.vault")),
      h("dd", {}, t("settings.vaultNotes", { n: meta.vault.notes })),
    ),
    note(t("settings.readOnly"), "", "lock"),
  );
  const localData = h(
    "section",
    { class: "card" },
    h("h2", {}, t("settings.localData")),
    h("p", { class: "muted" }, t("settings.localDataText", { n: presets.length })),
    h(
      "div",
      { class: "row mt-3" },
      h(
        "button",
        {
          class: "btn",
          onclick: async () => {
            if (
              await confirmDialog({
                title: t("settings.resetTitle"),
                body: t("settings.resetBody"),
                confirm: t("settings.reset"),
                danger: true,
              })
            ) {
              for (const k of ["filters", "presets", "recent-searches", "saved-searches", "recent-items"])
                store.set(k, k === "filters" ? {} : []);
              toast(t("settings.resetDone"), "ok");
            }
          },
        },
        icon("trash", 14),
        t("settings.reset"),
      ),
    ),
  );
  // The reference tables stay folded: they are long and nobody changes them.
  const capabilities = h(
    "details",
    { class: "card fold mt-4" },
    h("summary", {}, h("h2", {}, t("settings.capabilities"))),
    h("p", { class: "muted mb-3" }, t("settings.capabilitiesNote")),
    h(
      "div",
      { class: "table-wrap" },
      h(
        "table",
        { class: "t" },
        h(
          "thead",
          {},
          h(
            "tr",
            {},
            h("th", {}, t("settings.capability")),
            h("th", {}, t("settings.status")),
            h("th", {}, t("settings.detail")),
          ),
        ),
        caps,
      ),
    ),
  );
  const shortcuts = h(
    "details",
    { class: "card fold mt-4" },
    h("summary", {}, h("h2", {}, t("settings.shortcuts"))),
    h(
      "dl",
      { class: "kv" },
      [
        ["Ctrl/⌘ K", "settings.sc.search"],
        ["/", "settings.sc.slash"],
        ["↑ ↓ Enter", "settings.sc.list"],
        ["Esc", "settings.sc.esc"],
      ].map(([k, d]) => [h("dt", {}, h("span", { class: "kbd" }, k)), h("dd", {}, t(d))]),
    ),
  );
  // What a person changes comes first; what only describes the server comes after.
  root.append(
    pageHead(t("settings.title"), t("settings.subtitle")),
    h("div", { class: "grid two top" }, appearance, background),
    h("div", { class: "grid two top mt-4" }, server, localData),
    h("div", { class: "mt-4" }, ignoreCard()),
    capabilities,
    shortcuts,
  );
}
