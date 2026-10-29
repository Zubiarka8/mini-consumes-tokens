// The .mctignore and .gitignore editors, shared by Configuración and the Archivos workspace.
import { h, clear, toast, api, saveIgnore, ApiError } from "./core.js";
import { icon, badge, note } from "./components.js";
import { t } from "./i18n.js";

const hasDirective = (text, directive) => text.split("\n").some((line) => line.trim() === directive);
const withDirective = (text, directive, on) => {
  const lines = text.split("\n").filter((line) => line.trim() !== directive);
  if (!on) return lines.join("\n");
  while (lines.length && lines[lines.length - 1] === "") lines.pop();
  return `${lines.join("\n")}${lines.length ? "\n" : ""}${directive}\n`;
};

/**
 * View and edit a rule file at the project root: .mctignore (default) or, with `file` "gitignore",
 * .gitignore. The file on disk stays the single source of truth. Only .mctignore has the
 * @import-gitignore directive and a starter template.
 */
export function ignoreCard(file = "mctignore") {
  const git = file === "gitignore";
  const text = (key, values) => t(`${git ? "gitignore" : "ignore"}.${key}`, values);
  const status = h("div", { class: "row wrap gap-2 mb-2" });
  const area = h("textarea", {
    class: "input wide mono code-input",
    rows: 14,
    spellcheck: false,
    "aria-label": text("fieldLabel"),
  });
  const meter = h("span", { class: "muted small", "aria-live": "polite" });
  const toggle = h("button", { class: "btn", type: "button" });
  const templateBtn = h("button", { class: "btn", type: "button" }, t("ignore.template"));
  const revert = h("button", { class: "btn", type: "button" }, t("ignore.revert"));
  const save = h("button", { class: "btn primary", type: "button" }, text("save"));
  const message = h("div", { "aria-live": "polite" });
  // After a save the index rebuilds in the background; this says so until the page redraws with it.
  const waiting = h("div", { role: "status", "aria-live": "polite", class: "mt-3" });
  const card = h(
    "section",
    { class: "card mt-4" },
    h("h2", {}, text("title")),
    h("p", { class: "muted mb-3" }, text("intro")),
    message,
  );
  let state = null;
  let base = "";
  const dirty = () => area.value !== base;
  const paint = () => {
    const imports = !git && hasDirective(area.value, state.directive);
    clear(status).append(
      git ? null : imports ? badge(t("ignore.importOn"), "success") : badge(t("ignore.importOff"), ""),
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
  const load = (next, content = next.content) => {
    state = next;
    base = next.content;
    area.value = content;
    paint();
  };
  area.addEventListener("input", () => {
    clear(waiting);
    paint();
  });
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
      load(await saveIgnore(area.value, base, file));
      toast(text("saved"), "ok");
      clear(waiting).append(note(t("ignore.waiting"), "", "clock"));
      // The page redraws with the new index once the cursor leaves the text field.
      area.blur();
    } catch (error) {
      if (error instanceof ApiError && error.status === 409 && error.body?.content !== undefined) {
        clear(waiting);
        load(error.body);
        toast(text("conflict"), "warn", 9000);
      } else toast(error.message, "err", 9000);
      paint();
    }
  });
  api("ignore", { file })
    .then((next) => {
      load(next, next.exists ? next.content : "");
      card.append(
        git
          ? note(t("gitignore.tip"), "", "info")
          : note(t("ignore.gitignoreTip", { directive: next.directive, command: next.command }), "", "info"),
        next.exists ? null : h("p", { class: "muted small mt-2" }, text("noFile")),
        git || next.gitignorePresent ? null : h("p", { class: "muted small mt-2" }, t("ignore.noGitignore")),
        h("div", { class: "mt-3" }, status, h("label", { class: "field" }, text("fieldLabel"), area), meter),
        h(
          "div",
          { class: "row wrap mt-3 gap-2" },
          save,
          revert,
          git ? null : toggle,
          git || next.exists ? null : templateBtn,
        ),
        waiting,
        h("p", { class: "muted small mt-3" }, t("ignore.reindex")),
      );
    })
    .catch((error) => {
      clear(message).append(note(`${text("loadFailed")} ${error.message}`, "bad", "alert"));
    });
  return card;
}
