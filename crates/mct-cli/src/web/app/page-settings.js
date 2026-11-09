// 10 · Configuración: theme, local server facts, backend capability matrix, local data
import { h, store, applyTheme, toast, confirmDialog, dateTime, saveSettings } from "./core.js";
import { power, MINUTES, MAX_MINUTES } from "./power.js";
import { persist } from "./server-settings.js";
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

/** A list of ready-made times plus "Custom" (a number of minutes), on one line. A sentence under it
 *  says what the current choice does. */
function durationPicker({ name, id, summary, onChange }) {
  const presets = [0, ...MINUTES];
  const current = () => power.get(name);
  const isCustom = () => !presets.includes(current());
  const select = h(
    "select",
    { class: "select", id },
    presets.map((m) => h("option", { value: String(m) }, label(m))),
    h("option", { value: "custom" }, t("power.custom")),
  );
  const input = h("input", {
    class: "input narrow",
    type: "number",
    min: "1",
    max: String(MAX_MINUTES),
    step: "1",
    inputmode: "numeric",
    "aria-label": t("power.customLabel"),
  });
  const unit = h("span", { class: "muted small" }, t("power.minutesUnit"));
  const note = h("p", { class: "muted small", "aria-live": "polite" });
  function paint(forced) {
    const active = forced || (isCustom() ? "custom" : String(current()));
    select.value = active;
    const custom = active === "custom";
    input.hidden = unit.hidden = !custom;
    if (custom && isCustom()) input.value = String(current());
    note.textContent = summary(current());
  }
  function apply(minutes) {
    power.set(name, minutes);
    paint();
    onChange(minutes);
  }
  select.addEventListener("change", () => {
    if (select.value === "custom") {
      paint("custom");
      input.focus();
    } else apply(Number(select.value));
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
  return h("div", { class: "stack-sm" }, h("div", { class: "row" }, select, input, unit), note);
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
              persist({ theme: id });
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
        {
          class: "select",
          // The file first: the page reloads as soon as the language changes.
          onchange: async (event) => {
            const lang = event.target.value;
            await persist({ lang });
            setLang(lang);
          },
        },
        LANGUAGES.map(([id, label]) => h("option", { value: id, selected: id === getLang() }, label)),
      ),
    ),
    h("p", { class: "muted small" }, t("settings.languageNote")),
  );
  const stopSummary = (m) => (m === 0 ? t("power.stopNever") : t("power.stopAfter", { time: label(m) }));
  const pauseSummary = (m) => (m === 0 ? t("power.pauseNever") : t("power.pauseAfter", { time: label(m) }));
  const mcpSummary = (m) => (m === 0 ? t("power.mcpNever") : t("power.mcpAfter", { time: label(m) }));
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
        h("label", { class: "setting-title", for: "power-stop" }, t("power.stop")),
        h("p", { class: "muted small" }, t("power.stopHelp")),
        durationPicker({
          name: "stop",
          id: "power-stop",
          summary: stopSummary,
          onChange: (minutes) =>
            saveSettings({ stop: minutes }).then(
              () => toast(t("power.stopSaved"), "ok"),
              () => toast(t("power.stopFailed"), "error"),
            ),
        }),
      ),
      h(
        "div",
        { class: "setting" },
        h("label", { class: "setting-title", for: "power-pause" }, t("power.pause")),
        h("p", { class: "muted small" }, t("power.pauseHelp")),
        durationPicker({
          name: "pause",
          id: "power-pause",
          summary: pauseSummary,
          onChange: (minutes) => {
            persist({ pause: minutes });
            toast(t("power.pauseSaved"), "ok");
          },
        }),
      ),
      h(
        "div",
        { class: "setting" },
        h("label", { class: "setting-title", for: "power-mcp" }, t("power.mcp")),
        h("p", { class: "muted small" }, t("power.mcpHelp")),
        durationPicker({
          name: "mcp",
          id: "power-mcp",
          summary: mcpSummary,
          onChange: (minutes) => {
            persist({ mcp: minutes });
            toast(t("power.mcpSaved"), "ok");
          },
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
    h("div", { class: "grid two top" }, h("div", { class: "stack" }, appearance, localData), h("div", { class: "stack" }, background, server)),
    h("div", { class: "mt-4" }, ignoreCard()),
    capabilities,
    shortcuts,
  );
}
