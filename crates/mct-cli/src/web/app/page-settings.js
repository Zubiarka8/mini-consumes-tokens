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

/** A list of ready-made times plus "Never" and a custom number of minutes. */
function durationPicker({ id, name, onChange }) {
  const current = () => power.get(name);
  const custom = () => ![0, ...MINUTES].includes(current());
  const select = h(
    "select",
    { class: "select", id },
    [0, ...MINUTES].map((m) => h("option", { value: String(m) }, label(m))),
    h("option", { value: "custom" }, t("power.custom")),
  );
  const input = h("input", {
    class: "input narrow",
    type: "number",
    min: "1",
    max: String(MAX_MINUTES),
    step: "1",
    "aria-label": t("power.customLabel"),
  });
  const show = () => {
    select.value = custom() ? "custom" : String(current());
    input.value = String(current() || 30);
    input.hidden = select.value !== "custom";
  };
  const apply = (minutes) => {
    power.set(name, minutes);
    show();
    onChange(minutes);
  };
  select.addEventListener("change", () => {
    if (select.value === "custom") {
      input.hidden = false;
      input.focus();
    } else apply(Number(select.value));
  });
  input.addEventListener("change", () => {
    const minutes = Math.round(Number(input.value));
    if (minutes >= 1 && minutes <= MAX_MINUTES) apply(minutes);
    else {
      toast(t("power.customInvalid", { max: MAX_MINUTES }), "error");
      show();
    }
  });
  show();
  return h("div", { class: "row" }, select, input, h("span", { class: "muted small" }, t("power.minutesUnit")));
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
  root.append(
    pageHead(t("settings.title"), t("settings.subtitle")),
    h(
      "div",
      { class: "grid two" },
      h(
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
      ),
      h(
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
      ),
    ),
    h(
      "section",
      { class: "card mt-4" },
      h("h2", {}, t("power.title")),
      h("p", { class: "muted mb-3" }, t("power.note")),
      h(
        "div",
        { class: "stack" },
        h(
          "div",
          { class: "field" },
          h("label", { for: "power-stop" }, t("power.stop")),
          durationPicker({
            id: "power-stop",
            name: "stop",
            onChange: (minutes) =>
              saveIdle(minutes).then(
                () => toast(t("power.stopSaved"), "ok"),
                () => toast(t("power.stopFailed"), "error"),
              ),
          }),
          h("p", { class: "muted small" }, t("power.stopHelp")),
        ),
        h(
          "div",
          { class: "field" },
          h("label", { for: "power-pause" }, t("power.pause")),
          durationPicker({ id: "power-pause", name: "pause", onChange: () => toast(t("power.pauseSaved"), "ok") }),
          h("p", { class: "muted small" }, t("power.pauseHelp")),
        ),
      ),
    ),
    h(
      "section",
      { class: "card mt-4" },
      h("h2", {}, t("settings.capabilities")),
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
    ),
    ignoreCard(),
    h(
      "div",
      { class: "grid two mt-4" },
      h(
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
      ),
      h(
        "section",
        { class: "card" },
        h("h2", {}, t("settings.shortcuts")),
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
      ),
    ),
  );
}
