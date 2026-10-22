// Code display: light syntax highlighting, line-numbered blocks, aligned diff, source dialog.
import { h, api, openDialog, copy, clear } from "./core.js";
import { icon, loading, errorBox } from "./components.js";
import { t } from "./i18n.js";

const KEYWORDS = new Set(`fn let mut pub use mod struct enum impl trait for while loop if else elif match return const static self Self super crate async await move as in is not and or
function var class extends implements interface new import export from default def lambda yield pass with try except catch finally raise throw throws final public private protected
package namespace using void int bool true false null None True False nil undefined this typeof instanceof switch case break continue do where type readonly abstract override
echo fi then done esac local select begin end module require`.split(/\s+/));
const HASH_LANGS = new Set(["python", "bash", "powershell", "ruby", "yaml", "toml", "markdown"]);
const DASH_LANGS = new Set(["lua", "sql"]);

// Tokenizes one line; `state.block` carries multi-line block comments across lines.
function tokenize(line, language, state) {
  const out = [];
  const hash = HASH_LANGS.has(language), dash = DASH_LANGS.has(language);
  let plain = "", i = 0;
  const flush = () => { if (plain) { out.push(plain); plain = ""; } };
  const span = (cls, text) => { flush(); out.push(h("span", { class: cls }, text)); };
  while (i < line.length) {
    const rest = line.slice(i);
    if (state.block) {
      const end = rest.indexOf("*/");
      if (end < 0) { span("tk-c", rest); i = line.length; } else { span("tk-c", rest.slice(0, end + 2)); i += end + 2; state.block = false; }
      continue;
    }
    if (!hash && !dash && rest.startsWith("/*")) { state.block = true; continue; }
    if ((hash && line[i] === "#") || (dash && rest.startsWith("--")) || (!hash && !dash && rest.startsWith("//"))) { span("tk-c", rest); break; }
    const ch = line[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      let j = i + 1;
      while (j < line.length && line[j] !== ch) j += line[j] === "\\" ? 2 : 1;
      span("tk-s", line.slice(i, Math.min(j + 1, line.length))); i = j + 1; continue;
    }
    const number = /^\d[\d_.xa-fA-F]*/.exec(rest);
    if (number && !/\w/.test(line[i - 1] || "")) { span("tk-n", number[0]); i += number[0].length; continue; }
    const word = /^[A-Za-z_][\w]*/.exec(rest);
    if (word) {
      const text = word[0];
      if (KEYWORDS.has(text)) span("tk-k", text);
      else if (rest[text.length] === "(" || rest[text.length] === "!") span("tk-f", text);
      else if (/^[A-Z]/.test(text) && /[a-z]/.test(text)) span("tk-t", text);
      else plain += text;
      i += text.length; continue;
    }
    plain += ch; i += 1;
  }
  flush();
  if (state.block && !out.length) out.push(h("span", { class: "tk-c" }, line));
  return out;
}

/** A line-numbered code block. `marks` maps line number → extra CSS class. */
export function codeBlock({ lines, start = 1, language = "", marks = {}, label = t("code.label"), maxHeight }) {
  const state = { block: false };
  const body = h("div", { class: "code", role: "region", "aria-label": label, tabindex: 0, style: maxHeight ? { maxHeight } : undefined },
    lines.map((text, index) => h("div", { class: `ln ${marks[start + index] || ""}` }, h("span", { "aria-hidden": "true" }, start + index), h("span", {}, tokenize(text, language, state)))));
  return body;
}

// ---------- diff ----------
const IDENT = /[A-Za-z_]\w*/g;
export function normalizer({ ignoreSpace, ignoreComments, normalizeIds }) {
  return (line) => {
    let text = line;
    if (ignoreComments) text = text.replace(/\s*(\/\/|#).*$/, "");
    if (normalizeIds) text = text.replace(IDENT, (word) => (KEYWORDS.has(word) ? word : "ID"));
    if (ignoreSpace) text = text.replace(/\s+/g, " ").trim();
    return text;
  };
}

/** Aligns two line arrays with an LCS; adjacent del+add runs become paired `change` rows. */
export function alignLines(a, b, normalize = (x) => x) {
  const na = a.map(normalize), nb = b.map(normalize);
  const n = na.length, m = nb.length;
  const table = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
  for (let i = n - 1; i >= 0; i -= 1) for (let j = m - 1; j >= 0; j -= 1) {
    table[i][j] = na[i] === nb[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
  }
  const rows = [];
  let i = 0, j = 0;
  const dels = [], adds = [];
  const flushChanges = () => {
    const paired = Math.min(dels.length, adds.length);
    for (let k = 0; k < paired; k += 1) rows.push({ kind: "change", a: dels[k], b: adds[k] });
    for (let k = paired; k < dels.length; k += 1) rows.push({ kind: "del", a: dels[k], b: null });
    for (let k = paired; k < adds.length; k += 1) rows.push({ kind: "add", a: null, b: adds[k] });
    dels.length = 0; adds.length = 0;
  };
  while (i < n || j < m) {
    if (i < n && j < m && na[i] === nb[j]) { flushChanges(); rows.push({ kind: "same", a: i, b: j }); i += 1; j += 1; }
    else if (j >= m || (i < n && table[i + 1][j] >= table[i][j + 1])) { dels.push(i); i += 1; }
    else { adds.push(j); j += 1; }
  }
  flushChanges();
  return rows;
}

/** Two aligned panes whose scrolling is kept in sync. */
export function compareView({ left, right, normalize }) {
  const rows = alignLines(left.lines, right.lines, normalize);
  const pane = (side, info) => {
    const state = { block: false };
    const idx = side === "a" ? "a" : "b";
    const body = h("div", { class: "code", tabindex: 0, role: "region", "aria-label": `${t("compare.code")}: ${info.path}` },
      rows.map((row) => {
        const cls = row.kind === "same" ? "" : row.kind === "change" ? (side === "a" ? "del" : "add") : row.kind === "del" ? (side === "a" ? "del" : "gap") : (side === "b" ? "add" : "gap");
        const source = row[idx];
        const text = source === null ? "" : info.lines[source];
        return h("div", { class: `ln ${cls}` }, h("span", { "aria-hidden": "true" }, source === null ? "" : info.start + source), h("span", {}, source === null ? "" : tokenize(text, info.language, state)));
      }));
    return body;
  };
  const a = pane("a", left), b = pane("b", right);
  let lock = false;
  const sync = (from, to) => from.addEventListener("scroll", () => {
    if (lock) return;
    lock = true; to.scrollTop = from.scrollTop; to.scrollLeft = from.scrollLeft;
    requestAnimationFrame(() => { lock = false; });
  });
  sync(a, b); sync(b, a);
  const changes = rows.filter((row) => row.kind !== "same").length;
  return { a, b, changes, rows: rows.length };
}

/** Modal with a paged view of an indexed file around `line`. */
export function openSource({ path, line = 1, language = "" }) {
  const size = 100;
  return openDialog((close) => {
    const body = h("div", {});
    let start = Math.max(1, line - 20);
    const show = async () => {
      clear(body).append(loading());
      try {
        const data = await api("source", { path, start, end: start + size - 1 }, "source");
        clear(body);
        body.append(
          h("div", { class: "row spread", style: { marginBottom: "8px" } },
            h("span", { class: "muted small" }, t("source.range", { from: data.start, to: data.start + data.lines.length - 1 })),
            h("div", { class: "row" },
              h("button", { class: "btn sm", disabled: start <= 1, onclick: () => { start = Math.max(1, start - size); show(); } }, t("source.prev")),
              h("button", { class: "btn sm", disabled: data.lines.length < size, onclick: () => { start += size; show(); } }, t("source.next")))),
          codeBlock({ lines: data.lines, start: data.start, language: data.language || language, marks: { [line]: "hl" }, maxHeight: "60vh", label: path }));
      } catch (error) { clear(body).append(errorBox(error, show)); }
    };
    show();
    return h("div", { class: "dlg" },
      h("div", { class: "row spread" }, h("h2", { class: "ellipsis mono", style: { fontSize: "14px" } }, `${path}:${line}`),
        h("div", { class: "row" }, h("button", { class: "btn sm", onclick: () => copy(`${path}:${line}`, t("common.pathCopied")) }, icon("copy", 14), t("common.copyPath")),
          h("button", { class: "icon-btn", "aria-label": t("common.close"), onclick: () => close(null) }, icon("x")))), body);
  }, { wide: true });
}
