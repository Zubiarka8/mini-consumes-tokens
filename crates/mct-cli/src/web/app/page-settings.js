// 10 · Configuración: theme, local server facts, backend capability matrix, local data
import {
  h,
  clear,
  store,
  applyTheme,
  toast,
  confirmDialog,
  dateTime,
  api,
  saveIgnore,
  ApiError,
} from "./core.js";
import { icon, pageHead, badge, note } from "./components.js";
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

const hasDirective = (text, directive) => text.split("\n").some((line) => line.trim() === directive);
const withDirective = (text, directive, on) => {
  const lines = text.split("\n").filter((line) => line.trim() !== directive);
  if (!on) return lines.join("\n");
  while (lines.length && lines[lines.length - 1] === "") lines.pop();
  return `${lines.join("\n")}${lines.length ? "\n" : ""}${directive}\n`;
};

/** View and edit the project's .mctignore; the file on disk stays the single source of truth. */
function ignoreCard() {
  const status = h("div", { class: "row wrap gap-2 mb-2" });
  const area = h("textarea", {
    class: "input wide mono code-input",
    rows: 14,
    spellcheck: false,
    "aria-label": t("ignore.fieldLabel"),
  });
  const meter = h("span", { class: "muted small", "aria-live": "polite" });
  const toggle = h("button", { class: "btn", type: "button" });
  const templateBtn = h("button", { class: "btn", type: "button" }, t("ignore.template"));
  const revert = h("button", { class: "btn", type: "button" }, t("ignore.revert"));
  const save = h("button", { class: "btn primary", type: "button" }, t("ignore.save"));
  const message = h("div", { "aria-live": "polite" });
  const card = h(
    "section",
    { class: "card mt-4" },
    h("h2", {}, t("ignore.title")),
    h("p", { class: "muted mb-3" }, t("ignore.intro")),
    message,
  );
  let state = null;
  let base = "";
  const dirty = () => area.value !== base;
  const paint = () => {
    const imports = hasDirective(area.value, state.directive);
    clear(status).append(
      imports ? badge(t("ignore.importOn"), "success") : badge(t("ignore.importOff"), ""),
      dirty() && badge(t("ignore.dirty"), "warning"),
    );
    toggle.replaceChildren(icon("filter", 14), imports ? t("ignore.toggleOff") : t("ignore.toggleOn"));
    meter.textContent = t("ignore.size", {
      n: new TextEncoder().encode(area.value).length,
      max: state.maxBytes,
    });
    save.disabled = !dirty();
    revert.disabled = !dirty();
  };
  const load = (next, text = next.content) => {
    state = next;
    base = next.content;
    area.value = text;
    paint();
  };
  area.addEventListener("input", paint);
  toggle.addEventListener("click", () => {
    area.value = withDirective(area.value, state.directive, !hasDirective(area.value, state.directive));
    paint();
    area.focus();
  });
  templateBtn.addEventListener("click", () => {
    area.value = state.template;
    paint();
    area.focus();
  });
  revert.addEventListener("click", () => {
    area.value = base;
    paint();
  });
  save.addEventListener("click", async () => {
    save.disabled = true;
    try {
      load(await saveIgnore(area.value, base));
      toast(t("ignore.saved"), "ok");
    } catch (error) {
      if (error instanceof ApiError && error.status === 409 && error.body?.content !== undefined) {
        load(error.body);
        toast(t("ignore.conflict"), "warn", 9000);
      } else toast(error.message, "err", 9000);
      paint();
    }
  });
  api("ignore")
    .then((next) => {
      load(next, next.exists ? next.content : "");
      card.append(
        note(t("ignore.gitignoreTip", { directive: next.directive, command: next.command }), "", "info"),
        next.exists ? null : h("p", { class: "muted small mt-2" }, t("ignore.noFile")),
        next.gitignorePresent ? null : h("p", { class: "muted small mt-2" }, t("ignore.noGitignore")),
        h(
          "div",
          { class: "mt-3" },
          status,
          h("label", { class: "field" }, t("ignore.fieldLabel"), area),
          meter,
        ),
        h("div", { class: "row wrap mt-3 gap-2" }, save, revert, toggle, next.exists ? null : templateBtn),
        h("p", { class: "muted small mt-3" }, t("ignore.reindex")),
      );
    })
    .catch((error) => {
      clear(message).append(note(`${t("ignore.loadFailed")} ${error.message}`, "bad", "alert"));
    });
  return card;
}

export function render({ root, meta }) {
  const theme = store.get("theme", "dark");
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
