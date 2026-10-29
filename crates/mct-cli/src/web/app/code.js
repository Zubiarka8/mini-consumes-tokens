// Code display: light syntax highlighting, line-numbered blocks, source dialog.
// The review workspace (web-ui/src/code/tokenize.ts) keeps a copy of the highlighter.
import { h, api, openDialog, copy, clear } from "./core.js";
import { icon, loading, errorBox } from "./components.js";
import { t } from "./i18n.js";

const KEYWORDS = new Set(
  `fn let mut pub use mod struct enum impl trait for while loop if else elif match return const static self Self super crate async await move as in is not and or
function var class extends implements interface new import export from default def lambda yield pass with try except catch finally raise throw throws final public private protected
package namespace using void int bool true false null None True False nil undefined this typeof instanceof switch case break continue do where type readonly abstract override
echo fi then done esac local select begin end module require`.split(/\s+/),
);
const HASH_LANGS = new Set(["python", "bash", "powershell", "ruby", "yaml", "toml", "markdown"]);
const DASH_LANGS = new Set(["lua", "sql"]);

// Tokenizes one line; `state.block` carries multi-line block comments across lines.
function tokenize(line, language, state) {
  const out = [];
  const hash = HASH_LANGS.has(language);
  const dash = DASH_LANGS.has(language);
  let plain = "";
  let i = 0;
  const flush = () => {
    if (plain) {
      out.push(plain);
      plain = "";
    }
  };
  const span = (cls, text) => {
    flush();
    out.push(h("span", { class: cls }, text));
  };
  while (i < line.length) {
    const rest = line.slice(i);
    if (state.block) {
      const end = rest.indexOf("*/");
      if (end < 0) {
        span("tk-c", rest);
        i = line.length;
      } else {
        span("tk-c", rest.slice(0, end + 2));
        i += end + 2;
        state.block = false;
      }
      continue;
    }
    if (!hash && !dash && rest.startsWith("/*")) {
      state.block = true;
      continue;
    }
    if (
      (hash && line[i] === "#") ||
      (dash && rest.startsWith("--")) ||
      (!hash && !dash && rest.startsWith("//"))
    ) {
      span("tk-c", rest);
      break;
    }
    const ch = line[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      let j = i + 1;
      while (j < line.length && line[j] !== ch) j += line[j] === "\\" ? 2 : 1;
      span("tk-s", line.slice(i, Math.min(j + 1, line.length)));
      i = j + 1;
      continue;
    }
    const number = /^\d[\d_.xa-fA-F]*/.exec(rest);
    if (number && !/\w/.test(line[i - 1] || "")) {
      span("tk-n", number[0]);
      i += number[0].length;
      continue;
    }
    const word = /^[A-Za-z_][\w]*/.exec(rest);
    if (word) {
      const text = word[0];
      if (KEYWORDS.has(text)) span("tk-k", text);
      else if (rest[text.length] === "(" || rest[text.length] === "!") span("tk-f", text);
      else if (/^[A-Z]/.test(text) && /[a-z]/.test(text)) span("tk-t", text);
      else plain += text;
      i += text.length;
      continue;
    }
    plain += ch;
    i += 1;
  }
  flush();
  if (state.block && !out.length) out.push(h("span", { class: "tk-c" }, line));
  return out;
}

/** A line-numbered code block. `marks` maps line number → extra CSS class. */
export function codeBlock({
  lines,
  start = 1,
  language = "",
  marks = {},
  label = t("code.label"),
  maxHeight,
}) {
  const state = { block: false };
  const body = h(
    "div",
    {
      class: "code",
      role: "region",
      "aria-label": label,
      tabindex: 0,
      style: maxHeight ? { maxHeight } : undefined,
    },
    lines.map((text, index) =>
      h(
        "div",
        { class: `ln ${marks[start + index] || ""}` },
        h("span", { "aria-hidden": "true" }, start + index),
        h("span", {}, tokenize(text, language, state)),
      ),
    ),
  );
  return body;
}

/** Modal with a paged view of an indexed file around `line`. */
export function openSource({ path, line = 1, language = "" }) {
  const size = 100;
  return openDialog(
    (close) => {
      const body = h("div", {});
      let start = Math.max(1, line - 20);
      const show = async () => {
        clear(body).append(loading());
        try {
          const data = await api("source", { path, start, end: start + size - 1 }, "source");
          clear(body);
          body.append(
            h(
              "div",
              { class: "row spread mb-2" },
              h(
                "span",
                { class: "muted small" },
                t("source.range", { from: data.start, to: data.start + data.lines.length - 1 }),
              ),
              h(
                "div",
                { class: "row" },
                h(
                  "button",
                  {
                    class: "btn sm",
                    disabled: start <= 1,
                    onclick: () => {
                      start = Math.max(1, start - size);
                      show();
                    },
                  },
                  t("source.prev"),
                ),
                h(
                  "button",
                  {
                    class: "btn sm",
                    disabled: data.lines.length < size,
                    onclick: () => {
                      start += size;
                      show();
                    },
                  },
                  t("source.next"),
                ),
              ),
            ),
            codeBlock({
              lines: data.lines,
              start: data.start,
              language: data.language || language,
              marks: { [line]: "hl" },
              maxHeight: "60vh",
              label: path,
            }),
          );
        } catch (error) {
          clear(body).append(errorBox(error, show));
        }
      };
      show();
      return h(
        "div",
        { class: "dlg" },
        h(
          "div",
          { class: "row spread" },
          h("h2", { class: "ellipsis mono source-title" }, `${path}:${line}`),
          h(
            "div",
            { class: "row" },
            h(
              "button",
              { class: "btn sm", onclick: () => copy(`${path}:${line}`, t("common.pathCopied")) },
              icon("copy", 14),
              t("common.copyPath"),
            ),
            h(
              "button",
              { class: "icon-btn", "aria-label": t("common.close"), onclick: () => close(null) },
              icon("x"),
            ),
          ),
        ),
        body,
      );
    },
    { wide: true },
  );
}
